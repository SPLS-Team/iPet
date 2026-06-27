use crate::app_error::{AppError, AppResult};
use crate::http_safety::{
    validate_url_runtime, HTTP_MAX_REDIRECTS, HTTP_MAX_RESPONSE_BYTES, HTTP_TIMEOUT_SECS,
};
use crate::storage::{Storage, ToolConfig};
use futures_util::StreamExt;
use ipet_tool_get_system_status::SystemMonitor;
use ipet_tool_scan_disk::{self as disk_scanner, DiskScanRequest};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Window};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

/// Default per-call deadline (seconds) for a `kind: "local"` subprocess tool.
const LOCAL_DEFAULT_TIMEOUT_SECS: u64 = 30;
/// Cap on stdout we'll read from a local tool, so a runaway script can't
/// exhaust memory. Mirrors the HTTP response cap for consistency.
const LOCAL_MAX_OUTPUT_BYTES: usize = HTTP_MAX_RESPONSE_BYTES;

/// Run a synchronous, CPU/blocking builtin tool on Tokio's blocking thread
/// pool and await its result.
///
/// The builtin tool functions (`search_files`, `git_status`, `tesseract`,
/// …) do real blocking work — `fs` reads, `std::process` spawns, `thread::sleep`.
/// Calling them directly inside this `async fn` would pin a Tokio worker
/// thread for the duration of the tool, starving other tasks (IPC, UI events,
/// concurrent tool calls) on the same runtime. `spawn_blocking` moves the
/// closure onto a dedicated blocking-pool thread, so the worker that drove
/// `dispatch_builtin` is freed to service the rest of the runtime while the
/// tool runs. This mirrors the existing `scan_disk` path (lib.rs:125).
///
/// `JoinError` (panic / cancellation) is surfaced as `AppError::Model`; the
/// tool's own `Result<String, String>` is mapped to `InvalidInput` to match
/// the prior in-line behavior.
async fn run_blocking<F>(f: F) -> AppResult<String>
where
    F: FnOnce() -> Result<String, String> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|err| AppError::Model(format!("builtin tool task failed: {err}")))?
        .map_err(AppError::InvalidInput)
}

#[derive(Clone)]
pub struct ToolDispatcher {
    system: Arc<Mutex<SystemMonitor>>,
    storage: Arc<Storage>,
    http: reqwest::Client,
    window: Option<Window>,
}

impl ToolDispatcher {
    pub fn new(system: Arc<Mutex<SystemMonitor>>, storage: Arc<Storage>) -> Self {
        Self::build(system, storage, None)
    }

    pub fn with_window(
        system: Arc<Mutex<SystemMonitor>>,
        storage: Arc<Storage>,
        window: Window,
    ) -> Self {
        Self::build(system, storage, Some(window))
    }

    fn build(
        system: Arc<Mutex<SystemMonitor>>,
        storage: Arc<Storage>,
        window: Option<Window>,
    ) -> Self {
        // A dedicated client for tool HTTP traffic with conservative timeouts
        // and a redirect cap. Falls back to the default client if the
        // builder rejects our config (which it never does for these flags).
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(HTTP_MAX_REDIRECTS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            system,
            storage,
            http,
            window,
        }
    }

    pub fn active_definitions(&self) -> AppResult<Vec<Value>> {
        Ok(self
            .storage
            .active_tools()?
            .into_iter()
            .map(tool_definition)
            .collect())
    }

    pub async fn dispatch(&self, name: &str, arguments: &str) -> AppResult<String> {
        let tool = self
            .storage
            .get_tool(name)?
            .ok_or_else(|| AppError::InvalidInput(format!("工具不存在: {name}")))?;

        if !tool.enabled {
            return Err(AppError::InvalidInput(format!("工具未启用: {name}")));
        }

        tracing::debug!(tool = %name, kind = %tool.kind, "dispatching tool");
        let result = match tool.kind.as_str() {
            "builtin" => self.dispatch_builtin(name, arguments).await,
            "http" => self.dispatch_http(&tool, arguments).await,
            "local" => self.dispatch_local(&tool, arguments).await,
            other => Err(AppError::InvalidInput(format!("不支持的工具类型: {other}"))),
        };
        if let Err(err) = &result {
            tracing::warn!(tool = %name, error = %err, "tool dispatch failed");
        }
        result
    }

