//! SQLite persistence for iPet.
//!
//! The [`Storage`] type owns a single SQLite connection guarded by a mutex and
//! exposes one method family per data domain. Domain methods live in sibling
//! modules (`chat`, `tools`, `token_usage`, `caches`, `preferences`) that each
//! add to `impl Storage`; this module owns the struct, connection lifecycle,
//! schema migrations, builtin-tool seeding, and retention sweeps.

use crate::app_error::{AppError, AppResult};
use crate::secret::MachineKey;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

mod caches;
mod chat;
mod preferences;
mod token_usage;
mod tools;

/// Builtin tool manifests, embedded at compile time so the `tool.json` files
/// under `tool-packages/` are the single source of truth for builtin tool
/// metadata (name, description, parameter schema). The same files ship as
/// distributable packages, so runtime and distribution never drift.
const SCAN_DISK_TOOL_JSON: &str = include_str!("../../../tool-packages/scan_disk/tool.json");
const SYSTEM_STATUS_TOOL_JSON: &str =
    include_str!("../../../tool-packages/get_system_status/tool.json");

/// Minimal projection of a `tool.json` manifest — just the fields needed to
/// seed a builtin `ToolConfig`. The full manifest (runtime, permissions,
/// safety, version) is distributable metadata that doesn't live in the DB.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuiltinToolManifest {
    name: String,
    display_name: String,
    description: String,
    #[serde(default = "default_true")]
    enabled: bool,
    parameters: Value,
}

fn default_true() -> bool {
    true
}

