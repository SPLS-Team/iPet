use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::Ordering;
use std::fs;
use std::io::Read;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};
use sysinfo::{ProcessesToUpdate, System};
use walkdir::{DirEntry, WalkDir};

const MAX_SEARCH_RESULTS: usize = 100;
const MAX_SEARCH_VISITED: usize = 20_000;
const DEFAULT_SEARCH_FILE_BYTES: u64 = 256 * 1024;
const MAX_TEXT_BYTES: u64 = 1024 * 1024;
const DEFAULT_TEXT_BYTES: u64 = 64 * 1024;
const DEFAULT_TEXT_LINES: usize = 300;
const MAX_TEXT_LINES: usize = 2_000;
const MAX_COMMAND_OUTPUT_CHARS: usize = 20_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilesArgs {
    pub path: String,
    pub query: String,
    #[serde(default)]
    pub search_content: Option<bool>,
    #[serde(default)]
    pub include_hidden: Option<bool>,
    #[serde(default)]
    pub max_results: Option<usize>,
    #[serde(default)]
    pub max_file_bytes: Option<u64>,
    #[serde(default)]
    pub file_extensions: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchMatch {
    path: String,
    kind: String,
    match_type: String,
    size_bytes: Option<u64>,
    modified_at: Option<String>,
    line: Option<usize>,
    snippet: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchFilesResult {
    root: String,
    query: String,
    scanned_entries: usize,
    truncated: bool,
    matches: Vec<SearchMatch>,
}

pub fn search_files(args: SearchFilesArgs) -> Result<String, String> {
    let root = normalize_existing_path(&args.path)?;
    if !root.is_dir() {
        return Err(format!("path is not a directory: {}", root.display()));
    }
    let query = args.query.trim();
    if query.is_empty() {
        return Err("query must not be empty".to_string());
    }

    let query_lc = query.to_lowercase();
    let include_hidden = args.include_hidden.unwrap_or(false);
    let search_content = args.search_content.unwrap_or(false);
    let max_results = args.max_results.unwrap_or(30).clamp(1, MAX_SEARCH_RESULTS);
    let max_file_bytes = args
        .max_file_bytes
        .unwrap_or(DEFAULT_SEARCH_FILE_BYTES)
        .clamp(1024, MAX_TEXT_BYTES);
    let extensions = args.file_extensions.map(normalize_extensions);
    let mut scanned_entries = 0usize;
    let mut matches = Vec::new();
    let mut truncated = false;

    let walker = WalkDir::new(&root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| include_hidden || !is_hidden(entry));

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        scanned_entries += 1;
        if scanned_entries >= MAX_SEARCH_VISITED {
            truncated = true;
            break;
        }
        if matches.len() >= max_results {
            truncated = true;
            break;
        }

        let path = entry.path();
        let metadata = entry.metadata().ok();
        let is_file = metadata.as_ref().map(|m| m.is_file()).unwrap_or(false);
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let kind = if is_dir { "directory" } else { "file" };

        if let Some(exts) = extensions.as_ref() {
            if is_file && !extension_matches(path, exts) {
                continue;
            }
        }

        let file_name = entry.file_name().to_string_lossy().to_lowercase();
        let path_text = path.display().to_string().to_lowercase();
        if file_name.contains(&query_lc) || path_text.contains(&query_lc) {
            matches.push(SearchMatch {
                path: path.display().to_string(),
                kind: kind.to_string(),
                match_type: "name".to_string(),
                size_bytes: metadata.as_ref().map(|m| m.len()),
                modified_at: metadata.as_ref().and_then(modified_at),
                line: None,
                snippet: None,
            });
            continue;
        }

        if search_content && is_file {
            if let Some((line, snippet)) = search_file_content(path, &query_lc, max_file_bytes) {
                matches.push(SearchMatch {
                    path: path.display().to_string(),
                    kind: "file".to_string(),
                    match_type: "content".to_string(),
                    size_bytes: metadata.as_ref().map(|m| m.len()),
                    modified_at: metadata.as_ref().and_then(modified_at),
                    line: Some(line),
                    snippet: Some(snippet),
                });
            }
        }
    }

    serde_json::to_string(&SearchFilesResult {
        root: root.display().to_string(),
        query: query.to_string(),
        scanned_entries,
        truncated,
        matches,
    })
    .map_err(|err| err.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadTextFileArgs {
    pub path: String,
    #[serde(default)]
    pub max_bytes: Option<u64>,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub max_lines: Option<usize>,
}

pub fn read_text_file(args: ReadTextFileArgs) -> Result<String, String> {
    let path = normalize_existing_path(&args.path)?;
    if !path.is_file() {
        return Err(format!("path is not a file: {}", path.display()));
    }
    let metadata = fs::metadata(&path).map_err(|err| err.to_string())?;
    let max_bytes = args
        .max_bytes
        .unwrap_or(DEFAULT_TEXT_BYTES)
        .clamp(1, MAX_TEXT_BYTES);
    let start_line = args.start_line.unwrap_or(1).max(1);
    let max_lines = args
        .max_lines
        .unwrap_or(DEFAULT_TEXT_LINES)
        .clamp(1, MAX_TEXT_LINES);

    let mut file = fs::File::open(&path).map_err(|err| err.to_string())?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    let truncated = bytes.len() as u64 > max_bytes || metadata.len() > max_bytes;
    if bytes.len() as u64 > max_bytes {
        bytes.truncate(max_bytes as usize);
    }

    let text = String::from_utf8_lossy(&bytes);
    let selected = text
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let line_no = idx + 1;
            (line_no >= start_line && line_no < start_line + max_lines)
                .then(|| format!("{line_no}: {line}"))
        })
        .collect::<Vec<_>>();

    serde_json::to_string(&json!({
        "path": path.display().to_string(),
        "sizeBytes": metadata.len(),
        "modifiedAt": modified_at(&metadata),
        "truncated": truncated,
        "startLine": start_line,
        "maxLines": max_lines,
        "content": selected.join("\n")
    }))
    .map_err(|err| err.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusArgs {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub include_diff: Option<bool>,
    #[serde(default)]
    pub max_diff_chars: Option<usize>,
}

pub fn git_status(args: GitStatusArgs) -> Result<String, String> {
    let path = args.path.unwrap_or_else(|| ".".to_string());
    let cwd = normalize_existing_path(&path)?;
    let root = run_command(&cwd, "git", &["rev-parse", "--show-toplevel"])?;
    if !root.status_success {
        return Err(format!("not a git repository: {}", cwd.display()));
    }

    let branch = run_command(&cwd, "git", &["branch", "--show-current"])?;
    let head = run_command(&cwd, "git", &["rev-parse", "--short", "HEAD"])?;
    let status = run_command(&cwd, "git", &["status", "--short", "--branch"])?;
    let diff_stat = run_command(&cwd, "git", &["diff", "--stat"])?;
    let include_diff = args.include_diff.unwrap_or(false);
    let max_diff_chars = args.max_diff_chars.unwrap_or(12_000).clamp(0, 50_000);
    let diff = if include_diff && max_diff_chars > 0 {
        Some(truncate_chars(
            run_command(&cwd, "git", &["diff"])?.stdout.trim(),
            max_diff_chars,
        ))
    } else {
        None
    };

    serde_json::to_string(&json!({
        "root": root.stdout.trim(),
        "branch": branch.stdout.trim(),
        "head": head.stdout.trim(),
        "status": status.stdout.trim(),
        "diffStat": diff_stat.stdout.trim(),
        "diff": diff
    }))
    .map_err(|err| err.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListProcessesArgs {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub sort_by: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessInfo {
    pid: String,
    name: String,
    cpu_usage: f32,
    memory_bytes: u64,
    virtual_memory_bytes: u64,
    executable: Option<String>,
    command: String,
}

pub fn list_processes(args: ListProcessesArgs) -> Result<String, String> {
    let mut system = System::new_all();
    system.refresh_processes(ProcessesToUpdate::All, true);
    std::thread::sleep(Duration::from_millis(180));
    system.refresh_processes(ProcessesToUpdate::All, true);

    let limit = args.limit.unwrap_or(30).clamp(1, 100);
    let filter = args.filter.unwrap_or_default().to_lowercase();
    let mut processes = system
        .processes()
        .values()
        .map(|process| {
            let name = process.name().to_string_lossy().to_string();
            let command = process
                .cmd()
                .iter()
                .map(|part| part.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            ProcessInfo {
                pid: process.pid().to_string(),
                name,
                cpu_usage: process.cpu_usage(),
                memory_bytes: process.memory(),
                virtual_memory_bytes: process.virtual_memory(),
                executable: process.exe().map(|path| path.display().to_string()),
                command,
            }
        })
        .filter(|process| {
            filter.is_empty()
                || process.name.to_lowercase().contains(&filter)
                || process.command.to_lowercase().contains(&filter)
        })
        .collect::<Vec<_>>();

    match args.sort_by.as_deref().unwrap_or("cpu") {
        "memory" => processes.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes)),
        "name" => processes.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
        _ => processes.sort_by(|a, b| {
            b.cpu_usage
                .partial_cmp(&a.cpu_usage)
                .unwrap_or(Ordering::Equal)
                .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
        }),
    }
    let total = processes.len();
    processes.truncate(limit);

    serde_json::to_string(&json!({
        "totalMatched": total,
        "limit": limit,
        "processes": processes
    }))
    .map_err(|err| err.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatusArgs {
    #[serde(default)]
    pub dns_host: Option<String>,
    #[serde(default)]
    pub tcp_target: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub include_interfaces: Option<bool>,
}

pub fn network_status(args: NetworkStatusArgs) -> Result<String, String> {
    let dns_host = args.dns_host.unwrap_or_else(|| "example.com".to_string());
    let tcp_target = args.tcp_target.unwrap_or_else(|| "1.1.1.1:53".to_string());
    let timeout = Duration::from_millis(args.timeout_ms.unwrap_or(1200).clamp(100, 5000));

    let dns_started = Instant::now();
    let dns_result = (dns_host.as_str(), 80).to_socket_addrs();
    let dns_latency_ms = dns_started.elapsed().as_millis() as u64;
    let dns = match dns_result {
        Ok(addrs) => {
            let addresses = addrs.map(|addr| addr.ip().to_string()).collect::<Vec<_>>();
            json!({ "host": dns_host, "ok": true, "latencyMs": dns_latency_ms, "addresses": addresses })
        }
        Err(err) => {
            json!({ "host": dns_host, "ok": false, "latencyMs": dns_latency_ms, "error": err.to_string() })
        }
    };

    let tcp_started = Instant::now();
    let tcp = match tcp_target.to_socket_addrs() {
        Ok(mut addrs) => {
            if let Some(addr) = addrs.next() {
                match TcpStream::connect_timeout(&addr, timeout) {
                    Ok(_) => {
                        json!({ "target": tcp_target, "address": addr.to_string(), "ok": true, "latencyMs": tcp_started.elapsed().as_millis() as u64 })
                    }
                    Err(err) => {
                        json!({ "target": tcp_target, "address": addr.to_string(), "ok": false, "latencyMs": tcp_started.elapsed().as_millis() as u64, "error": err.to_string() })
                    }
                }
            } else {
                json!({ "target": tcp_target, "ok": false, "error": "no resolved address" })
            }
        }
        Err(err) => json!({ "target": tcp_target, "ok": false, "error": err.to_string() }),
    };

    let interfaces = if args.include_interfaces.unwrap_or(true) {
        Some(interface_summary())
    } else {
        None
    };

    serde_json::to_string(&json!({
        "platform": std::env::consts::OS,
        "hostName": host_name(),
        "dns": dns,
        "tcp": tcp,
        "interfaces": interfaces
    }))
    .map_err(|err| err.to_string())
}

fn normalize_existing_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return Err("path must not be empty".to_string());
    }
    fs::canonicalize(path).map_err(|err| format!("{}: {err}", path.display()))
}

fn is_hidden(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

fn normalize_extensions(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().trim_start_matches('.').to_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn extension_matches(path: &Path, exts: &[String]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| exts.iter().any(|item| item == &ext.to_lowercase()))
        .unwrap_or(false)
}

fn search_file_content(
    path: &Path,
    query_lc: &str,
    max_file_bytes: u64,
) -> Option<(usize, String)> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.len() > max_file_bytes {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    for (idx, line) in text.lines().enumerate() {
        if line.to_lowercase().contains(query_lc) {
            return Some((idx + 1, truncate_chars(line.trim(), 220)));
        }
    }
    None
}

fn modified_at(metadata: &fs::Metadata) -> Option<String> {
    metadata.modified().ok().map(system_time_to_rfc3339)
}

fn system_time_to_rfc3339(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339()
}

#[derive(Debug)]
struct CommandResult {
    status_success: bool,
    stdout: String,
    stderr: String,
}

fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<CommandResult, String> {
    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    hide_window(&mut command);
    let output = command
        .output()
        .map_err(|err| format!("failed to run {} {}: {err}", program, args.join(" ")))?;
    Ok(CommandResult {
        status_success: output.status.success(),
        stdout: truncate_chars(
            &String::from_utf8_lossy(&output.stdout),
            MAX_COMMAND_OUTPUT_CHARS,
        ),
        stderr: truncate_chars(
            &String::from_utf8_lossy(&output.stderr),
            MAX_COMMAND_OUTPUT_CHARS,
        ),
    })
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let mut out = value.chars().take(max).collect::<String>();
    out.push_str("\n...[truncated]");
    out
}

fn interface_summary() -> String {
    #[cfg(target_os = "windows")]
    let result = run_command(Path::new("."), "ipconfig", &["/all"]);
    #[cfg(target_os = "macos")]
    let result = run_command(Path::new("."), "ifconfig", &[]);
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = run_command(Path::new("."), "ip", &["addr"]);
    #[cfg(not(any(target_os = "windows", unix)))]
    let result: Result<CommandResult, String> =
        Err("interface summary unsupported on this OS".to_string());

    match result {
        Ok(out) if out.status_success => out.stdout,
        Ok(out) => format!("command failed: {}", out.stderr),
        Err(err) => err,
    }
}

fn host_name() -> Option<String> {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
}

#[cfg(target_os = "windows")]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

#[cfg(not(target_os = "windows"))]
fn hide_window(_command: &mut Command) {}

// =========================================================================
// Extended desktop tools (medium/low priority): clipboard, open, system-log,
// project introspection, notes, cleanup candidates, keyless web/weather, and
// screenshot OCR. Read-only unless noted; every network/subprocess call caps
// its output and carries a timeout (ref-plan §6.4 / CLAUDE.md performance gates).
// =========================================================================

const MAX_LOG_CHARS: usize = 20_000;
const MAX_CLIPBOARD_CHARS: usize = 20_000;
const MAX_NOTE_BYTES: u64 = 256 * 1024;
const LOCAL_HTTP_TIMEOUT_SECS: u64 = 8;
const LOCAL_HTTP_MAX_BODY_CHARS: usize = 200_000;

// --- clipboard_read ------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardReadArgs {}

pub fn clipboard_read(_args: ClipboardReadArgs) -> Result<String, String> {
    let raw = read_clipboard()?;
    let total = raw.chars().count();
    let text = truncate_chars(&raw, MAX_CLIPBOARD_CHARS);
    serde_json::to_string(&json!({
        "platform": std::env::consts::OS,
        "length": total,
        "truncated": total > MAX_CLIPBOARD_CHARS,
        "text": text
    }))
    .map_err(|err| err.to_string())
}

fn read_clipboard() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let out = run_command(
            Path::new("."),
            "powershell",
            &["-NoProfile", "-Command", "Get-Clipboard -Raw"],
        )?;
        if !out.status_success {
            return Err(format!("Get-Clipboard failed: {}", out.stderr));
        }
        Ok(out.stdout)
    }
    #[cfg(target_os = "macos")]
    {
        let out = run_command(Path::new("."), "pbpaste", &[])?;
        if !out.status_success {
            return Err(format!("pbpaste failed: {}", out.stderr));
        }
        Ok(out.stdout)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(o) = run_command(Path::new("."), "xclip", &["-selection", "clipboard", "-o"]) {
            if o.status_success {
                return Ok(o.stdout);
            }
        }
        match run_command(Path::new("."), "xsel", &["--clipboard", "--output"]) {
            Ok(o) => Ok(o.stdout),
            Err(err) => Err(format!("xclip/xsel not available: {err}")),
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        Err("clipboard unsupported on this OS".to_string())
    }
}

// --- open_path -----------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPathArgs {
    pub path: String,
}

pub fn open_path(args: OpenPathArgs) -> Result<String, String> {
    let target = args.path.trim();
    if target.is_empty() {
        return Err("path must not be empty".to_string());
    }
    let is_url = target.starts_with("http://") || target.starts_with("https://");
    let (kind, opened) = if is_url {
        open_with_system(target)?;
        ("url".to_string(), target.to_string())
    } else {
        let resolved = normalize_existing_path(target)?;
        let k = if resolved.is_dir() {
            "directory"
        } else {
            "file"
        };
        let display = resolved.display().to_string();
        open_with_system(&display)?;
        (k.to_string(), display)
    };
    serde_json::to_string(&json!({ "opened": opened, "kind": kind })).map_err(|err| err.to_string())
}

fn open_with_system(target: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        run_command(Path::new("."), "cmd", &["/C", "start", "", target])?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        run_command(Path::new("."), "open", &[target])?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run_command(Path::new("."), "xdg-open", &[target])?;
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        Err("open unsupported on this OS".to_string())
    }
}

// --- recent_system_errors ------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentSystemErrorsArgs {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub since_hours: Option<u64>,
}

pub fn recent_system_errors(args: RecentSystemErrorsArgs) -> Result<String, String> {
    let limit = args.limit.unwrap_or(20).clamp(1, 100);
    let hours = args.since_hours.unwrap_or(24).clamp(1, 168);
    let raw = fetch_system_errors(hours, limit)?;
    let total = raw.chars().count();
    let text = truncate_chars(&raw, MAX_LOG_CHARS);
    serde_json::to_string(&json!({
        "platform": std::env::consts::OS,
        "sinceHours": hours,
        "limit": limit,
        "truncated": total > MAX_LOG_CHARS,
        "entries": text
    }))
    .map_err(|err| err.to_string())
}

fn fetch_system_errors(hours: u64, limit: usize) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        // wevtutil: Level 1=Critical, 2=Error; /rd:true newest-first.
        // timediff is milliseconds since @SystemTime, so the time bound honors
        // sinceHours. &lt;= is the XML-escaped form of <= inside the query.
        let ms = hours * 3_600_000;
        let query = format!("/q:*[System[(Level=1 or Level=2) and TimeCreated[timediff(@SystemTime) &lt;= {ms}]]]");
        let count = format!("/c:{limit}");
        let args: Vec<String> = ["qe", "System"]
            .iter()
            .map(|s| s.to_string())
            .chain([query, count, "/rd:true".to_string(), "/f:text".to_string()])
            .collect();
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let out = run_command(Path::new("."), "wevtutil", &refs)?;
        Ok(format!("{}\n{}", out.stdout, out.stderr))
    }
    #[cfg(target_os = "macos")]
    {
        let last = format!("{hours}h");
        let args: Vec<&str> = vec!["show", "--last", &last, "--predicate", "messageType == error", "--style", "syslog"];
        let out = run_command(Path::new("."), "log", &args)?;
        Ok(out.stdout)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let n = (limit * 5).to_string();
        let since = format!("-{hours}h");
        let args: Vec<&str> = vec!["-p", "err", "--since", &since, "-n", &n, "--no-pager"];
        let out = run_command(Path::new("."), "journalctl", &args)?;
        Ok(out.stdout)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        Err("system error log unsupported on this OS".to_string())
    }
}

