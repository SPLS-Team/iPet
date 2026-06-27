mod app_error;
mod config;
mod http_safety;
mod llm_client;
mod secret;
mod storage;
#[cfg(test)]
mod testutil;
mod tool_dispatcher;
mod tool_package;
mod usage_tracker;

use app_error::{public_error, AppError, AppResult};
use config::{LlmSettingsInput, LlmSettingsStatus};
use ipet_tool_get_system_status::{SystemMonitor, SystemSnapshot};
use ipet_tool_scan_disk::{self as disk_scanner, DiskScanRequest, DiskScanResult, ScanCancellation};
use llm_client::{ChatRequest, ChatTurnResult, LlmClient};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use storage::{
    AppUsageStats, ChatRecord, ChatSession, Memory, PomodoroStats, Storage, TokenUsageStats,
    ToolConfig, ToolConfigInput,
};
use tauri::{Emitter, LogicalSize, Manager, State, Window};
use tokio::sync::Mutex;
use tool_dispatcher::ToolDispatcher;

// The builtin tools live in external crates (`ipet-tool-*`) with their own
// minimal `AppError`. Map them into the host error type so `?` works across
// the crate boundary without forcing the tool crates to depend on
// rusqlite/reqwest.
impl From<ipet_tool_scan_disk::AppError> for AppError {
    fn from(err: ipet_tool_scan_disk::AppError) -> Self {
        match err {
            ipet_tool_scan_disk::AppError::Io(e) => AppError::Io(e),
            ipet_tool_scan_disk::AppError::Json(e) => AppError::Json(e),
            ipet_tool_scan_disk::AppError::InvalidInput(msg) => AppError::InvalidInput(msg),
            ipet_tool_scan_disk::AppError::Cancelled => AppError::Cancelled,
        }
    }
}

pub struct AppState {
    storage: Arc<Storage>,
    system: Arc<Mutex<SystemMonitor>>,
    /// Active disk scans keyed by caller-provided id so they can be cancelled
    /// mid-flight. Entries are removed when the scan task exits.
    scans: Arc<StdMutex<HashMap<String, ScanCancellation>>>,
    /// The session new messages are written to / recent messages are read
    /// from. Frontend-driven `set_current_session` updates this; startup seeds
    /// it from `ensure_default_session` so there's always a valid id.
    current_session_id: Arc<StdMutex<i64>>,
}

