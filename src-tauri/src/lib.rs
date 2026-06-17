mod app_error;
mod config;
mod disk_scanner;
mod llm_client;
mod storage;
mod system_monitor;
mod tool_dispatcher;

use app_error::{public_error, AppError, AppResult};
use config::{LlmSettingsInput, LlmSettingsStatus};
use disk_scanner::{DiskScanRequest, DiskScanResult};
use llm_client::{ChatRequest, LlmClient, PreparedTurn, TokenUsage};
use serde::Serialize;
use std::sync::Arc;
use storage::{ChatRecord, Storage, TokenUsageStats, ToolConfig, ToolConfigInput};
use system_monitor::{SystemMonitor, SystemSnapshot};
use tauri::{Emitter, LogicalSize, Manager, State, Window};
use tokio::sync::Mutex;
use tool_dispatcher::ToolDispatcher;

pub struct AppState {
    storage: Arc<Storage>,
    system: Arc<Mutex<SystemMonitor>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatStreamEvent {
    request_id: String,
    kind: String,
    content: String,
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
        let _ = state.storage.cache_system_sample(&json);
    }
    Ok(snapshot)
}

#[tauri::command]
async fn scan_disk(
    request: DiskScanRequest,
    state: State<'_, AppState>,
) -> Result<DiskScanResult, String> {
    let result = tokio::task::spawn_blocking(move || disk_scanner::scan_path(request))
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(public_error))?;

    if let Ok(json) = serde_json::to_string(&result) {
        let _ = state.storage.cache_disk_scan(&result.root.path, &json);
    }
    Ok(result)
}

#[tauri::command]
fn get_recent_messages(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<ChatRecord>, String> {
    state
        .storage
        .recent_messages(limit.unwrap_or(40).clamp(1, 200))
        .map_err(public_error)
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
fn get_token_stats(state: State<'_, AppState>) -> Result<TokenUsageStats, String> {
    state.storage.token_stats().map_err(public_error)
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
    let size = if enabled {
        LogicalSize::new(148.0, 166.0)
    } else {
        LogicalSize::new(440.0, 720.0)
    };
    window.set_size(size).map_err(|error| error.to_string())
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

    if let Some(last_user) = request.messages.iter().rev().find(|msg| msg.role == "user") {
        state
            .storage
            .save_chat_message("user", last_user.content.trim())?;
    }

    let settings = state.storage.load_llm_settings()?;
    let client = LlmClient::new(settings)?;
    let dispatcher = ToolDispatcher::new(state.system.clone(), state.storage.clone());
    let mut usage_total = TokenUsage::default();
    let mut tool_call_count = 0usize;

    let assistant_text = match client
        .prepare_turn_with_tools(&request.messages, &dispatcher)
        .await?
    {
        PreparedTurn::DirectText { text, usage } => {
            usage_total.add(usage.as_ref());
            emit_text_as_chunks(&window, &request.request_id, &text).await?;
            text
        }
        PreparedTurn::ToolAugmented {
            messages,
            usage,
            tool_call_count: calls,
        } => {
            usage_total.add(usage.as_ref());
            tool_call_count = calls;
            emit_chat_event(&window, &request.request_id, "tool", "本地工具已执行")?;
            let stream_result = client
                .stream_final_response(messages, |delta| {
                    let window = window.clone();
                    let request_id = request.request_id.clone();
                    async move {
                        emit_chat_event(&window, &request_id, "delta", &delta)?;
                        Ok(())
                    }
                })
                .await?;
            usage_total.add(stream_result.usage.as_ref());
            stream_result.text
        }
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
            .save_chat_message("assistant", assistant_text.trim())?;
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

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let storage = Arc::new(Storage::open(data_dir.join("ipet.sqlite3"))?);
            let system = Arc::new(Mutex::new(SystemMonitor::new()));
            app.manage(AppState { storage, system });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_llm_settings,
            save_llm_settings,
            get_system_status,
            scan_disk,
            get_recent_messages,
            list_tools,
            save_tool,
            set_tool_enabled,
            delete_tool,
            get_token_stats,
            send_chat_message,
            set_always_on_top,
            set_mouse_passthrough,
            minimize_window,
            close_window,
            start_window_drag,
            set_compact_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running iPet");
}