// --- package_scripts -----------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageScriptsArgs {
    #[serde(default)]
    pub path: Option<String>,
}

pub fn package_scripts(args: PackageScriptsArgs) -> Result<String, String> {
    let dir = normalize_existing_path(&args.path.unwrap_or_else(|| ".".to_string()))?;
    let pkg = dir.join("package.json");
    if !pkg.is_file() {
        return Err(format!("no package.json in {}", dir.display()));
    }
    let raw = fs::read_to_string(&pkg).map_err(|err| format!("read {}: {err}", pkg.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|err| format!("invalid package.json: {err}"))?;
    let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let version = value.get("version").and_then(|v| v.as_str()).unwrap_or("");
    let scripts = value.get("scripts").cloned().unwrap_or_else(|| json!({}));
    serde_json::to_string(&json!({
        "path": dir.display().to_string(),
        "name": name,
        "version": version,
        "scripts": scripts
    }))
    .map_err(|err| err.to_string())
}

// --- run_project_check ---------------------------------------------------

/// Commands the LLM may invoke through this tool. Anything else is rejected so
/// a model can't run arbitrary scripts. Extending this list is a deliberate,
/// reviewed change.
const ALLOWED_CHECK_COMMANDS: &[&str] = &[
    "npm test",
    "npm run test",
    "npm run build",
    "npm run lint",
    "npm run check",
    "npm run typecheck",
    "cargo test",
    "cargo check",
    "cargo build",
    "cargo clippy",
    "cargo fmt --check",
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProjectCheckArgs {
    #[serde(default)]
    pub path: Option<String>,
    pub command: String,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

pub fn run_project_check(args: RunProjectCheckArgs) -> Result<String, String> {
    let command = args.command.trim();
    if command.is_empty() {
        return Err("command must not be empty".to_string());
    }
    let normalized = command.split_whitespace().collect::<Vec<_>>().join(" ");
    if !ALLOWED_CHECK_COMMANDS.iter().any(|allowed| *allowed == normalized) {
        return Err(format!(
            "command not in allow-list: {normalized}（允许：{}）",
            ALLOWED_CHECK_COMMANDS.join(", ")
        ));
    }
    let cwd = normalize_existing_path(&args.path.unwrap_or_else(|| ".".to_string()))?;
    let parts: Vec<String> = command.split_whitespace().map(|s| s.to_string()).collect();
    let program = parts[0].clone();
    let rest: Vec<String> = parts[1..].to_vec();
    let timeout = args.timeout_secs.unwrap_or(60).clamp(5, 600);
    let result = run_timed_command(&cwd, &program, &rest, timeout)?;
    let timed_out = !result.status_success && result.stderr.starts_with("timed out");
    serde_json::to_string(&json!({
        "command": normalized,
        "cwd": cwd.display().to_string(),
        "exitOk": result.status_success,
        "timedOut": timed_out,
        "stdout": result.stdout,
        "stderr": result.stderr
    }))
    .map_err(|err| err.to_string())
}

/// Run a subprocess with a hard timeout, draining stdout/stderr on separate
/// threads so a large build log can't fill the OS pipe buffer and deadlock the
/// child before we ever read it.
fn run_timed_command(
    cwd: &Path,
    program: &str,
    args: &[String],
    timeout_secs: u64,
) -> Result<CommandResult, String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    hide_window(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|err| format!("failed to spawn {program}: {err}"))?;
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();
    let out_thread = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut h) = stdout_handle {
            let _ = std::io::Read::read_to_string(&mut h, &mut s);
        }
        s
    });
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(mut h) = stderr_handle {
            let _ = std::io::Read::read_to_string(&mut h, &mut s);
        }
        s
    });
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut exit_ok = false;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_ok = status.success();
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(err) => return Err(format!("wait failed: {err}")),
        }
    }
    let out_s = out_thread.join().unwrap_or_default();
    let err_s = err_thread.join().unwrap_or_default();
    let stderr = if timed_out {
        format!("timed out after {timeout_secs}s\n{err_s}")
    } else {
        err_s
    };
    Ok(CommandResult {
        status_success: exit_ok,
        stdout: truncate_chars(&out_s, MAX_COMMAND_OUTPUT_CHARS),
        stderr: truncate_chars(&stderr, MAX_COMMAND_OUTPUT_CHARS),
    })
}

