//! `token_usage` table — per-request token accounting for the stats view.

use super::{query_usage_buckets, AppResult, Storage, TokenUsageRecord, TokenUsageStats};
use chrono::Utc;
use rusqlite::params;

impl Storage {
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
}