impl AppState {
    /// Read the active session id. Panics only if the mutex is poisoned, which
    /// would indicate a panic in another command — fatal enough to surface.
    fn current_session(&self) -> i64 {
        *self.current_session_id.lock().expect("session mutex poisoned")
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatStreamEvent {
    request_id: String,
    kind: String,
    content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskScanProgress {
    scan_id: String,
    scanned_entries: u64,
}

#[tauri::command]
fn get_llm_settings(state: State<'_, AppState>) -> Result<LlmSettingsStatus, String> {
    get_llm_settings_inner(&state).map_err(public_error)
}

#[tauri::command]
fn save_llm_settings(
    input: LlmSettingsInput,
    state: State<'_, AppState>,
) -> Result<LlmSettingsStatus, String> {
    save_llm_settings_inner(input, &state).map_err(public_error)
}

#[tauri::command]
async fn get_system_status(
    process_limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<SystemSnapshot, String> {
    let mut monitor = state.system.lock().await;
    let snapshot = monitor.snapshot(process_limit.unwrap_or(10).clamp(3, 30));
    if let Ok(json) = serde_json::to_string(&snapshot) {
        if let Err(err) = state.storage.cache_system_sample(&json) {
            tracing::warn!(error = %err, "failed to cache system sample");
        }
    }
    Ok(snapshot)
}

#[tauri::command]
async fn scan_disk(
    window: Window,
    scan_id: Option<String>,
    request: DiskScanRequest,
    state: State<'_, AppState>,
) -> Result<DiskScanResult, String> {
    let scan_id = scan_id.unwrap_or_else(|| format!("scan-{}", uuid_like()));
    let cancellation = ScanCancellation::new();
    {
        let mut scans = state
            .scans
            .lock()
            .map_err(|_| "scan registry poisoned".to_string())?;
        scans.insert(scan_id.clone(), cancellation.clone());
    }

    // Build a progress emitter that ships the running entry count to the
    // frontend on the `disk-scan-progress` channel. The throttled callback in
    // disk_scanner makes sure we don't drown the IPC bus.
    let progress_window = window.clone();
    let progress_id = scan_id.clone();
    let on_progress: Box<dyn Fn(u64) + Send + Sync> = Box::new(move |scanned_entries| {
        let _ = progress_window.emit(
            "disk-scan-progress",
            DiskScanProgress {
                scan_id: progress_id.clone(),
                scanned_entries,
            },
        );
    });

    let scans_for_cleanup = state.scans.clone();
    let cleanup_id = scan_id.clone();
    let result = tokio::task::spawn_blocking(move || {
        disk_scanner::scan_path_with(request, cancellation, Some(on_progress))
    })
    .await
    .map_err(|error| error.to_string())
    .and_then(|result| result.map_err(|err| public_error(err.into())));

    // Drop the cancellation handle regardless of outcome so the registry
    // doesn't grow over a long session.
    if let Ok(mut scans) = scans_for_cleanup.lock() {
        scans.remove(&cleanup_id);
    }

    let result = result?;
    if let Ok(json) = serde_json::to_string(&result) {
        if let Err(err) = state.storage.cache_disk_scan(&result.root.path, &json) {
            tracing::warn!(error = %err, path = %result.root.path, "failed to cache disk scan");
        }
    }
    Ok(result)
}

#[tauri::command]
fn cancel_disk_scan(scan_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let scans = state
        .scans
        .lock()
        .map_err(|_| "scan registry poisoned".to_string())?;
    let Some(handle) = scans.get(&scan_id) else {
        return Ok(false);
    };
    handle.cancel();
    tracing::info!(scan_id = %scan_id, "disk scan cancellation requested");
    Ok(true)
}

#[tauri::command]
fn get_recent_messages(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<ChatRecord>, String> {
    let session_id = state.current_session();
    state
        .storage
        .recent_messages(session_id, limit.unwrap_or(40).clamp(1, 200))
        .map_err(public_error)
}

// --- sessions: multi-session chat management ------------------------------

#[tauri::command]
fn list_sessions(state: State<'_, AppState>) -> Result<Vec<ChatSession>, String> {
    state.storage.list_sessions().map_err(public_error)
}

#[tauri::command]
fn create_session(
    title: Option<String>,
    state: State<'_, AppState>,
) -> Result<ChatSession, String> {
    state
        .storage
        .create_session(title.as_deref())
        .map_err(public_error)
}

/// Switch the active session. Returns the resolved id so the frontend can
/// confirm; a non-existent id is rejected so the UI never points at a ghost.
#[tauri::command]
fn set_current_session(
    id: i64,
    state: State<'_, AppState>,
) -> Result<i64, String> {
    let session = state
        .storage
        .get_session(id)
        .map_err(public_error)?
        .ok_or_else(|| format!("会话不存在: {id}"))?;
    *state.current_session_id.lock().map_err(|_| "session mutex poisoned")? = session.id;
    Ok(session.id)
}

/// The session new messages currently land in. Frontend mirrors this in its
/// own state; this command reconciles after a restart or cross-window action.
#[tauri::command]
fn get_current_session(state: State<'_, AppState>) -> Result<i64, String> {
    Ok(state.current_session())
}

#[tauri::command]
fn rename_session(
    id: i64,
    title: String,
    state: State<'_, AppState>,
) -> Result<Option<ChatSession>, String> {
    state
        .storage
        .rename_session(id, &title)
        .map_err(public_error)
}

/// Delete a session and its messages. If the deleted session was the active
/// one, switch to whichever session is now most-recently-active (creating a
/// fresh one if the table is empty) so there's always a valid current id.
#[tauri::command]
fn delete_session(id: i64, state: State<'_, AppState>) -> Result<i64, String> {
    state.storage.delete_session(id).map_err(public_error)?;
    let was_current = state.current_session() == id;
    if was_current {
        let next = state
            .storage
            .ensure_default_session()
            .map_err(public_error)?;
        *state.current_session_id.lock().map_err(|_| "session mutex poisoned")? = next;
        Ok(next)
    } else {
        Ok(state.current_session())
    }
}

// --- memories: long-term memory management --------------------------------

#[tauri::command]
fn list_memories(state: State<'_, AppState>) -> Result<Vec<Memory>, String> {
    state.storage.list_memories().map_err(public_error)
}

#[tauri::command]
fn update_memory(
    id: i64,
    content: String,
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<Option<Memory>, String> {
    state
        .storage
        .update_memory(id, &content, category.as_deref())
        .map_err(public_error)
}

#[tauri::command]
fn delete_memory(id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    state.storage.delete_memory(id).map_err(public_error)
}

#[tauri::command]
fn list_tools(state: State<'_, AppState>) -> Result<Vec<ToolConfig>, String> {
    state.storage.list_tools().map_err(public_error)
}

#[tauri::command]
fn save_tool(input: ToolConfigInput, state: State<'_, AppState>) -> Result<ToolConfig, String> {
    state.storage.save_custom_tool(input).map_err(public_error)
}

#[tauri::command]
fn set_tool_enabled(
    name: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<ToolConfig, String> {
    state
        .storage
        .set_tool_enabled(&name, enabled)
        .map_err(public_error)
}

#[tauri::command]
fn delete_tool(name: String, state: State<'_, AppState>) -> Result<(), String> {
    state.storage.delete_tool(&name).map_err(public_error)
}

#[tauri::command]
fn import_tool_from_path(
    path: String,
    state: State<'_, AppState>,
) -> Result<ToolConfig, String> {
    let pkg = tool_package::load_package(std::path::Path::new(&path)).map_err(public_error)?;
    tracing::info!(
        tool = %pkg.input.name,
        version = pkg.version.as_deref().unwrap_or("-"),
        permissions = ?pkg.permissions,
        "importing tool package"
    );
    state.storage.save_custom_tool(pkg.input).map_err(public_error)
}

#[tauri::command]
fn get_token_stats(state: State<'_, AppState>) -> Result<TokenUsageStats, String> {
    state.storage.token_stats().map_err(public_error)
}

/// Aggregate foreground usage time for the Usage view. `range` is `today` /
/// `7d` / `30d`; defaults to `today`. Reads the same `app_usage` rows the
/// background sampler writes and the `app_usage_stats` tool queries.
#[tauri::command]
fn get_app_usage(
    range: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<AppUsageStats, String> {
    let range = range.as_deref().unwrap_or("today");
    let limit = limit.unwrap_or(15).clamp(1, 100);
    state
        .storage
        .app_usage_stats(range, limit)
        .map_err(public_error)
}

/// Record one completed pomodoro session (`kind` = `work` / `break`). Called by
/// the frontend on a non-skipped phase transition; ignored on failure so a DB
/// hiccup never blocks the timer.
#[tauri::command]
fn record_pomodoro_session(
    kind: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .storage
        .record_pomodoro_session(&kind)
        .map_err(public_error)
}

/// Aggregate completed pomodoro sessions for the Usage view.
#[tauri::command]
fn get_pomodoro_stats(
    range: Option<String>,
    state: State<'_, AppState>,
) -> Result<PomodoroStats, String> {
    let range = range.as_deref().unwrap_or("today");
    state.storage.pomodoro_stats(range).map_err(public_error)
}

/// Fetch the model list from the configured provider's `/models` endpoint.
/// Works against any OpenAI-compatible `GET {base_url}/models` (returns
/// `{ data: [{ id: "..." }, ...] }`). Used by the Model view's "刷新模型列表"
/// button so the user can pick from what the endpoint actually offers instead
/// of typing a model id blind. Requires a saved API key.
#[tauri::command]
async fn list_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    list_models_inner(&state).await.map_err(public_error)
}

async fn list_models_inner(state: &State<'_, AppState>) -> AppResult<Vec<String>> {
    let settings = state.storage.load_llm_settings()?;
    let api_key = settings
        .api_key
        .as_ref()
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| AppError::Config("请先保存 API Key 后再获取模型列表".to_string()))?;
    let base = settings.normalized_base_url();
    if base.is_empty() {
        return Err(AppError::Config("Base URL 不能为空".to_string()));
    }

    let url = format!("{base}/models");
    let response = reqwest::Client::new()
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|e| AppError::Model(format!("请求模型列表失败：{e}")))?
        .error_for_status()
        .map_err(|e| AppError::Model(format!("模型列表接口返回错误：{e}")))?;

    let body: ModelsResponse = response
        .json()
        .await
        .map_err(|e| AppError::Model(format!("解析模型列表失败：{e}")))?;
    let mut ids: Vec<String> = body.data.into_iter().map(|m| m.id).collect();
    ids.sort();
    Ok(ids)
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
}

#[tauri::command]
async fn send_chat_message(
    window: Window,
    request: ChatRequest,
    state: State<'_, AppState>,
) -> Result<(), String> {
    send_chat_message_inner(window, request, &state)
        .await
        .map_err(public_error)
}

#[tauri::command]
fn set_always_on_top(window: Window, enabled: bool) -> Result<(), String> {
    window
        .set_always_on_top(enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_mouse_passthrough(window: Window, enabled: bool) -> Result<(), String> {
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn minimize_window(window: Window) -> Result<(), String> {
    window.minimize().map_err(|error| error.to_string())
}

#[tauri::command]
fn close_window(window: Window) -> Result<(), String> {
    window.close().map_err(|error| error.to_string())
}

#[tauri::command]
fn start_window_drag(window: Window) -> Result<(), String> {
    window.start_dragging().map_err(|error| error.to_string())
}

#[tauri::command]
fn set_compact_window(window: Window, enabled: bool) -> Result<(), String> {
    // Compact mode shrinks to a small puck the user can drag around. Before
    // shrinking, record the current window size on the window's local state
    // so toggling back can restore exactly what the user had — instead of
    // snapping every expand back to the original 440x720 default.
    const STATE_KEY: &str = "ipet:expanded-size";

    // The expanded (talk/control) minimum matches the Control Center default
    // size: 720x720 keeps the sidebar nav visible and prevents the user from
    // shrinking the window below the layout's responsive breakpoint. Capsule
    // mode temporarily relaxes the minimum so the glass-pill size can be set.
    const EXPANDED_MIN: (f64, f64) = (720.0, 720.0);
    // The capsule is now a wide, short translucent pill rather than the old
    // 148x166 pet puck; the min lets it shrink to a one-line status pill.
    const CAPSULE_MIN: (f64, f64) = (160.0, 36.0);
    const CAPSULE_SIZE: (f64, f64) = (248.0, 46.0);

    if enabled {
        if let Ok(size) = window.outer_size() {
            if let Ok(scale) = window.scale_factor() {
                let logical = size.to_logical::<f64>(scale);
                // We tuck the previous size into the app's storage via the
                // window label; using the global state object keeps this
                // platform-agnostic without a new persistent table.
                let key = format!("{STATE_KEY}:{}", window.label());
                if let Some(app) = window.try_state::<AppState>() {
                    let _ = app.storage.set_session_value(
                        &key,
                        &format!("{},{}", logical.width, logical.height),
                    );
                }
            }
        }
        // Relax the minimum first, then shrink — order matters: a min of 720
        // would clamp the pill set_size and break the capsule.
        let _ = window.set_min_size(Some(tauri::Size::Logical(LogicalSize::new(
            CAPSULE_MIN.0,
            CAPSULE_MIN.1,
        ))));
        window
            .set_size(LogicalSize::new(CAPSULE_SIZE.0, CAPSULE_SIZE.1))
            .map_err(|error| error.to_string())
    } else {
        // Restore the expanded minimum before resizing so the restored size
        // (if smaller than 720, e.g. a stale saved value) is clamped up.
        let _ = window.set_min_size(Some(tauri::Size::Logical(LogicalSize::new(
            EXPANDED_MIN.0,
            EXPANDED_MIN.1,
        ))));
        let (width, height) = window
            .try_state::<AppState>()
            .and_then(|app| {
                let key = format!("{STATE_KEY}:{}", window.label());
                app.storage.get_session_value(&key).ok().flatten()
            })
            .and_then(|raw| {
                let mut parts = raw.splitn(2, ',');
                let w = parts.next()?.parse::<f64>().ok()?;
                let h = parts.next()?.parse::<f64>().ok()?;
                if w >= 200.0 && h >= 200.0 {
                    Some((w, h))
                } else {
                    None
                }
            })
            .unwrap_or(EXPANDED_MIN);
        // Clamp the restored size up to the expanded minimum — never let an
        // old saved value shrink the window below the Control Center default.
        let width = width.max(EXPANDED_MIN.0);
        let height = height.max(EXPANDED_MIN.1);
        window
            .set_size(LogicalSize::new(width, height))
            .map_err(|error| error.to_string())
    }
}

/// Read an arbitrary preference string (e.g. the chosen UI theme). Returns
/// `None` when the key is unset so the frontend can fall back to its default.
#[tauri::command]
fn get_preference(key: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    state
        .storage
        .get_session_value(&key)
        .map_err(public_error)
}

/// Persist an arbitrary preference string (e.g. the chosen UI theme).
#[tauri::command]
fn set_preference(key: String, value: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .storage
        .set_session_value(&key, &value)
        .map_err(public_error)
}

/// Send a desktop notification via the OS notification center. Used by the
/// frontend (browser path falls back to its mock) to ping the user when a chat
/// reply completes or when an auto system-check detects a high-load condition.
/// Failures are non-fatal — we surface them as a string so the caller can log
/// without crashing the chat turn that triggered the notification.
#[tauri::command]
async fn send_notification(
    app: tauri::AppHandle,
    title: String,
    body: String,
) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(&title)
        .body(&body)
        .show()
        .map_err(|e| e.to_string())
}

fn get_llm_settings_inner(state: &State<'_, AppState>) -> AppResult<LlmSettingsStatus> {
    let settings = state.storage.load_llm_settings()?;
    Ok(LlmSettingsStatus::from_settings(
        &settings,
        state.storage.db_path(),
    ))
}

fn save_llm_settings_inner(
    input: LlmSettingsInput,
    state: &State<'_, AppState>,
) -> AppResult<LlmSettingsStatus> {
    let mut settings = state.storage.load_llm_settings()?;
    input.merge_into(&mut settings);
    if settings.system_prompt.trim().is_empty() {
        settings.system_prompt = config::LlmSettings::default().system_prompt;
    }
    state.storage.save_llm_settings(&settings)?;
    Ok(LlmSettingsStatus::from_settings(
        &settings,
        state.storage.db_path(),
    ))
}

async fn send_chat_message_inner(
    window: Window,
    request: ChatRequest,
    state: &State<'_, AppState>,
) -> AppResult<()> {
    emit_chat_event(&window, &request.request_id, "start", "")?;

    let session_id = state.current_session();
    if let Some(last_user) = request.messages.iter().rev().find(|msg| msg.role == "user") {
        state
            .storage
            .save_chat_message(session_id, "user", last_user.content.trim())?;
    }

    let settings = state.storage.load_llm_settings()?;
    // Pull a small, bounded slice of recently-updated memories for stable
    // system-prompt injection (Tier 1 dual-track: this is the always-on half;
    // memory_search is the on-demand half). Failures here are non-fatal — a
    // turn without injected memory still works.
    let memory_slice = state
        .storage
        .recent_memories(8)
        .unwrap_or_else(|err| {
            tracing::warn!(error = %err, "failed to load recent memories for prompt");
            Vec::new()
        })
        .into_iter()
        .map(|m| format!("- [{}] {}", m.key, m.content.replace('\n', " ")))
        .collect::<Vec<_>>();
    let client = LlmClient::new(settings)?.with_recent_memories(memory_slice);
    let dispatcher =
        ToolDispatcher::with_window(state.system.clone(), state.storage.clone(), window.clone());
    let has_tools = !dispatcher.active_definitions()?.is_empty();

    // Two paths:
    // - No tools active → stream tokens straight from the model for snappy UX.
    // - Tools active → run the multi-round tool loop (non-streaming), then
    //   fake-stream the final text so the UI still feels alive. We can't
    //   stream during the loop because tool_calls arrive interleaved with
    //   content chunks and the OpenAI streaming format doesn't let us tell
    //   "this turn wants tools" until the stream finishes.
    let ChatTurnResult {
        text: assistant_text,
        usage: usage_total,
        tool_call_count,
        // Reasoning is emitted live via the stream callback (no-tools path) or
        // as a bulk event above (tools path), so the final aggregated value
        // isn't needed here — bind it away.
        reasoning: _reasoning_text,
    } = if has_tools {
        let result = client
            .complete_with_tool_loop(&request.messages, &dispatcher)
            .await?;
        if result.tool_call_count > 0 {
            emit_chat_event(&window, &request.request_id, "tool", "本地工具已执行")?;
        }
        // The tool-loop path is non-streaming, so the reasoning text arrives
        // in one shot — emit it before the answer chunks.
        if !result.reasoning.is_empty() {
            emit_chat_event(&window, &request.request_id, "reasoning", &result.reasoning)?;
        }
        emit_text_as_chunks(&window, &request.request_id, &result.text).await?;
        result
    } else {
        let window_for_reasoning = window.clone();
        let request_id_for_reasoning = request.request_id.clone();
        client
            .stream_simple(
                &request.messages,
                |delta| {
                    let window = window.clone();
                    let request_id = request.request_id.clone();
                    async move {
                        emit_chat_event(&window, &request_id, "delta", &delta)?;
                        Ok(())
                    }
                },
                |reasoning| {
                    let window = window_for_reasoning.clone();
                    let request_id = request_id_for_reasoning.clone();
                    async move {
                        emit_chat_event(&window, &request_id, "reasoning", &reasoning)?;
                        Ok(())
                    }
                },
            )
            .await?
    };

    if usage_total.total_tokens > 0 {
        state.storage.record_token_usage(
            &request.request_id,
            client.model(),
            usage_total.prompt_tokens,
            usage_total.completion_tokens,
            usage_total.total_tokens,
            tool_call_count as i64,
        )?;
    }

    if !assistant_text.trim().is_empty() {
        state
            .storage
            .save_chat_message(session_id, "assistant", assistant_text.trim())?;
    }
    emit_chat_event(&window, &request.request_id, "done", "")?;
    Ok(())
}

async fn emit_text_as_chunks(window: &Window, request_id: &str, text: &str) -> AppResult<()> {
    let mut chunk = String::new();
    for ch in text.chars() {
        chunk.push(ch);
        if chunk.chars().count() >= 4 || matches!(ch, '。' | '！' | '？' | '\n' | '.' | '!' | '?') {
            emit_chat_event(window, request_id, "delta", &chunk)?;
            chunk.clear();
            tokio::time::sleep(std::time::Duration::from_millis(18)).await;
        }
    }
    if !chunk.is_empty() {
        emit_chat_event(window, request_id, "delta", &chunk)?;
    }
    Ok(())
}

fn emit_chat_event(window: &Window, request_id: &str, kind: &str, content: &str) -> AppResult<()> {
    window
        .emit(
            "chat-stream",
            ChatStreamEvent {
                request_id: request_id.to_string(),
                kind: kind.to_string(),
                content: content.to_string(),
            },
        )
        .map_err(|error| AppError::Model(error.to_string()))
}

/// Very small unique-id helper. We don't pull `uuid` just for this — combining
/// the current nanoseconds with the thread id is more than enough to keep the
/// scan registry's HashMap keys distinct across concurrent invocations.
fn uuid_like() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

/// Initialize the global tracing subscriber once. Respects the `IPET_LOG`
/// env var (e.g. `IPET_LOG=ipet_lib=debug,reqwest=info`); defaults to INFO.
fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_env("IPET_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    // `try_init` is best-effort: if a host (tests, embedding) already set a
    // subscriber we silently keep theirs instead of panicking.
    let _ = fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_thread_ids(false)
        .try_init();
}

pub fn run() {
    init_tracing();
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let secret = match secret::MachineKey::load_or_generate(&data_dir) {
                Ok(k) => Some(k),
                Err(err) => {
                    tracing::warn!(error = %err, "machine key unavailable; api_key will be stored as plaintext");
                    None
                }
            };
            let storage = match Storage::open_with_secret(data_dir.join("ipet.sqlite3"), secret) {
                Ok(s) => Arc::new(s),
                Err(err) => {
                    tracing::error!(error = %err, path = %data_dir.display(), "failed to open storage");
                    return Err(Box::new(err));
                }
            };
            // One-shot retention pass on startup. Failures are non-fatal —
            // the app still works, we just log so persistent failures show up.
            match storage.prune_old() {
                Ok(report) => tracing::info!(
                    chat_removed = report.chat_removed,
                    tokens_removed = report.tokens_removed,
                    samples_removed = report.samples_removed,
                    disk_removed = report.disk_removed,
                    usage_removed = report.usage_removed,
                    pomodoro_removed = report.pomodoro_removed,
                    "data retention sweep complete"
                ),
                Err(err) => tracing::warn!(error = %err, "data retention sweep failed"),
            }
            let system = Arc::new(Mutex::new(SystemMonitor::new()));
            // There must always be a session to write into; create one if the
            // DB is fresh (the v3 migration back-fills one for pre-v3 DBs).
            let current_session_id = match storage.ensure_default_session() {
                Ok(id) => id,
                Err(err) => {
                    tracing::error!(error = %err, "failed to ensure default session");
                    return Err(Box::new(err));
                }
            };
            tracing::info!(data_dir = %data_dir.display(), "iPet backend initialized");
            // Spawn the foreground-usage sampler. It ticks every 15s, reads the
            // `track_app_usage` toggle each tick (so the System view switch
            // takes effect without an event bus), and writes to `app_usage`.
            // Failures inside the loop are non-fatal and logged per-tick.
            let sampler_storage = storage.clone();
            tauri::async_runtime::spawn(usage_tracker::UsageSampler::new(sampler_storage).run());
            app.manage(AppState {
                storage,
                system,
                scans: Arc::new(StdMutex::new(HashMap::new())),
                current_session_id: Arc::new(StdMutex::new(current_session_id)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_llm_settings,
            save_llm_settings,
            get_system_status,
            scan_disk,
            cancel_disk_scan,
            get_recent_messages,
            list_sessions,
            create_session,
            set_current_session,
            get_current_session,
            rename_session,
            delete_session,
            list_memories,
            update_memory,
            delete_memory,
            list_tools,
            save_tool,
            set_tool_enabled,
            delete_tool,
            import_tool_from_path,
            get_token_stats,
            get_app_usage,
            record_pomodoro_session,
            get_pomodoro_stats,
            list_models,
            send_chat_message,
            set_always_on_top,
            set_mouse_passthrough,
            minimize_window,
            close_window,
            start_window_drag,
            set_compact_window,
            get_preference,
            set_preference,
            send_notification
        ])
        .run(tauri::generate_context!())
        .expect("error while running iPet");
}