// --- disk_cleanup_candidates --------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskCleanupCandidatesArgs {
    #[serde(default)]
    pub max_results: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupCandidate {
    path: String,
    size_bytes: u64,
    modified_at: Option<String>,
}

pub fn disk_cleanup_candidates(args: DiskCleanupCandidatesArgs) -> Result<String, String> {
    let max_results = args.max_results.unwrap_or(40).clamp(1, 200);
    let dirs = cleanup_candidate_dirs();
    let mut candidates: Vec<CleanupCandidate> = Vec::new();
    let mut visited = 0usize;
    const MAX_VISIT: usize = 50_000;
    for dir in &dirs {
        if !dir.is_dir() {
            continue;
        }
        for entry in WalkDir::new(dir)
            .follow_links(false)
            .max_depth(3)
            .into_iter()
            .flatten()
        {
            visited += 1;
            if visited > MAX_VISIT || candidates.len() >= max_results * 4 {
                break;
            }
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };
            if !meta.is_file() || meta.len() < 64 * 1024 {
                continue;
            }
            candidates.push(CleanupCandidate {
                path: entry.path().display().to_string(),
                size_bytes: meta.len(),
                modified_at: modified_at(&meta),
            });
        }
    }
    candidates.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    candidates.truncate(max_results);
    let total: u64 = candidates.iter().map(|c| c.size_bytes).sum();
    serde_json::to_string(&json!({
        "platform": std::env::consts::OS,
        "scannedDirs": dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
        "candidates": candidates,
        "totalBytes": total,
        "note": "只读列举，不删除任何文件。"
    }))
    .map_err(|err| err.to_string())
}