    async fn dispatch_builtin(&self, name: &str, arguments: &str) -> AppResult<String> {
        match name {
            "get_system_status" => {
                let args = serde_json::from_str::<SystemStatusArgs>(arguments).unwrap_or_default();
                let mut monitor = self.system.lock().await;
                let snapshot = monitor.snapshot(args.process_limit.unwrap_or(10).clamp(3, 30));
                let json = serde_json::to_string(&snapshot)?;
                self.storage.cache_system_sample(&json)?;
                Ok(json)
            }
            "scan_disk" => {
                let args = serde_json::from_str::<DiskScanArgs>(arguments)
                    .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                let request = DiskScanRequest {
                    path: args.path,
                    max_depth: args.max_depth,
                    max_children: args.max_children,
                    max_duration_secs: None,
                };
                let result = tokio::task::spawn_blocking(move || disk_scanner::scan_path(request))
                    .await
                    .map_err(|error| AppError::Model(error.to_string()))??;
                let json = serde_json::to_string(&result)?;
                self.storage.cache_disk_scan(&result.root.path, &json)?;
                Ok(json)
            }
            "search_files" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::SearchFilesArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::search_files(args)).await
            }
            "read_text_file" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::ReadTextFileArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::read_text_file(args)).await
            }
            "git_status" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::GitStatusArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::git_status(args)).await
            }
            "list_processes" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::ListProcessesArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::list_processes(args)).await
            }
            "network_status" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::NetworkStatusArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::network_status(args)).await
            }
            "clipboard_read" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::ClipboardReadArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::clipboard_read(args)).await
            }
            "open_path" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::OpenPathArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::open_path(args)).await
            }
            "recent_system_errors" => {
                let args = serde_json::from_str::<
                    ipet_tool_desktop_tools::RecentSystemErrorsArgs,
                >(arguments)
                .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::recent_system_errors(args)).await
            }
            "package_scripts" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::PackageScriptsArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::package_scripts(args)).await
            }
            "run_project_check" => {
                let args = serde_json::from_str::<
                    ipet_tool_desktop_tools::RunProjectCheckArgs,
                >(arguments)
                .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::run_project_check(args)).await
            }
            "disk_cleanup_candidates" => {
                let args = serde_json::from_str::<
                    ipet_tool_desktop_tools::DiskCleanupCandidatesArgs,
                >(arguments)
                .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::disk_cleanup_candidates(args)).await
            }
            "create_note" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::CreateNoteArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::create_note(args)).await
            }
            "weather_lookup" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::WeatherLookupArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::weather_lookup(args)).await
            }
            "web_search" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::WebSearchArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::web_search(args)).await
            }
            "screenshot_ocr" => {
                let args =
                    serde_json::from_str::<ipet_tool_desktop_tools::ScreenshotOcrArgs>(arguments)
                        .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                run_blocking(move || ipet_tool_desktop_tools::screenshot_ocr(args)).await
            }
            // Long-term memory tools. Unlike the desktop tools above, these
            // touch `Storage` (SQLite under a std Mutex), so they live in the
            // dispatcher rather than the dependency-light desktop_tools crate —
            // keeps the tool crate free of a storage/tokio dependency and
            // avoids a circular one. Storage I/O is blocking, so run it on the
            // blocking pool just like the other builtins.
            "memory_save" => {
                let args = serde_json::from_str::<MemorySaveArgs>(arguments)
                    .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                let storage = self.storage.clone();
                run_blocking(move || {
                    let category = args.category.as_deref().unwrap_or("general");
                    let memory = storage
                        .save_memory(&args.key, &args.content, category)
                        .map_err(|e| e.to_string())?;
                    serde_json::to_string(&memory).map_err(|err| err.to_string())
                })
                .await
            }
            "memory_search" => {
                let args = serde_json::from_str::<MemorySearchArgs>(arguments)
                    .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                let storage = self.storage.clone();
                run_blocking(move || {
                    let limit = args.limit.unwrap_or(8).clamp(1, 50);
                    let hits = storage
                        .search_memories(&args.query, limit)
                        .map_err(|e| e.to_string())?;
                    serde_json::to_string(&json!({
                        "query": args.query,
                        "resultCount": hits.len(),
                        "memories": hits
                    }))
                    .map_err(|err| err.to_string())
                })
                .await
            }
            // Foreground-app usage stats (the desktop "screen time"). Reads the
            // same `app_usage` rows the background sampler writes; lives in the
            // dispatcher (not desktop_tools) because it needs Storage, like the
            // memory tools above.
            "app_usage_stats" => {
                let args = serde_json::from_str::<AppUsageStatsArgs>(arguments)
                    .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                let storage = self.storage.clone();
                run_blocking(move || {
                    let range = args.range.as_deref().unwrap_or("today");
                    let limit = args.limit.unwrap_or(15).clamp(1, 100);
                    let stats = storage
                        .app_usage_stats(range, limit)
                        .map_err(|e| e.to_string())?;
                    serde_json::to_string(&stats).map_err(|err| err.to_string())
                })
                .await
            }
            "control_pomodoro" => {
                let args = serde_json::from_str::<PomodoroControlArgs>(arguments)
                    .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                validate_pomodoro_args(&args)?;
                let window = self.window.as_ref().ok_or_else(|| {
                    AppError::InvalidInput("番茄钟控制需要可用的应用窗口".to_string())
                })?;
                window
                    .emit("pomodoro-control", &args)
                    .map_err(|error| AppError::Model(error.to_string()))?;
                Ok(json!({
                    "ok": true,
                    "action": args.action,
                    "message": "番茄钟指令已发送到 iPet 界面"
                })
                .to_string())
            }
            other => Err(AppError::InvalidInput(format!("未知内置工具: {other}"))),
        }
    }

    async fn dispatch_http(&self, tool: &ToolConfig, arguments: &str) -> AppResult<String> {
        let http = tool
            .http
            .as_ref()
            .ok_or_else(|| AppError::InvalidInput("HTTP 工具缺少 http 配置".to_string()))?;
        let method = http
            .method
            .to_ascii_uppercase()
            .parse::<Method>()
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;
        // Re-validate at runtime: rejects malformed URLs and any host that
        // resolves to a loopback / private / link-local address. This is
        // belt-and-suspenders with the save-time check, in case the DB was
        // edited externally or DNS shifts after save.
        let url = validate_url_runtime(&http.url).await?;
        let args = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}));

        let mut request = self.http.request(method.clone(), url);
        for header in &http.headers {
            if !header.key.trim().is_empty() {
                request = request.header(header.key.trim(), header.value.trim());
            }
        }

        if method == Method::GET {
            if let Some(map) = args.as_object() {
                let query = map
                    .iter()
                    .map(|(key, value)| (key.as_str(), scalar_to_query(value)))
                    .collect::<Vec<_>>();
                request = request.query(&query);
            }
        } else {
            request = request.json(&args);
        }

        let response = request.send().await?.error_for_status()?;
        let status = response.status().as_u16();
        let body = read_capped_body(response).await?;
        Ok(json!({
            "status": status,
            "body": body
        })
        .to_string())
    }

    /// Spawn a `kind: "local"` tool as a subprocess, ship the model's
    /// arguments as a single JSON line on stdin, and return the child's
    /// stdout. The child owns its own process, so a crash or hang is bounded
    /// by the timeout — it can't take the host down with it.
    async fn dispatch_local(&self, tool: &ToolConfig, arguments: &str) -> AppResult<String> {
        let local = tool
            .local
            .as_ref()
            .ok_or_else(|| AppError::InvalidInput("local 工具缺少 local 配置".to_string()))?;

        // Normalize the incoming arguments to a JSON object; if the model sent
        // malformed JSON, default to {} so the tool still gets a parseable
        // line rather than crashing on stdin.
        let args: Value = serde_json::from_str(arguments).unwrap_or_else(|_| json!({}));

        let timeout = Duration::from_secs(
            local
                .timeout_secs
                .filter(|&s| s > 0)
                .unwrap_or(LOCAL_DEFAULT_TIMEOUT_SECS),
        );

        let mut command = tokio::process::Command::new(&local.command);
        command
            .args(&local.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = local.cwd.as_ref() {
            command.current_dir(cwd);
        }
        // Windows: don't create a visible console window for the child.
        // tokio's `Command` re-exports `creation_flags` from the Windows
        // process extension directly, so no trait import is needed.
        #[cfg(target_os = "windows")]
        {
            // CREATE_NO_WINDOW = 0x08000000
            command.creation_flags(0x0800_0000);
        }

        let mut child = command.spawn().map_err(|err| {
            AppError::InvalidInput(format!("启动本地工具失败 ({}): {err}", local.command))
        })?;

        // Write the arguments JSON line to stdin, then close stdin so the
        // child sees EOF and can finish.
        if let Some(mut stdin) = child.stdin.take() {
            let line = format!("{}\n", args);
            // Best-effort write; a tool that exits without reading stdin is
            // fine — we ignore the broken-pipe error.
            let _ = stdin.write_all(line.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        // Race the child against the timeout. `tokio::select!` cancels the
        // loser; if the timeout wins we kill the child so it can't linger.
        let outcome = tokio::select! {
            biased;
            _ = tokio::time::sleep(timeout) => {
                // Timed out — kill best-effort and report.
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(AppError::InvalidInput(format!(
                    "本地工具超时（{} 秒）", timeout.as_secs()
                )));
            }
            wait_result = child.wait() => {
                let status = wait_result?;
                read_local_output(status, stdout, stderr).await
            }
        }?;
        Ok(outcome)
    }
}

/// Read the (possibly capped) stdout of a finished local tool and turn a
/// non-zero exit into an error carrying stderr for diagnostics.
async fn read_local_output(
    status: std::process::ExitStatus,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
) -> AppResult<String> {
    use tokio::io::AsyncReadExt;

    let stdout_bytes = match stdout {
        Some(mut s) => read_capped_stdout(&mut s).await?,
        None => Vec::new(),
    };
    let stderr_bytes = match stderr {
        Some(mut s) => {
            let mut buf = Vec::new();
            let _ = s.read_to_end(&mut buf).await;
            buf
        }
        None => Vec::new(),
    };

    if !status.success() {
        let stderr_text = String::from_utf8_lossy(&stderr_bytes);
        return Err(AppError::InvalidInput(format!(
            "本地工具退出码非 0（{}）: {}",
            status.code().unwrap_or(-1),
            stderr_text.trim()
        )));
    }
    // We return the raw stdout as-is (it's the tool's contract: stdout is the
    // JSON result). UTF-8 lossy keeps a misbehaving tool from crashing the
    // dispatcher; the model just sees what was printed.
    Ok(String::from_utf8_lossy(&stdout_bytes).into_owned())
}

/// Read up to `LOCAL_MAX_OUTPUT_BYTES` from a child's stdout; abort with an
/// error if the child tries to flood us.
async fn read_capped_stdout<R>(reader: &mut R) -> AppResult<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8 * 1024];
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        if buf.len().saturating_add(n) > LOCAL_MAX_OUTPUT_BYTES {
            return Err(AppError::InvalidInput(format!(
                "本地工具输出超过 {} 字节上限",
                LOCAL_MAX_OUTPUT_BYTES
            )));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}

/// Stream the response body and abort if it exceeds the configured cap so a
/// hostile or buggy upstream cannot exhaust our memory.
async fn read_capped_body(response: reqwest::Response) -> AppResult<String> {
    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if buf.len().saturating_add(chunk.len()) > HTTP_MAX_RESPONSE_BYTES {
            return Err(AppError::InvalidInput(format!(
                "HTTP 响应体超过 {} 字节上限",
                HTTP_MAX_RESPONSE_BYTES
            )));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn tool_definition(tool: ToolConfig) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters
        }
    })
}