fn builtin_tool_from_manifest(raw: &str) -> AppResult<ToolConfig> {
    let manifest: BuiltinToolManifest = serde_json::from_str(raw).map_err(|err| {
        AppError::InvalidInput(format!("解析内置工具 manifest 失败: {err}"))
    })?;
    Ok(ToolConfig {
        name: manifest.name,
        display_name: manifest.display_name,
        description: manifest.description,
        kind: "builtin".to_string(),
        enabled: manifest.enabled,
        built_in: true,
        parameters: manifest.parameters,
        http: None,
        local: None,
        updated_at: Utc::now().to_rfc3339(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRecord {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolHeader {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpToolConfig {
    pub method: String,
    pub url: String,
    pub headers: Vec<ToolHeader>,
}

/// Configuration for a `kind: "local"` tool — an executable or script the
/// dispatcher spawns per call, talking JSON over stdin/stdout. See
/// `docs/TOOL_PACKAGE.md` §local.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalToolConfig {
    /// Executable to run (PATH-resolved, or an absolute path). Relative paths
    /// are resolved against the package directory at import time and stored
    /// absolute, so the tool keeps working regardless of the host's CWD.
    pub command: String,
    /// Extra args appended after `command`. Args may reference the model's
    /// arguments via the `$ARGS` placeholder? No — arguments travel on stdin
    /// to avoid shell-injection; args are static.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for the child. Defaults to the package dir at import
    /// time (resolved to absolute), or the host CWD if unset.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Hard kill deadline for the child, in seconds. Defaults to 30.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub kind: String,
    pub enabled: bool,
    pub built_in: bool,
    pub parameters: Value,
    pub http: Option<HttpToolConfig>,
    pub local: Option<LocalToolConfig>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfigInput {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub kind: String,
    pub enabled: bool,
    pub parameters: Value,
    pub http: Option<HttpToolConfig>,
    #[serde(default)]
    pub local: Option<LocalToolConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageRecord {
    pub id: i64,
    pub request_id: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub tool_calls: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageBucket {
    pub label: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub requests: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageStats {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub requests: i64,
    pub tool_calls: i64,
    pub by_day: Vec<TokenUsageBucket>,
    pub by_model: Vec<TokenUsageBucket>,
    pub recent: Vec<TokenUsageRecord>,
}

pub struct Storage {
    db_path: PathBuf,
    conn: Mutex<Connection>,
    /// Optional at-rest crypto key. When present, sensitive fields like
    /// `LlmSettings.api_key` are stored in the DB as an `enc:v1:...` envelope
    /// instead of plaintext. Legacy plaintext values continue to load via
    /// `MachineKey::decrypt_or_passthrough`.
    ///
    /// `pub(crate)` so the domain modules (`chat`, `tools`, …) under
    /// `storage/` can read/encrypt the api_key; nothing outside this crate
    /// touches it.
    pub(crate) secret: Option<MachineKey>,
}

/// Configurable retention thresholds. The defaults trim everything aggressively
/// to keep the SQLite file small; consumers can override per-call via
/// `Storage::prune_with_policy`.
#[derive(Debug, Clone, Copy)]
pub struct RetentionPolicy {
    pub chat_keep: usize,
    pub token_usage_days: u32,
    pub system_samples_days: u32,
    pub disk_scan_days: u32,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            chat_keep: 2000,
            token_usage_days: 90,
            system_samples_days: 30,
            disk_scan_days: 30,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RetentionReport {
    pub chat_removed: usize,
    pub tokens_removed: usize,
    pub samples_removed: usize,
    pub disk_removed: usize,
}

/// Current schema version. Bump when adding a migration to `MIGRATIONS`.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Ordered schema migrations. `MIGRATIONS[i]` advances the DB from
/// `user_version == i` to `i + 1`. Append-only — never reorder or rewrite an
/// entry that has shipped, or existing user databases will silently skip it.
const MIGRATIONS: &[fn(&Connection) -> AppResult<()>] = &[
    // v0 -> v1: initial schema. `IF NOT EXISTS` so a legacy DB created before
    // versioning (user_version == 0) converges without error.
    |conn| -> AppResult<()> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS preferences (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS chat_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS disk_scan_cache (
                root_path TEXT PRIMARY KEY,
                result_json TEXT NOT NULL,
                scanned_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS system_samples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                sample_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tool_configs (
                name TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                description TEXT NOT NULL,
                kind TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                built_in INTEGER NOT NULL,
                parameters_json TEXT NOT NULL,
                http_json TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS token_usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL,
                completion_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                tool_calls INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    },
    // v1 -> v2: add `local_json` column for `kind: "local"` tool configs
    // (subprocess/stdio tools). Nullable — http/builtin tools leave it NULL.
    |conn| -> AppResult<()> {
        conn.execute("ALTER TABLE tool_configs ADD COLUMN local_json TEXT", [])?;
        Ok(())
    },
    // Add future v2 -> v3 migrations here as append-only entries.
];

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        Self::open_with_secret(path, None)
    }

    pub fn open_with_secret(
        path: impl AsRef<Path>,
        secret: Option<MachineKey>,
    ) -> AppResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        let storage = Self {
            db_path: path,
            conn: Mutex::new(conn),
            secret,
        };
        storage.migrate()?;
        storage.seed_builtin_tools()?;
        Ok(storage)
    }

    pub fn db_path(&self) -> PathBuf {
        self.db_path.clone()
    }

    /// Trim historical rows so the SQLite file doesn't grow forever. Safe to
    /// call on startup — runs as a few small `DELETE`s with simple ordering.
    ///
    /// Retention policy (chosen to keep the file under a few MB even after
    /// heavy use; tune `RetentionPolicy` if you want longer history):
    /// - chat_messages: keep the most recent 2000 rows
    /// - token_usage: drop rows older than 90 days
    /// - system_samples: drop rows older than 30 days
    /// - disk_scan_cache: drop entries older than 30 days
    pub fn prune_old(&self) -> AppResult<RetentionReport> {
        self.prune_with_policy(RetentionPolicy::default())
    }

    pub fn prune_with_policy(&self, policy: RetentionPolicy) -> AppResult<RetentionReport> {
        let conn = self.lock()?;
        let now = Utc::now();

        let chat_removed = conn.execute(
            "DELETE FROM chat_messages
             WHERE id NOT IN (
                 SELECT id FROM chat_messages
                 ORDER BY id DESC
                 LIMIT ?1
             )",
            params![policy.chat_keep as i64],
        )?;

        let token_cutoff = (now - chrono::Duration::days(policy.token_usage_days as i64))
            .to_rfc3339();
        let tokens_removed = conn.execute(
            "DELETE FROM token_usage WHERE created_at < ?1",
            params![token_cutoff],
        )?;

        let samples_cutoff = (now - chrono::Duration::days(policy.system_samples_days as i64))
            .to_rfc3339();
        let samples_removed = conn.execute(
            "DELETE FROM system_samples WHERE created_at < ?1",
            params![samples_cutoff],
        )?;

        let disk_cutoff = (now - chrono::Duration::days(policy.disk_scan_days as i64))
            .to_rfc3339();
        let disk_removed = conn.execute(
            "DELETE FROM disk_scan_cache WHERE scanned_at < ?1",
            params![disk_cutoff],
        )?;

        Ok(RetentionReport {
            chat_removed,
            tokens_removed,
            samples_removed,
            disk_removed,
        })
    }

    /// Run schema migrations forward to the current `SCHEMA_VERSION`.
    ///
    /// Migrations are versioned via `PRAGMA user_version`: each migration
    /// bumps the version by one and only runs on DBs below that version.
    /// New schemas append a migration fn to `MIGRATIONS`; never edit an
    /// already-shipped migration in place — existing user DBs would skip it.
    ///
    /// `IF NOT EXISTS` on the v1 tables makes a brand-new DB and a legacy DB
    /// created before versioning was introduced both converge to v1 without
    /// error (legacy DBs report `user_version = 0`).
    fn migrate(&self) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;

        let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        for (version, migration) in MIGRATIONS.iter().enumerate() {
            let target = (version + 1) as i64;
            if current < target {
                migration(&conn)?;
                conn.execute_batch(&format!("PRAGMA user_version = {target}"))?;
                tracing::info!(from = current, to = target, "storage schema migrated");
            }
        }
        Ok(())
    }

    fn seed_builtin_tools(&self) -> AppResult<()> {
        // The builtin tool manifests are embedded from tool-packages/*/tool.json
        // (see `SCAN_DISK_TOOL_JSON` / `SYSTEM_STATUS_TOOL_JSON`), so editing a
        // tool's schema in its package is the only place that change needs to
        // land — the runtime seed picks it up on next start.
        let tools = [
            builtin_tool_from_manifest(SYSTEM_STATUS_TOOL_JSON)?,
            builtin_tool_from_manifest(SCAN_DISK_TOOL_JSON)?,
        ];

        for tool in tools {
            self.upsert_builtin_tool(&tool)?;
        }
        Ok(())
    }

    /// Acquire the connection guard. `pub(crate)` so the domain modules
    /// (`chat`, `tools`, …) under `storage/` can run queries against the
    /// shared connection.
    pub(crate) fn lock(
        &self,
    ) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| AppError::Config("database lock poisoned".to_string()))
    }
}

/// Read a `tool_configs` row into a [`ToolConfig`]. `pub(super)` so the
/// `tools` module can reuse it across list/get queries.
pub(crate) fn read_tool_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolConfig> {
    let parameters_json: String = row.get(6)?;
    let http_json: Option<String> = row.get(7)?;
    let local_json: Option<String> = row.get(9)?;
    Ok(ToolConfig {
        name: row.get(0)?,
        display_name: row.get(1)?,
        description: row.get(2)?,
        kind: row.get(3)?,
        enabled: row.get::<_, i64>(4)? != 0,
        built_in: row.get::<_, i64>(5)? != 0,
        parameters: serde_json::from_str(&parameters_json).unwrap_or_else(|_| json!({
            "type": "object",
            "properties": {}
        })),
        http: http_json.and_then(|raw| serde_json::from_str(&raw).ok()),
        local: local_json.and_then(|raw| serde_json::from_str(&raw).ok()),
        updated_at: row.get(8)?,
    })
}

/// Aggregate `token_usage` into labeled buckets for the stats view.
pub(crate) fn query_usage_buckets(
    conn: &Connection,
    sql: &str,
) -> AppResult<Vec<TokenUsageBucket>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(TokenUsageBucket {
            label: row.get(0)?,
            prompt_tokens: row.get(1)?,
            completion_tokens: row.get(2)?,
            total_tokens: row.get(3)?,
            requests: row.get(4)?,
        })
    })?;

    let mut buckets = Vec::new();
    for row in rows {
        buckets.push(row?);
    }
    Ok(buckets)
}

/// Validate a custom tool before persisting. `pub(super)` so the `tools`
/// module can call it from `save_custom_tool`.
pub(crate) fn validate_tool_input(input: &ToolConfigInput) -> AppResult<()> {
    if !is_valid_tool_name(&input.name) {
        return Err(AppError::InvalidInput(
            "工具名称只能包含英文字母、数字和下划线，且必须以字母或下划线开头".to_string(),
        ));
    }
    if input.display_name.trim().is_empty() {
        return Err(AppError::InvalidInput("工具显示名不能为空".to_string()));
    }
    if input.description.trim().is_empty() {
        return Err(AppError::InvalidInput("工具描述不能为空".to_string()));
    }
    if input
        .parameters
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        != "object"
    {
        return Err(AppError::InvalidInput(
            "工具 parameters 必须是 JSON Schema object".to_string(),
        ));
    }
    match input.kind.as_str() {
        "http" => validate_http_tool(input)?,
        "local" => validate_local_tool(input)?,
        other => {
            return Err(AppError::InvalidInput(format!(
                "不支持的工具类型: {other}（当前支持 http / local）"
            )))
        }
    }
    Ok(())
}

fn validate_http_tool(input: &ToolConfigInput) -> AppResult<()> {
    let http = input
        .http
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput("HTTP 工具必须配置 http".to_string()))?;
    let method = http.method.to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "POST" | "PUT" | "PATCH") {
        return Err(AppError::InvalidInput(
            "HTTP 工具 method 仅支持 GET/POST/PUT/PATCH".to_string(),
        ));
    }
    crate::http_safety::validate_url_syntax(&http.url)?;
    Ok(())
}

/// Validate a `kind: "local"` tool. The command must be non-empty; relative
/// commands are resolved against PATH by the OS at spawn time. Absolute paths
/// are sanity-checked to exist. `cwd`, if absolute, must exist; relative cwd
/// is resolved against the host CWD at dispatch time (not validated here).
fn validate_local_tool(input: &ToolConfigInput) -> AppResult<()> {
    let local = input
        .local
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput("local 工具必须配置 local".to_string()))?;
    if local.command.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "local 工具的 command 不能为空".to_string(),
        ));
    }
    // Absolute command path must point at something runnable. Bare names
    // (python, node, ./script) are left to PATH / the shell at spawn time.
    let cmd_path = std::path::Path::new(&local.command);
    if cmd_path.is_absolute() && !cmd_path.exists() {
        return Err(AppError::InvalidInput(format!(
            "local 工具 command 路径不存在: {}",
            local.command
        )));
    }
    if let Some(cwd) = local.cwd.as_ref() {
        let cwd_path = std::path::Path::new(cwd);
        if cwd_path.is_absolute() && !cwd_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "local 工具 cwd 路径不存在: {cwd}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn is_valid_tool_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LlmSettings;
    use crate::testutil::TempDir;

    fn fresh_storage() -> (TempDir, Storage) {
        let dir = TempDir::new("storage");
        let storage = Storage::open(dir.path().join("ipet-test.sqlite3"))
            .expect("storage must open on a fresh temp path");
        (dir, storage)
    }

    fn encrypted_storage() -> (TempDir, Storage) {
        let dir = TempDir::new("storage-enc");
        let key = crate::secret::MachineKey::load_or_generate(dir.path())
            .expect("machine key must initialize");
        let storage = Storage::open_with_secret(dir.path().join("ipet-test.sqlite3"), Some(key))
            .expect("encrypted storage must open");
        (dir, storage)
    }

    #[test]
    fn fresh_db_reaches_current_schema_version() {
        let (_dir, storage) = fresh_storage();
        let version: i64 = storage
            .lock()
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION, "fresh DB should be fully migrated");
    }

    #[test]
    fn legacy_unversioned_db_converges_on_open() {
        // Simulate a DB created before versioning existed: tables present,
        // but user_version == 0. Storage::open must bring it forward without
        // error and without dropping the pre-existing row.
        let dir = TempDir::new("legacy-db");
        let db_path = dir.path().join("ipet-legacy.sqlite3");
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);
                 INSERT INTO preferences (key, value, updated_at) VALUES ('k', 'v', '1970-01-01T00:00:00Z');",
            )
            .unwrap();
            // deliberately leave user_version at its default of 0
        }
        let storage = Storage::open(&db_path).expect("legacy DB must migrate on open");

        let version: i64 = storage
            .lock()
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);

        // The legacy row survived the migration.
        let value: String = storage
            .lock()
            .unwrap()
            .query_row("SELECT value FROM preferences WHERE key = 'k'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(value, "v");
    }

    fn http_tool_input(name: &str, url: &str) -> ToolConfigInput {
        ToolConfigInput {
            name: name.to_string(),
            display_name: format!("display-{name}"),
            description: "a test tool".to_string(),
            kind: "http".to_string(),
            enabled: true,
            parameters: json!({"type": "object", "properties": {}}),
            http: Some(HttpToolConfig {
                method: "GET".to_string(),
                url: url.to_string(),
                headers: vec![],
            }),
            local: None,
        }
    }

    #[test]
    fn open_seeds_builtin_tools() {
        let (_dir, storage) = fresh_storage();
        let tools = storage.list_tools().unwrap();
        let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&"get_system_status") && names.contains(&"scan_disk"),
            "missing built-in tools, got {names:?}"
        );
        assert!(
            tools.iter().filter(|t| t.built_in).count() >= 2,
            "built-in tools must keep their built_in flag"
        );
    }

    #[test]
    fn llm_settings_roundtrip_preserves_fields() {
        let (_dir, storage) = fresh_storage();
        let mut settings = LlmSettings::default();
        settings.api_key = Some("sk-roundtrip".into());
        settings.model = "test-model".into();
        settings.temperature = 0.42;
        settings.max_context_messages = 12;
        storage.save_llm_settings(&settings).unwrap();

        let loaded = storage.load_llm_settings().unwrap();
        assert_eq!(loaded.api_key.as_deref(), Some("sk-roundtrip"));
        assert_eq!(loaded.model, "test-model");
        assert!((loaded.temperature - 0.42).abs() < f32::EPSILON);
        assert_eq!(loaded.max_context_messages, 12);
    }

    #[test]
    fn save_llm_settings_rejects_invalid_fields() {
        let (_dir, storage) = fresh_storage();
        let mut bad = LlmSettings::default();
        bad.model = "   ".into();
        let err = storage.save_llm_settings(&bad).unwrap_err();
        assert!(matches!(err, AppError::Config(_)), "got {err:?}");
    }

    #[test]
    fn chat_messages_round_trip_in_chronological_order() {
        let (_dir, storage) = fresh_storage();
        storage.save_chat_message("user", "hello").unwrap();
        storage.save_chat_message("assistant", "hi there").unwrap();
        storage.save_chat_message("user", "how are you").unwrap();

        let recent = storage.recent_messages(10).unwrap();
        assert_eq!(recent.len(), 3);
        // recent_messages returns oldest-first after the internal reverse.
        let contents: Vec<_> = recent.iter().map(|r| r.content.as_str()).collect();
        assert_eq!(contents, vec!["hello", "hi there", "how are you"]);
    }

    #[test]
    fn recent_messages_respects_limit() {
        let (_dir, storage) = fresh_storage();
        for i in 0..5 {
            storage.save_chat_message("user", &format!("msg-{i}")).unwrap();
        }
        let recent = storage.recent_messages(2).unwrap();
        assert_eq!(recent.len(), 2);
        // After the reverse, we should see the two newest in chronological order.
        let contents: Vec<_> = recent.iter().map(|r| r.content.as_str()).collect();
        assert_eq!(contents, vec!["msg-3", "msg-4"]);
    }

    #[test]
    fn save_custom_tool_rejects_overriding_builtin() {
        let (_dir, storage) = fresh_storage();
        let input = http_tool_input("get_system_status", "https://example.com/x");
        let err = storage.save_custom_tool(input).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn save_custom_tool_rejects_disallowed_url() {
        let (_dir, storage) = fresh_storage();
        let input = http_tool_input("my_tool", "http://127.0.0.1/admin");
        let err = storage.save_custom_tool(input).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)), "got {err:?}");
    }

    #[test]
    fn local_tool_round_trips_through_storage() {
        let (_dir, storage) = fresh_storage();
        let input = ToolConfigInput {
            name: "my_local".to_string(),
            display_name: "本地".to_string(),
            description: "a local tool".to_string(),
            kind: "local".to_string(),
            enabled: true,
            parameters: json!({"type": "object", "properties": {}}),
            http: None,
            local: Some(LocalToolConfig {
                command: "node".to_string(),
                args: vec!["script.js".to_string()],
                cwd: None,
                timeout_secs: Some(20),
            }),
        };
        let saved = storage.save_custom_tool(input).unwrap();
        assert_eq!(saved.kind, "local");
        let local = saved.local.as_ref().expect("local config persisted");
        assert_eq!(local.command, "node");
        assert_eq!(local.args, vec!["script.js".to_string()]);
        assert_eq!(local.timeout_secs, Some(20));
        assert!(saved.http.is_none());

        // Reload from DB to confirm the local_json column survives the round trip.
        let reloaded = storage.get_tool("my_local").unwrap().unwrap();
        assert_eq!(
            reloaded.local.as_ref().unwrap().command,
            "node",
            "local config must round-trip through local_json"
        );
    }

    #[test]
    fn local_tool_validation_rejects_empty_command() {
        let (_dir, storage) = fresh_storage();
        let input = ToolConfigInput {
            name: "bad_local".to_string(),
            display_name: "Bad".to_string(),
            description: "d".to_string(),
            kind: "local".to_string(),
            enabled: true,
            parameters: json!({"type": "object"}),
            http: None,
            local: Some(LocalToolConfig {
                command: "   ".to_string(),
                args: vec![],
                cwd: None,
                timeout_secs: None,
            }),
        };
        let err = storage.save_custom_tool(input).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn save_and_toggle_custom_http_tool() {
        let (_dir, storage) = fresh_storage();
        let input = http_tool_input("my_tool", "https://example.com/api");
        let saved = storage.save_custom_tool(input).unwrap();
        assert_eq!(saved.name, "my_tool");
        assert!(saved.enabled);
        assert!(!saved.built_in);

        let toggled = storage.set_tool_enabled("my_tool", false).unwrap();
        assert!(!toggled.enabled);

        storage.delete_tool("my_tool").unwrap();
        assert!(storage.get_tool("my_tool").unwrap().is_none());
    }

    #[test]
    fn delete_tool_refuses_to_remove_builtin() {
        let (_dir, storage) = fresh_storage();
        let err = storage.delete_tool("get_system_status").unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn token_stats_accumulates_usage() {
        let (_dir, storage) = fresh_storage();
        storage
            .record_token_usage("req-1", "model-a", 10, 20, 30, 1)
            .unwrap();
        storage
            .record_token_usage("req-2", "model-a", 5, 5, 10, 0)
            .unwrap();
        let stats = storage.token_stats().unwrap();
        assert_eq!(stats.prompt_tokens, 15);
        assert_eq!(stats.completion_tokens, 25);
        assert_eq!(stats.total_tokens, 40);
        assert_eq!(stats.requests, 2);
        assert_eq!(stats.tool_calls, 1);
        assert!(stats.recent.iter().any(|r| r.request_id == "req-1"));
    }

    #[test]
    fn invalid_tool_name_rejected() {
        assert!(!is_valid_tool_name(""));
        assert!(!is_valid_tool_name("1starts_with_digit"));
        assert!(!is_valid_tool_name("has-dash"));
        assert!(!is_valid_tool_name("has space"));
        assert!(is_valid_tool_name("_ok"));
        assert!(is_valid_tool_name("get_status_v2"));
    }

    #[test]
    fn prune_caps_chat_history_to_policy() {
        let (_dir, storage) = fresh_storage();
        for i in 0..30 {
            storage.save_chat_message("user", &format!("msg-{i}")).unwrap();
        }
        let policy = RetentionPolicy {
            chat_keep: 10,
            ..RetentionPolicy::default()
        };
        let report = storage.prune_with_policy(policy).unwrap();
        assert_eq!(report.chat_removed, 20);
        let recent = storage.recent_messages(100).unwrap();
        assert_eq!(recent.len(), 10);
        // recent_messages reverses inside, so it's chronological — oldest of
        // the survivors should be msg-20, newest msg-29.
        assert_eq!(recent.first().unwrap().content, "msg-20");
        assert_eq!(recent.last().unwrap().content, "msg-29");
    }

    #[test]
    fn prune_keeps_recent_token_usage() {
        let (_dir, storage) = fresh_storage();
        // Fresh inserts get Utc::now() timestamps, well within any sane
        // retention window; nothing should be removed.
        storage
            .record_token_usage("r1", "model", 1, 1, 2, 0)
            .unwrap();
        let report = storage.prune_old().unwrap();
        assert_eq!(report.tokens_removed, 0);
        let stats = storage.token_stats().unwrap();
        assert_eq!(stats.requests, 1);
    }

    #[test]
    fn session_value_round_trips() {
        let (_dir, storage) = fresh_storage();
        assert!(storage.get_session_value("missing").unwrap().is_none());
        storage.set_session_value("ipet:test", "hello,world").unwrap();
        assert_eq!(
            storage.get_session_value("ipet:test").unwrap().as_deref(),
            Some("hello,world")
        );
        // Re-setting overwrites instead of failing.
        storage.set_session_value("ipet:test", "v2").unwrap();
        assert_eq!(
            storage.get_session_value("ipet:test").unwrap().as_deref(),
            Some("v2")
        );
    }

    #[test]
    fn encrypted_storage_round_trips_api_key() {
        let (_dir, storage) = encrypted_storage();
        let mut settings = LlmSettings::default();
        settings.api_key = Some("sk-roundtrip-encrypted".into());
        storage.save_llm_settings(&settings).unwrap();

        // On the wire, the persisted JSON must be encrypted (no plaintext).
        let raw = storage
            .get_session_value("llm_settings")
            .unwrap()
            .expect("settings row must exist");
        assert!(
            !raw.contains("sk-roundtrip-encrypted"),
            "plaintext api_key leaked into stored JSON: {raw}"
        );
        assert!(
            raw.contains("enc:v1:"),
            "encrypted envelope marker missing: {raw}"
        );

        // Reads should return the original plaintext.
        let back = storage.load_llm_settings().unwrap();
        assert_eq!(back.api_key.as_deref(), Some("sk-roundtrip-encrypted"));
    }

    #[test]
    fn encrypted_storage_passes_through_legacy_plaintext() {
        // Simulate an old DB by writing settings with the encrypted storage
        // disabled, then reading them back with encryption enabled.
        let dir = TempDir::new("legacy-key");
        let db = dir.path().join("ipet-test.sqlite3");
        {
            let plain = Storage::open(&db).unwrap();
            let mut settings = LlmSettings::default();
            settings.api_key = Some("sk-legacy".into());
            plain.save_llm_settings(&settings).unwrap();
        }

        let key = crate::secret::MachineKey::load_or_generate(dir.path()).unwrap();
        let enc = Storage::open_with_secret(&db, Some(key)).unwrap();
        let loaded = enc.load_llm_settings().unwrap();
        assert_eq!(
            loaded.api_key.as_deref(),
            Some("sk-legacy"),
            "legacy plaintext key must still load"
        );

        // And once we save it back, it should now be persisted encrypted.
        enc.save_llm_settings(&loaded).unwrap();
        let raw = enc.get_session_value("llm_settings").unwrap().unwrap();
        assert!(raw.contains("enc:v1:"), "save did not upgrade to encrypted form");
        assert!(!raw.contains("sk-legacy"));
    }
}
