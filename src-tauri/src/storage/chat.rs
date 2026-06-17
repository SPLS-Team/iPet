//! `chat_messages` table — chat history persistence.

use super::{AppResult, ChatRecord, Storage};
use chrono::Utc;
use rusqlite::params;

impl Storage {
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
}