fn cleanup_candidate_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Ok(t) = std::env::var("TEMP") {
            dirs.push(PathBuf::from(t));
        }
        if let Ok(la) = std::env::var("LOCALAPPDATA") {
            dirs.push(PathBuf::from(&la).join("Temp"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(t) = std::env::var("TMPDIR") {
            dirs.push(PathBuf::from(t));
        }
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(&home).join("Library/Caches"));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        dirs.push(PathBuf::from("/tmp"));
        dirs.push(PathBuf::from("/var/tmp"));
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(&home).join(".cache"));
        }
    }
    dirs
}

// --- create_note ---------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateNoteArgs {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub append: Option<bool>,
}

pub fn create_note(args: CreateNoteArgs) -> Result<String, String> {
    let title = args.title.trim();
    if title.is_empty() {
        return Err("title must not be empty".to_string());
    }
    let content_bytes = args.content.as_bytes();
    if content_bytes.len() as u64 > MAX_NOTE_BYTES {
        return Err(format!("note too large (max {MAX_NOTE_BYTES} bytes)"));
    }
    let notes_dir = notes_directory()?;
    fs::create_dir_all(&notes_dir).map_err(|err| format!("create notes dir: {err}"))?;
    let slug = slugify(title);
    let file_path = notes_dir.join(format!("{slug}.md"));
    let append = args.append.unwrap_or(false);
    let body = if append {
        let existing = fs::read_to_string(&file_path).unwrap_or_default();
        if existing.is_empty() {
            format!("# {title}\n\n{}", args.content)
        } else {
            format!("{existing}\n\n## {title}\n\n{}", args.content)
        }
    } else {
        format!("# {title}\n\n{}", args.content)
    };
    fs::write(&file_path, &body).map_err(|err| format!("write note: {err}"))?;
    serde_json::to_string(&json!({
        "path": file_path.display().to_string(),
        "title": title,
        "appended": append,
        "bytes": body.len()
    }))
    .map_err(|err| err.to_string())
}

