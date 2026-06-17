//! `tool_configs` table — builtin + custom tool registry.

use super::{
    read_tool_row, validate_tool_input, AppError, AppResult, Storage, ToolConfig, ToolConfigInput,
};
use chrono::Utc;
use rusqlite::{params, OptionalExtension};

impl Storage {
    pub fn list_tools(&self) -> AppResult<Vec<ToolConfig>> {
        let conn = self.lock()?;
        let mut stmt = conn.prepare(
            "SELECT name, display_name, description, kind, enabled, built_in,
                    parameters_json, http_json, updated_at, local_json
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
                    parameters_json, http_json, updated_at, local_json
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
        let local_json = input
            .local
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let parameters_json = serde_json::to_string_pretty(&input.parameters)?;
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO tool_configs
                (name, display_name, description, kind, enabled, built_in,
                 parameters_json, http_json, local_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(name) DO UPDATE SET
                display_name = excluded.display_name,
                description = excluded.description,
                kind = excluded.kind,
                enabled = excluded.enabled,
                parameters_json = excluded.parameters_json,
                http_json = excluded.http_json,
                local_json = excluded.local_json,
                updated_at = excluded.updated_at",
            params![
                input.name,
                input.display_name,
                input.description,
                input.kind,
                i64::from(input.enabled),
                parameters_json,
                http_json,
                local_json,
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

    /// Upsert a builtin tool, refreshing its metadata from the embedded
    /// `tool.json` manifest on each start. Custom columns (enabled for
    /// custom tools) are untouched.
    pub(super) fn upsert_builtin_tool(&self, tool: &ToolConfig) -> AppResult<()> {
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
}
