use crate::app_error::{AppError, AppResult};
use crate::config::LlmSettings;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        let storage = Self {
            db_path: path,
            conn: Mutex::new(conn),
        };
        storage.migrate()?;
        storage.seed_builtin_tools()?;
        Ok(storage)
    }

    pub fn db_path(&self) -> PathBuf {
        self.db_path.clone()
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
        let tools = vec![
            ToolConfig {
                name: "get_system_status".to_string(),
                display_name: "系统状态".to_string(),
                description: "获取当前 CPU、内存、磁盘和高占用进程概览。".to_string(),
                kind: "builtin".to_string(),
                enabled: true,
                built_in: true,
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "process_limit": {
                            "type": "integer",
                            "minimum": 3,
                            "maximum": 30,
                            "description": "返回的进程数量。"
                        }
                    }
                }),
                http: None,
                updated_at: Utc::now().to_rfc3339(),
            },
            ToolConfig {
                name: "scan_disk".to_string(),
                display_name: "磁盘扫描".to_string(),
                description: "扫描指定目录，按占用大小返回主要子目录和文件。".to_string(),
                kind: "builtin".to_string(),
                enabled: true,
                built_in: true,
                parameters: json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "要扫描的本地目录绝对路径。"
                        },
                        "max_depth": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 12,
                            "description": "递归展示深度。"
                        },
                        "max_children": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 64,
                            "description": "每层最多返回多少个子节点。"
                        }
                    }
                }),
                http: None,
                updated_at: Utc::now().to_rfc3339(),
            },
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

        match value {
            Some(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
            None => Ok(LlmSettings::default()),
        }
    }

    pub fn save_llm_settings(&self, settings: &LlmSettings) -> AppResult<()> {
        settings
            .validate_public_fields()
            .map_err(AppError::Config)?;
        let value = serde_json::to_string_pretty(settings)?;
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
    if !(http.url.starts_with("http://") || http.url.starts_with("https://")) {
        return Err(AppError::InvalidInput(
            "HTTP 工具 URL 必须以 http:// 或 https:// 开头".to_string(),
        ));
    }
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