/// Notes live under `~/.ipet/notes/`. An `IPET_NOTES_DIR` override exists for
/// tests and users who want a custom location.
fn notes_directory() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("IPET_NOTES_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = home_directory().ok_or_else(|| "could not resolve home directory".to_string())?;
    Ok(home.join(".ipet").join("notes"))
}

#[cfg(target_os = "windows")]
fn home_directory() -> Option<PathBuf> {
    std::env::var("USERPROFILE").ok().map(PathBuf::from)
}

#[cfg(not(target_os = "windows"))]
fn home_directory() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "note".to_string()
    } else {
        trimmed.to_string()
    }
}

// --- weather_lookup (keyless, wttr.in) -----------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeatherLookupArgs {
    pub city: String,
    #[serde(default)]
    pub units: Option<String>,
}

pub fn weather_lookup(args: WeatherLookupArgs) -> Result<String, String> {
    let city = args.city.trim();
    if city.is_empty() {
        return Err("city must not be empty".to_string());
    }
    let fahrenheit = args
        .units
        .map(|u| u.eq_ignore_ascii_case("f"))
        .unwrap_or(false);
    let url = format!("https://wttr.in/{}?format=j1", url_encode(city));
    let body = http_get_text(&url, LOCAL_HTTP_TIMEOUT_SECS)?;
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| format!("wttr.in returned non-JSON: {err}"))?;
    let cur = value
        .get("current_condition")
        .and_then(|v| v.get(0))
        .ok_or_else(|| "wttr.in: no current_condition".to_string())?;
    let area_name = value
        .get("nearest_area")
        .and_then(|v| v.get(0))
        .and_then(|a| a.get("areaName"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.as_str())
        .unwrap_or(city);
    let temp_key = if fahrenheit { "temp_F" } else { "temp_C" };
    let feels_key = if fahrenheit { "FeelsLikeF" } else { "FeelsLikeC" };
    let unit_label = if fahrenheit { "F" } else { "C" };
    let pick = |key: &str| cur.get(key).and_then(|v| v.as_str()).unwrap_or("?");
    let desc = cur
        .get("weatherDesc")
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    serde_json::to_string(&json!({
        "city": area_name,
        "description": desc,
        "temperature": format!("{}°{}", pick(temp_key), unit_label),
        "feelsLike": format!("{}°{}", pick(feels_key), unit_label),
        "humidity": format!("{}%", pick("humidity")),
        "windKmph": pick("windspeedKmph"),
        "units": if fahrenheit { "f" } else { "c" },
        "source": "wttr.in"
    }))
    .map_err(|err| err.to_string())
}

