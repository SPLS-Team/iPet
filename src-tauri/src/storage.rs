use crate::app_error::{AppError, AppResult};
use crate::config::LlmSettings;
use crate::secret::MachineKey;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Builtin tool manifests, embedded at compile time so the `tool.json` files
/// under `tool-packages/` are the single source of truth for builtin tool
/// metadata (name, description, parameter schema). The same files ship as
/// distributable packages, so runtime and distribution never drift.
const SCAN_DISK_TOOL_JSON: &str = include_str!("../../tool-packages/scan_disk/tool.json");
const SYSTEM_STATUS_TOOL_JSON: &str =
    include_str!("../../tool-packages/get_system_status/tool.json");

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
}

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageBucket {
    pub label: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
    pub requests: i64,
}

#[derive(Debug, Clone, Serialize)]
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
    secret: Option<MachineKey>,
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

    fn migrate(&self) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

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

    pub fn load_llm_settings(&self) -> AppResult<LlmSettings> {
        let conn = self.lock()?;
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM preferences WHERE key = 'llm_settings'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        drop(conn);

        let mut settings: LlmSettings = match value {
            Some(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            None => LlmSettings::default(),
        };

        // If a machine key is wired in, transparently decrypt the api_key.
        // Legacy plaintext keys pass through (see decrypt_or_passthrough) so
        // upgrades don't drop existing credentials.
        if let (Some(key), Some(api_key)) = (self.secret.as_ref(), settings.api_key.as_ref()) {
            match key.decrypt_or_passthrough(api_key) {
                Ok(plain) => settings.api_key = Some(plain),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to decrypt stored api_key; clearing");
                    settings.api_key = None;
                }
            }
        }
        Ok(settings)
    }

    pub fn save_llm_settings(&self, settings: &LlmSettings) -> AppResult<()> {
        settings
            .validate_public_fields()
            .map_err(AppError::Config)?;

        // Clone so we can swap the api_key for an encrypted envelope without
        // mutating the caller's struct.
        let mut to_persist = settings.clone();
        if let (Some(key), Some(api_key)) = (self.secret.as_ref(), to_persist.api_key.as_ref()) {
            if !api_key.trim().is_empty() && !api_key.starts_with("enc:v1:") {
                to_persist.api_key = Some(key.encrypt(api_key)?);
            }
        }

        let value = serde_json::to_string_pretty(&to_persist)?;
        self.set_preference("llm_settings", &value)
    }

    pub fn save_chat_message(&self, role: &str, content: &str) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO chat_messages (role, content, created_at) VALUES (?1, ?2, ?3)",
            params![role, content, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn recent_messages(&self, limit: usize) -> AppResult<Vec<ChatRecord>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT id, role, content, created_at
             FROM chat_messages
             ORDER BY id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ChatRecord {
                id: row.get(0)?,
                role: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        records.reverse();
        Ok(records)
    }

    pub fn cache_disk_scan(&self, root_path: &str, result_json: &str) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO disk_scan_cache (root_path, result_json, scanned_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(root_path) DO UPDATE SET
                result_json = excluded.result_json,
                scanned_at = excluded.scanned_at",
            params![root_path, result_json, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn cache_system_sample(&self, sample_json: &str) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO system_samples (sample_json, created_at) VALUES (?1, ?2)",
            params![sample_json, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn list_tools(&self) -> AppResult<Vec<ToolConfig>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT name, display_name, description, kind, enabled, built_in,
                    parameters_json, http_json, updated_at
             FROM tool_configs
             ORDER BY built_in DESC, name ASC",
        )?;
        let rows = stmt.query_map([], read_tool_row)?;

        let mut tools = Vec::new();
        for row in rows {
            tools.push(row?);
        }
        Ok(tools)
    }

    pub fn active_tools(&self) -> AppResult<Vec<ToolConfig>> {
        Ok(self
            .list_tools()?
            .into_iter()
            .filter(|tool| tool.enabled)
            .collect())
    }

    pub fn get_tool(&self, name: &str) -> AppResult<Option<ToolConfig>> {
        let conn = self.lock()?;
        conn.query_row(
            "SELECT name, display_name, description, kind, enabled, built_in,
                    parameters_json, http_json, updated_at
             FROM tool_configs
             WHERE name = ?1",
            params![name],
            read_tool_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn save_custom_tool(&self, input: ToolConfigInput) -> AppResult<ToolConfig> {
        validate_tool_input(&input)?;
        if self
            .get_tool(&input.name)?
            .map(|tool| tool.built_in)
            .unwrap_or(false)
        {
            return Err(AppError::InvalidInput(
                "内置工具不能被自定义工具覆盖".to_string(),
            ));
        }

        let now = Utc::now().to_rfc3339();
        let http_json = input
            .http
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let parameters_json = serde_json::to_string_pretty(&input.parameters)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO tool_configs
                (name, display_name, description, kind, enabled, built_in,
                 parameters_json, http_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?8)
             ON CONFLICT(name) DO UPDATE SET
                display_name = excluded.display_name,
                description = excluded.description,
                kind = excluded.kind,
                enabled = excluded.enabled,
                parameters_json = excluded.parameters_json,
                http_json = excluded.http_json,
                updated_at = excluded.updated_at",
            params![
                input.name,
                input.display_name,
                input.description,
                input.kind,
                i64::from(input.enabled),
                parameters_json,
                http_json,
                now
            ],
        )?;
        drop(conn);

        self.get_tool(&input.name)?
            .ok_or_else(|| AppError::Config("工具保存后未找到".to_string()))
    }

    pub fn set_tool_enabled(&self, name: &str, enabled: bool) -> AppResult<ToolConfig> {
        let conn = self.lock()?;
        conn.execute(
            "UPDATE tool_configs SET enabled = ?1, updated_at = ?2 WHERE name = ?3",
            params![i64::from(enabled), Utc::now().to_rfc3339(), name],
        )?;
        drop(conn);
        self.get_tool(name)?
            .ok_or_else(|| AppError::InvalidInput(format!("工具不存在: {name}")))
    }

    pub fn delete_tool(&self, name: &str) -> AppResult<()> {
        if self
            .get_tool(name)?
            .map(|tool| tool.built_in)
            .unwrap_or(false)
        {
            return Err(AppError::InvalidInput("内置工具不能删除".to_string()));
        }

        let conn = self.lock()?;
        conn.execute("DELETE FROM tool_configs WHERE name = ?1", params![name])?;
        Ok(())
    }

    pub fn record_token_usage(
        &self,
        request_id: &str,
        model: &str,
        prompt_tokens: i64,
        completion_tokens: i64,
        total_tokens: i64,
        tool_calls: i64,
    ) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO token_usage
                (request_id, model, prompt_tokens, completion_tokens, total_tokens, tool_calls, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                request_id,
                model,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                tool_calls,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn token_stats(&self) -> AppResult<TokenUsageStats> {
        let conn = self.lock()?;
        let (prompt_tokens, completion_tokens, total_tokens, requests, tool_calls) = conn
            .query_row(
                "SELECT
                    COALESCE(SUM(prompt_tokens), 0),
                    COALESCE(SUM(completion_tokens), 0),
                    COALESCE(SUM(total_tokens), 0),
                    COUNT(*),
                    COALESCE(SUM(tool_calls), 0)
                 FROM token_usage",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )?;

        let by_day = query_usage_buckets(
            &conn,
            "SELECT substr(created_at, 1, 10), SUM(prompt_tokens), SUM(completion_tokens),
                    SUM(total_tokens), COUNT(*)
             FROM token_usage
             GROUP BY substr(created_at, 1, 10)
             ORDER BY substr(created_at, 1, 10) DESC
             LIMIT 14",
        )?;
        let by_model = query_usage_buckets(
            &conn,
            "SELECT model, SUM(prompt_tokens), SUM(completion_tokens), SUM(total_tokens), COUNT(*)
             FROM token_usage
             GROUP BY model
             ORDER BY SUM(total_tokens) DESC",
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, request_id, model, prompt_tokens, completion_tokens,
                    total_tokens, tool_calls, created_at
             FROM token_usage
             ORDER BY id DESC
             LIMIT 30",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TokenUsageRecord {
                id: row.get(0)?,
                request_id: row.get(1)?,
                model: row.get(2)?,
                prompt_tokens: row.get(3)?,
                completion_tokens: row.get(4)?,
                total_tokens: row.get(5)?,
                tool_calls: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        let mut recent = Vec::new();
        for row in rows {
            recent.push(row?);
        }

        Ok(TokenUsageStats {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            requests,
            tool_calls,
            by_day,
            by_model,
            recent,
        })
    }

    fn upsert_builtin_tool(&self, tool: &ToolConfig) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO tool_configs
                (name, display_name, description, kind, enabled, built_in,
                 parameters_json, http_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, NULL, ?7, ?7)
             ON CONFLICT(name) DO UPDATE SET
                display_name = excluded.display_name,
                description = excluded.description,
                kind = excluded.kind,
                built_in = 1,
                parameters_json = excluded.parameters_json",
            params![
                tool.name,
                tool.display_name,
                tool.description,
                tool.kind,
                i64::from(tool.enabled),
                serde_json::to_string_pretty(&tool.parameters)?,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    fn set_preference(&self, key: &str, value: &str) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO preferences (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at",
            params![key, value, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Persist an arbitrary string under an arbitrary key. Used for UI-side
    /// state like "the window size the user had before going compact" that
    /// doesn't deserve its own table.
    pub fn set_session_value(&self, key: &str, value: &str) -> AppResult<()> {
        self.set_preference(key, value)
    }

    pub fn get_session_value(&self, key: &str) -> AppResult<Option<String>> {
        let conn = self.lock()?;
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM preferences WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| AppError::Config("database lock poisoned".to_string()))
    }
}

fn read_tool_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolConfig> {
    let parameters_json: String = row.get(6)?;
    let http_json: Option<String> = row.get(7)?;
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
        updated_at: row.get(8)?,
    })
}

fn query_usage_buckets(conn: &Connection, sql: &str) -> AppResult<Vec<TokenUsageBucket>> {
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

fn validate_tool_input(input: &ToolConfigInput) -> AppResult<()> {
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
    if input.kind != "http" {
        return Err(AppError::InvalidInput(
            "当前自定义工具仅支持 kind=http".to_string(),
        ));
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

fn is_valid_tool_name(name: &str) -> bool {
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