fn scalar_to_query(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

#[derive(Debug, Default, Deserialize)]
struct SystemStatusArgs {
    process_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct DiskScanArgs {
    path: String,
    max_depth: Option<usize>,
    max_children: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemorySaveArgs {
    key: String,
    content: String,
    #[serde(default)]
    category: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemorySearchArgs {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppUsageStatsArgs {
    #[serde(default)]
    range: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PomodoroControlArgs {
    action: String,
    #[serde(default)]
    work_minutes: Option<u32>,
    #[serde(default)]
    break_minutes: Option<u32>,
    #[serde(default)]
    long_break_minutes: Option<u32>,
    #[serde(default)]
    long_break_every: Option<u32>,
    #[serde(default)]
    auto_start_break: Option<bool>,
    #[serde(default)]
    auto_start_work: Option<bool>,
}

fn validate_pomodoro_args(args: &PomodoroControlArgs) -> AppResult<()> {
    match args.action.as_str() {
        "start" | "pause" | "resume" | "toggle" | "skip" | "reset" | "configure" => {}
        other => {
            return Err(AppError::InvalidInput(format!(
                "不支持的番茄钟动作: {other}"
            )));
        }
    }

    validate_optional_range("workMinutes", args.work_minutes, 1, 90)?;
    validate_optional_range("breakMinutes", args.break_minutes, 1, 60)?;
    validate_optional_range("longBreakMinutes", args.long_break_minutes, 1, 60)?;
    validate_optional_range("longBreakEvery", args.long_break_every, 2, 12)?;
    Ok(())
}

fn validate_optional_range(name: &str, value: Option<u32>, min: u32, max: u32) -> AppResult<()> {
    if let Some(value) = value {
        if !(min..=max).contains(&value) {
            return Err(AppError::InvalidInput(format!(
                "{name} 必须在 {min} 到 {max} 之间"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{LocalToolConfig, ToolConfigInput};
    use crate::testutil::TempDir;

    /// Build a dispatcher backed by a fresh on-disk Storage (no system monitor
    /// needed — these tests only exercise the local/http paths).
    fn fresh_dispatcher() -> (TempDir, ToolDispatcher) {
        let dir = TempDir::new("disp");
        let storage = Arc::new(
            crate::storage::Storage::open(dir.path().join("ipet-disp.sqlite3"))
                .expect("storage opens"),
        );
        let system = Arc::new(Mutex::new(SystemMonitor::new()));
        (dir, ToolDispatcher::new(system, storage))
    }

    /// A platform-specific no-op-ish command that exits 0 and prints a fixed
    /// token, so we can exercise the spawn→stdout→exit-0 path without a real
    /// tool script.
    fn echo_command() -> (String, Vec<String>) {
        if cfg!(target_os = "windows") {
            (
                "cmd".to_string(),
                vec!["/C".to_string(), "echo ok".to_string()],
            )
        } else {
            ("printf".to_string(), vec!["ok".to_string()])
        }
    }

    #[test]
    fn pomodoro_args_validate_action_and_ranges() {
        let valid = PomodoroControlArgs {
            action: "configure".to_string(),
            work_minutes: Some(25),
            break_minutes: Some(5),
            long_break_minutes: Some(15),
            long_break_every: Some(4),
            auto_start_break: Some(true),
            auto_start_work: Some(false),
        };
        assert!(validate_pomodoro_args(&valid).is_ok());

        let invalid_action = PomodoroControlArgs {
            action: "launch".to_string(),
            work_minutes: None,
            break_minutes: None,
            long_break_minutes: None,
            long_break_every: None,
            auto_start_break: None,
            auto_start_work: None,
        };
        assert!(validate_pomodoro_args(&invalid_action).is_err());

        let invalid_range = PomodoroControlArgs {
            action: "configure".to_string(),
            work_minutes: Some(120),
            break_minutes: None,
            long_break_minutes: None,
            long_break_every: None,
            auto_start_break: None,
            auto_start_work: None,
        };
        assert!(validate_pomodoro_args(&invalid_range).is_err());
    }

    #[tokio::test]
    async fn dispatch_local_runs_subprocess_and_returns_stdout() {
        let (_dir, dispatcher) = fresh_dispatcher();
        let (command, args) = echo_command();
        let input = ToolConfigInput {
            name: "echo_test".to_string(),
            display_name: "Echo".to_string(),
            description: "test local tool".to_string(),
            kind: "local".to_string(),
            enabled: true,
            parameters: json!({"type": "object", "properties": {}}),
            http: None,
            local: Some(LocalToolConfig {
                command,
                args,
                cwd: None,
                timeout_secs: Some(10),
            }),
        };
        dispatcher.storage.save_custom_tool(input).unwrap();

        let out = dispatcher
            .dispatch("echo_test", r#"{"text":"hi"}"#)
            .await
            .expect("local dispatch succeeds");
        // The tool prints "ok" (plus a trailing newline on Windows). We only
        // care that stdout made it back.
        assert!(
            out.contains("ok"),
            "expected stdout containing 'ok', got: {out}"
        );
    }

    #[tokio::test]
    async fn imported_local_package_runs_script_arg_from_package_cwd() {
        let (dir, dispatcher) = fresh_dispatcher();
        let pkg_dir = dir.path().join("pkg-script-cwd");
        std::fs::create_dir_all(&pkg_dir).unwrap();

        let (script_name, command, args) = if cfg!(target_os = "windows") {
            let script_name = "emit-ok.bat";
            std::fs::write(pkg_dir.join(script_name), "@echo off\necho script-ok\n").unwrap();
            (
                script_name,
                "cmd".to_string(),
                vec!["/C".to_string(), script_name.to_string()],
            )
        } else {
            let script_name = "emit-ok.sh";
            std::fs::write(pkg_dir.join(script_name), "printf script-ok\n").unwrap();
            (script_name, "sh".to_string(), vec![script_name.to_string()])
        };

        let manifest = json!({
            "schemaVersion": 2,
            "name": "script_cwd_test",
            "displayName": "Script CWD",
            "description": "runs a package-local script passed as an arg",
            "kind": "local",
            "parameters": {"type": "object", "properties": {}},
            "local": {
                "command": command,
                "args": args,
                "timeoutSecs": 10
            }
        });
        std::fs::write(
            pkg_dir.join("tool.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let pkg = crate::tool_package::load_package(&pkg_dir).unwrap();
        let local = pkg.input.local.as_ref().unwrap();
        assert_eq!(local.args.last().map(String::as_str), Some(script_name));
        assert!(
            local.cwd.is_some(),
            "imported local packages must default cwd to the package dir"
        );
        dispatcher.storage.save_custom_tool(pkg.input).unwrap();

        let out = dispatcher
            .dispatch("script_cwd_test", "{}")
            .await
            .expect("imported package script should run from package cwd");
        assert!(
            out.contains("script-ok"),
            "expected package script stdout, got: {out}"
        );
    }

    #[tokio::test]
    async fn dispatch_local_reports_nonzero_exit() {
        let (_dir, dispatcher) = fresh_dispatcher();
        let command = if cfg!(target_os = "windows") {
            "cmd".to_string()
        } else {
            "false".to_string()
        };
        let args = if cfg!(target_os = "windows") {
            vec!["/C".to_string(), "exit 3".to_string()]
        } else {
            vec![]
        };
        let input = ToolConfigInput {
            name: "failing_test".to_string(),
            display_name: "Failing".to_string(),
            description: "exits nonzero".to_string(),
            kind: "local".to_string(),
            enabled: true,
            parameters: json!({"type": "object"}),
            http: None,
            local: Some(LocalToolConfig {
                command,
                args,
                cwd: None,
                timeout_secs: Some(10),
            }),
        };
        dispatcher.storage.save_custom_tool(input).unwrap();

        let err = dispatcher
            .dispatch("failing_test", "{}")
            .await
            .expect_err("nonzero exit must error");
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn dispatch_local_rejects_disabled_tool() {
        let (_dir, dispatcher) = fresh_dispatcher();
        let (command, args) = echo_command();
        let input = ToolConfigInput {
            name: "disabled_test".to_string(),
            display_name: "Disabled".to_string(),
            description: "d".to_string(),
            kind: "local".to_string(),
            enabled: false,
            parameters: json!({"type": "object"}),
            http: None,
            local: Some(LocalToolConfig {
                command,
                args,
                cwd: None,
                timeout_secs: Some(10),
            }),
        };
        dispatcher.storage.save_custom_tool(input).unwrap();

        let err = dispatcher
            .dispatch("disabled_test", "{}")
            .await
            .expect_err("disabled tool must not dispatch");
        assert!(matches!(err, AppError::InvalidInput(_)));
    }
}