// --- web_search (keyless, DuckDuckGo Instant Answer) ---------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchArgs {
    pub query: String,
    #[serde(default)]
    pub max_results: Option<usize>,
}

pub fn web_search(args: WebSearchArgs) -> Result<String, String> {
    let query = args.query.trim();
    if query.is_empty() {
        return Err("query must not be empty".to_string());
    }
    let max = args.max_results.unwrap_or(8).clamp(1, 20);
    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1&t=ipet",
        url_encode(query)
    );
    let body = http_get_text(&url, LOCAL_HTTP_TIMEOUT_SECS)?;
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| format!("DuckDuckGo non-JSON: {err}"))?;
    let mut results: Vec<serde_json::Value> = Vec::new();
    if let Some(topics) = value.get("RelatedTopics").and_then(|v| v.as_array()) {
        for topic in topics {
            if results.len() >= max {
                break;
            }
            if let Some(text) = topic.get("Text").and_then(|v| v.as_str()) {
                push_search_result(&mut results, text, topic.get("FirstURL"), max);
            } else if let Some(nested) = topic.get("Topics").and_then(|v| v.as_array()) {
                for sub in nested {
                    if results.len() >= max {
                        break;
                    }
                    if let Some(text) = sub.get("Text").and_then(|v| v.as_str()) {
                        push_search_result(&mut results, text, sub.get("FirstURL"), max);
                    }
                }
            }
        }
    }
    let abstract_text = value
        .get("AbstractText")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    serde_json::to_string(&json!({
        "query": query,
        "resultCount": results.len(),
        "abstract": abstract_text,
        "results": results,
        "source": "DuckDuckGo Instant Answer",
        "note": "DuckDuckGo Instant Answer 仅返回摘要/百科类结果，非完整网页搜索。"
    }))
    .map_err(|err| err.to_string())
}

