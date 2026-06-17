//! `preferences` table — key/value store backing LLM settings and ad-hoc
//! UI session state (e.g. the pre-compact window size).

use super::{AppError, AppResult, Storage};
use crate::config::LlmSettings;
use chrono::Utc;
use rusqlite::{params, OptionalExtension};

impl Storage {
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
}
