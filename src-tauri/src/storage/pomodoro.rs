//! `pomodoro_log` table — one row per completed focus/break session, written
//! from the frontend on phase transitions. Aggregated into the Usage view's
//! pomodoro section (today/7d/30d completed-work counts + per-day trend).
//!
//! `day` is a local-date string (`YYYY-MM-DD`), mirroring `app_usage` so both
//! features share the lexicographic-date ordering trick for range queries.

use super::{AppResult, Storage};
use chrono::{Duration, Local};
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// One day's pomodoro totals, for the trend chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroDayBucket {
    pub day: String,
    pub work_count: i64,
    pub break_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroStats {
    pub range: String,
    pub total_work: i64,
    pub total_break: i64,
    pub by_day: Vec<PomodoroDayBucket>,
}

impl Storage {
    /// Record one completed session. `kind` is `"work"` or `"break"`; anything
    /// else is rejected so a malformed call can't pollute the stats.
    pub fn record_pomodoro_session(&self, kind: &str) -> AppResult<()> {
        if !matches!(kind, "work" | "break") {
            return Err(crate::app_error::AppError::InvalidInput(format!(
                "番茄钟类型必须是 work 或 break，收到: {kind}"
            )));
        }
        let day = Local::now().format("%Y-%m-%d").to_string();
        let completed_at = chrono::Utc::now().to_rfc3339();
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO pomodoro_log (day, kind, completed_at) VALUES (?1, ?2, ?3)",
            params![day, kind, completed_at],
        )?;
        Ok(())
    }

    /// Aggregate completed sessions over `range` (`today` / `7d` / `30d`).
    pub fn pomodoro_stats(&self, range: &str) -> AppResult<PomodoroStats> {
        let today = Local::now().date_naive();
        let (start_day, range_label) = match range {
            "today" => (today, "today"),
            "30d" => (today - Duration::days(29), "30d"),
            _ => (today - Duration::days(6), "7d"),
        };
        let start = start_day.format("%Y-%m-%d").to_string();
        let end = today.format("%Y-%m-%d").to_string();

        let conn = self.lock()?;

        let (total_work, total_break): (i64, i64) = conn
            .query_row(
                "SELECT
                    COALESCE(SUM(CASE WHEN kind = 'work' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE WHEN kind = 'break' THEN 1 ELSE 0 END), 0)
                 FROM pomodoro_log WHERE day BETWEEN ?1 AND ?2",
                params![start, end],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or((0, 0));

        let mut stmt = conn.prepare(
            "SELECT day,
                    SUM(CASE WHEN kind = 'work' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN kind = 'break' THEN 1 ELSE 0 END)
             FROM pomodoro_log
             WHERE day BETWEEN ?1 AND ?2
             GROUP BY day
             ORDER BY day ASC",
        )?;
        let rows = stmt.query_map(params![start, end], |row| {
            Ok(PomodoroDayBucket {
                day: row.get(0)?,
                work_count: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                break_count: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            })
        })?;
        let mut by_day = Vec::new();
        for row in rows {
            by_day.push(row?);
        }

        Ok(PomodoroStats {
            range: range_label.to_string(),
            total_work,
            total_break,
            by_day,
        })
    }

    /// Drop rows older than `keep_days` (local-date-string cutoff, like
    /// `prune_app_usage`). Called from the startup retention sweep.
    pub fn prune_pomodoro(&self, keep_days: u32) -> AppResult<usize> {
        let conn = self.lock()?;
        let cutoff = (Local::now().date_naive() - Duration::days(keep_days as i64))
            .format("%Y-%m-%d")
            .to_string();
        let removed = conn.execute("DELETE FROM pomodoro_log WHERE day < ?1", params![cutoff])?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn fresh_storage() -> (TempDir, Storage) {
        let dir = TempDir::new("pomodoro");
        let storage = Storage::open(dir.path().join("ipet-pomodoro-test.sqlite3")).unwrap();
        (dir, storage)
    }

    fn seed(storage: &Storage, day: &str, kind: &str) {
        let conn = storage.lock().unwrap();
        conn.execute(
            "INSERT INTO pomodoro_log (day, kind, completed_at) VALUES (?1, ?2, ?3)",
            params![day, kind, "2026-01-01T00:00:00Z"],
        )
        .unwrap();
    }

    #[test]
    fn record_rejects_unknown_kind() {
        let (_dir, storage) = fresh_storage();
        let err = storage.record_pomodoro_session("focus").unwrap_err();
        assert!(matches!(err, crate::app_error::AppError::InvalidInput(_)));
    }

    #[test]
    fn stats_aggregate_over_7d_window() {
        let (_dir, storage) = fresh_storage();
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let yesterday = (Local::now().date_naive() - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let old = (Local::now().date_naive() - Duration::days(10))
            .format("%Y-%m-%d")
            .to_string();

        seed(&storage, &today, "work");
        seed(&storage, &today, "work");
        seed(&storage, &today, "break");
        seed(&storage, &yesterday, "work");
        seed(&storage, &old, "work"); // outside 7d

        let stats = storage.pomodoro_stats("7d").unwrap();
        assert_eq!(stats.range, "7d");
        assert_eq!(stats.total_work, 3, "2 today + 1 yesterday");
        assert_eq!(stats.total_break, 1);
        assert_eq!(stats.by_day.len(), 2);
        assert!(stats.by_day.iter().any(|d| d.day == today && d.work_count == 2));
    }

    #[test]
    fn today_range_only_counts_today() {
        let (_dir, storage) = fresh_storage();
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let yesterday = (Local::now().date_naive() - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        seed(&storage, &today, "work");
        seed(&storage, &yesterday, "work");

        let stats = storage.pomodoro_stats("today").unwrap();
        assert_eq!(stats.total_work, 1);
        assert_eq!(stats.by_day.len(), 1);
    }

    #[test]
    fn prune_drops_old_rows() {
        let (_dir, storage) = fresh_storage();
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let ancient = (Local::now().date_naive() - Duration::days(400))
            .format("%Y-%m-%d")
            .to_string();
        seed(&storage, &ancient, "work");
        seed(&storage, &today, "work");

        let removed = storage.prune_pomodoro(180).unwrap();
        assert_eq!(removed, 1);
        let stats = storage.pomodoro_stats("30d").unwrap();
        assert_eq!(stats.total_work, 1);
    }
}