fn push_search_result(
    out: &mut Vec<serde_json::Value>,
    text: &str,
    url: Option<&serde_json::Value>,
    max: usize,
) {
    if out.len() >= max {
        return;
    }
    let url_str = url.and_then(|v| v.as_str()).unwrap_or("");
    out.push(json!({
        "title": truncate_chars(text, 120),
        "url": url_str,
        "snippet": truncate_chars(text, 300)
    }));
}

fn http_get_text(url: &str, timeout_secs: u64) -> Result<String, String> {
    let response = ureq::get(url)
        .timeout(Duration::from_secs(timeout_secs))
        .call()
        .map_err(|err| format!("HTTP request failed: {err}"))?;
    let status = response.status();
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status} for {url}"));
    }
    let body = response
        .into_string()
        .map_err(|err| format!("read body: {err}"))?;
    Ok(truncate_chars(&body, LOCAL_HTTP_MAX_BODY_CHARS))
}

fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

// --- screenshot_ocr ------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotOcrArgs {
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub max_chars: Option<usize>,
}

pub fn screenshot_ocr(args: ScreenshotOcrArgs) -> Result<String, String> {
    let lang = args.language.unwrap_or_else(|| "eng".to_string());
    let max_chars = args.max_chars.unwrap_or(20_000).clamp(1, 100_000);
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let png_path = std::env::temp_dir().join(format!("ipet-shot-{nanos}.png"));
    capture_screen(&png_path)?;
    let text = ocr_file(&png_path, &lang)?;
    let _ = fs::remove_file(&png_path);
    let total = text.chars().count();
    serde_json::to_string(&json!({
        "platform": std::env::consts::OS,
        "language": lang,
        "length": total,
        "truncated": total > max_chars,
        "text": truncate_chars(&text, max_chars)
    }))
    .map_err(|err| err.to_string())
}

fn capture_screen(png_path: &Path) -> Result<(), String> {
    let path_str = png_path.display().to_string();
    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $b = New-Object System.Drawing.Bitmap([System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width, [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height); $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(0, 0, 0, 0, $b.Size); $b.Save('{path_str}'); $g.Dispose(); $b.Dispose()"
        );
        let out = run_command(
            Path::new("."),
            "powershell",
            &["-NoProfile", "-Command", script.as_str()],
        )?;
        if !png_path.is_file() {
            return Err(format!("screen capture failed: {}", out.stderr));
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let out = run_command(Path::new("."), "screencapture", &["-x", path_str.as_str()])?;
        if !png_path.is_file() {
            return Err(format!("screencapture failed: {}", out.stderr));
        }
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if run_command(Path::new("."), "scrot", &[path_str.as_str()])
            .map(|o| o.status_success)
            .unwrap_or(false)
            && png_path.is_file()
        {
            return Ok(());
        }
        if run_command(Path::new("."), "gnome-screenshot", &["-f", path_str.as_str()])
            .map(|o| o.status_success)
            .unwrap_or(false)
            && png_path.is_file()
        {
            return Ok(());
        }
        let out = run_command(
            Path::new("."),
            "import",
            &["-window", "root", path_str.as_str()],
        )?;
        if !png_path.is_file() {
            return Err(format!(
                "no screen capture tool available (tried scrot/gnome-screenshot/import): {}",
                out.stderr
            ));
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        Err("screen capture unsupported on this OS".to_string())
    }
}

fn ocr_file(png_path: &Path, lang: &str) -> Result<String, String> {
    let path_str = png_path.display().to_string();
    let out = run_command(
        Path::new("."),
        "tesseract",
        &[path_str.as_str(), "stdout", "-l", lang],
    )?;
    if !out.status_success {
        return Err(format!(
            "tesseract failed (is tesseract installed?): {}",
            out.stderr
        ));
    }
    Ok(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ipet-desktop-tools-{name}-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn search_files_finds_name_matches() {
        let dir = temp_dir("search");
        fs::write(dir.join("notes.txt"), "hello").unwrap();
        let out = search_files(SearchFilesArgs {
            path: dir.display().to_string(),
            query: "notes".to_string(),
            search_content: None,
            include_hidden: None,
            max_results: None,
            max_file_bytes: None,
            file_extensions: None,
        })
        .unwrap();
        assert!(out.contains("notes.txt"));
    }

    #[test]
    fn read_text_file_respects_line_window() {
        let dir = temp_dir("read");
        let file = dir.join("a.txt");
        fs::write(&file, "a\nb\nc\n").unwrap();
        let out = read_text_file(ReadTextFileArgs {
            path: file.display().to_string(),
            max_bytes: None,
            start_line: Some(2),
            max_lines: Some(1),
        })
        .unwrap();
        assert!(out.contains("2: b"));
        assert!(!out.contains("1: a"));
    }

    #[test]
    fn slugify_lowercases_and_dashes_non_alnum() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("   "), "note");
        assert_eq!(slugify("a///b"), "a-b");
    }

    #[test]
    fn url_encode_percent_encodes_reserved() {
        assert_eq!(url_encode("Tokyo"), "Tokyo");
        assert_eq!(url_encode("a b/c"), "a%20b%2Fc");
        assert_eq!(url_encode("中文"), "%E4%B8%AD%E6%96%87");
    }

    #[test]
    fn package_scripts_reads_scripts_object() {
        let dir = temp_dir("pkgscripts");
        fs::write(
            dir.join("package.json"),
            r#"{"name":"demo","version":"1.2.3","scripts":{"test":"vitest run","build":"vite build"}}"#,
        )
        .unwrap();
        let out = package_scripts(PackageScriptsArgs { path: Some(dir.display().to_string()) }).unwrap();
        assert!(out.contains("\"name\":\"demo\""));
        assert!(out.contains("vitest run"));
        assert!(out.contains("vite build"));
    }

    #[test]
    fn package_scripts_errors_when_missing() {
        let dir = temp_dir("pkgscripts-missing");
        let err = package_scripts(PackageScriptsArgs { path: Some(dir.display().to_string()) }).unwrap_err();
        assert!(err.contains("no package.json"));
    }

    #[test]
    fn run_project_check_rejects_unallowlisted_command() {
        let err = run_project_check(RunProjectCheckArgs {
            path: None,
            command: "rm -rf /".to_string(),
            timeout_secs: None,
        })
        .unwrap_err();
        assert!(err.contains("allow-list"), "got: {err}");
    }

    #[test]
    fn run_project_check_rejects_empty_command() {
        let err = run_project_check(RunProjectCheckArgs {
            path: None,
            command: "   ".to_string(),
            timeout_secs: None,
        })
        .unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn create_note_writes_and_appends_markdown() {
        // Merged into one test because IPET_NOTES_DIR is process-global; two
        // tests mutating it in parallel would race. Slug derives from the
        // title, so a second call with the same title + append=true extends
        // the same file instead of creating a new one.
        let dir = temp_dir("note");
        std::env::set_var("IPET_NOTES_DIR", dir.as_os_str());

        let out = create_note(CreateNoteArgs {
            title: "My Note Title".to_string(),
            content: "hello world".to_string(),
            append: None,
        })
        .unwrap();
        assert!(out.contains("my-note-title.md"));
        let written = fs::read_to_string(dir.join("my-note-title.md")).unwrap();
        assert!(written.contains("# My Note Title"));
        assert!(written.contains("hello world"));

        create_note(CreateNoteArgs {
            title: "Journal".to_string(),
            content: "one".to_string(),
            append: None,
        })
        .unwrap();
        create_note(CreateNoteArgs {
            title: "Journal".to_string(),
            content: "two".to_string(),
            append: Some(true),
        })
        .unwrap();
        let journal = fs::read_to_string(dir.join("journal.md")).unwrap();
        assert!(journal.contains("one"), "original content kept: {journal}");
        assert!(journal.contains("## Journal"), "append adds a sub-heading: {journal}");
        assert!(journal.contains("two"), "appended content present: {journal}");

        std::env::remove_var("IPET_NOTES_DIR");
    }
}
