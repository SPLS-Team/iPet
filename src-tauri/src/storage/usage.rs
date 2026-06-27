//! `app_usage` table — per-day, per-app foreground seconds, accumulated by the
//! background sampler in `usage_tracker`. One row per (day, app_key); the
//! sampler upserts a tick's worth of seconds each interval. Queries aggregate
//! over a day range for the Usage view and the `app_usage_stats` tool.
//!
//! `day` is a local-date string (`YYYY-MM-DD`) so "today" matches the user's
//! wall clock; lexicographic comparison on `YYYY-MM-DD` doubles as date
//! ordering, which the range queries rely on.

use super::{AppResult, Storage};
use chrono::{Duration, Local};
use rusqlite::params;
use serde::{Deserialize, Serialize};

/// One app's accumulated foreground time over the queried range.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsageEntry {
    pub app_key: String,
    pub app_name: String,
    pub seconds: i64,
    pub last_seen: Option<String>,
}

/// Per-day total across all apps (for the trend chart).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsageDayBucket {
    pub day: String,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsageStats {
    pub range: String,
    pub total_seconds: i64,
    pub by_app: Vec<AppUsageEntry>,
    pub by_day: Vec<AppUsageDayBucket>,
}

impl Storage {
    /// Accumulate `seconds` onto the (day, app_key) row, creating it if absent.
    /// `last_seen` is the sampler's tick timestamp (RFC3339) so the UI can show
    /// "last active" without an extra query.
    pub fn record_app_usage(
        &self,
        day: &str,
        app_key: &str,
        app_name: &str,
        seconds: i64,
        last_seen: &str,
    ) -> AppResult<()> {
        let conn = self.lock()?;
        conn.execute(
            "INSERT INTO app_usage (day, app_key, app_name, seconds, last_seen)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(day, app_key) DO UPDATE SET
                seconds = seconds + excluded.seconds,
                app_name = excluded.app_name,
                last_seen = excluded.last_seen",
            params![day, app_key, app_name, seconds, last_seen],
        )?;
        Ok(())
    }

    /// Aggregate foreground seconds over `range` (`today` / `7d` / `30d`).
    /// `by_app` is capped at `limit` (top apps by total seconds); `by_day`
    /// returns every day in the range for the trend chart.
    pub fn app_usage_stats(&self, range: &str, limit: usize) -> AppResult<AppUsageStats> {
        let limit = limit.clamp(1, 200);
        let today = Local::now().date_naive();
        let (start_day, range_label) = match range {
            "today" => (today, "today"),
            "30d" => (today - Duration::days(29), "30d"),
            _ => (today - Duration::days(6), "7d"),
        };
        let start = start_day.format("%Y-%m-%d").to_string();
        let end = today.format("%Y-%m-%d").to_string();

        let conn = self.lock()?;

        let total_seconds: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(seconds), 0) FROM app_usage WHERE day BETWEEN ?1 AND ?2",
                params![start, end],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let mut stmt = conn.prepare(
            "SELECT app_key, app_name, COALESCE(SUM(seconds), 0), MAX(last_seen)
             FROM app_usage
             WHERE day BETWEEN ?1 AND ?2
             GROUP BY app_key
             ORDER BY COALESCE(SUM(seconds), 0) DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![start, end, limit as i64], |row| {
            Ok(AppUsageEntry {
                app_key: row.get(0)?,
                app_name: row.get(1)?,
                seconds: row.get(2)?,
                last_seen: row.get(3)?,
            })
        })?;
        let mut by_app = Vec::new();
        for row in rows {
            by_app.push(row?);
        }

        let mut stmt = conn.prepare(
            "SELECT day, COALESCE(SUM(seconds), 0)
             FROM app_usage
             WHERE day BETWEEN ?1 AND ?2
             GROUP BY day
             ORDER BY day ASC",
        )?;
        let rows = stmt.query_map(params![start, end], |row| {
            Ok(AppUsageDayBucket {
                day: row.get(0)?,
                seconds: row.get(1)?,
            })
        })?;
        let mut by_day = Vec::new();
        for row in rows {
            by_day.push(row?);
        }

        Ok(AppUsageStats {
            range: range_label.to_string(),
            total_seconds,
            by_app,
            by_day,
        })
    }

    /// Drop app-usage rows older than `keep_days`, called from the startup
    /// retention sweep. `cutoff` is a `YYYY-MM-DD` string; lexicographic `<`
    /// is correct for this format.
    pub fn prune_app_usage(&self, keep_days: u32) -> AppResult<usize> {
        let conn = self.lock()?;
        let cutoff = (Local::now().date_naive() - Duration::days(keep_days as i64))
            .format("%Y-%m-%d")
            .to_string();
        let removed = conn.execute("DELETE FROM app_usage WHERE day < ?1", params![cutoff])?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn fresh_storage() -> (TempDir, Storage) {
        let dir = TempDir::new("usage");
        let storage = Storage::open(dir.path().join("ipet-usage-test.sqlite3")).unwrap();
        (dir, storage)
    }

    #[test]
    fn stats_aggregate_over_a_7d_window_relative_to_today() {
        let (_dir, storage) = fresh_storage();
        let today = Local::now().date_naive();
        let today_s = today.format("%Y-%m-%d").to_string();
        let two_days_ago = (today - Duration::days(2))
            .format("%Y-%m-%d")
            .to_string();
        let ten_days_ago = (today - Duration::days(10))
            .format("%Y-%m-%d")
            .to_string();

        // Today: Code 30s, Chrome 10s.
        storage
            .record_app_usage(&today_s, "code", "Code", 15, "now1")
            .unwrap();
        storage
            .record_app_usage(&today_s, "code", "Code", 15, "now2")
            .unwrap();
        storage
            .record_app_usage(&today_s, "chrome", "Chrome", 10, "now3")
            .unwrap();
        // Two days ago: Code 20s — inside the 7d window.
        storage
            .record_app_usage(&two_days_ago, "code", "Code", 20, "old1")
            .unwrap();
        // Ten days ago: Code 100s — outside the 7d window.
        storage
            .record_app_usage(&ten_days_ago, "code", "Code", 100, "ancient")
            .unwrap();

        let stats = storage.app_usage_stats("7d", 50).unwrap();
        assert_eq!(stats.range, "7d");
        // 30 (today Code) + 10 (Chrome) + 20 (Code two days ago) = 60.
        assert_eq!(stats.total_seconds, 60);
        // Code tops the list: 30 + 20 = 50.
        assert_eq!(stats.by_app[0].app_key, "code");
        assert_eq!(stats.by_app[0].seconds, 50);
        assert_eq!(stats.by_app[1].app_key, "chrome");
        assert_eq!(stats.by_app[1].seconds, 10);
        // Two distinct days in the window.
        assert_eq!(stats.by_day.len(), 2);
        assert!(stats.by_day.iter().any(|d| d.day == today_s && d.seconds == 40));
        assert!(stats
            .by_day
            .iter()
            .any(|d| d.day == two_days_ago && d.seconds == 20));
    }

    #[test]
    fn today_range_only_counts_today() {
        let (_dir, storage) = fresh_storage();
        let today = Local::now().date_naive();
        let yesterday = (today - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let today_s = today.format("%Y-%m-%d").to_string();

        storage
            .record_app_usage(&today_s, "code", "Code", 25, "now")
            .unwrap();
        storage
            .record_app_usage(&yesterday, "code", "Code", 999, "yesterday")
            .unwrap();

        let stats = storage.app_usage_stats("today", 50).unwrap();
        assert_eq!(stats.range, "today");
        assert_eq!(stats.total_seconds, 25, "yesterday excluded from today");
        assert_eq!(stats.by_app.len(), 1);
        assert_eq!(stats.by_app[0].seconds, 25);
    }

    #[test]
    fn prune_drops_rows_older_than_keep_days() {
        let (_dir, storage) = fresh_storage();
        let today = Local::now().date_naive();
        let ancient = (today - Duration::days(400))
            .format("%Y-%m-%d")
            .to_string();
        let today_s = today.format("%Y-%m-%d").to_string();

        storage
            .record_app_usage(&ancient, "code", "Code", 50, "ancient")
            .unwrap();
        storage
            .record_app_usage(&today_s, "code", "Code", 5, "now")
            .unwrap();

        let removed = storage.prune_app_usage(180).unwrap();
        assert_eq!(removed, 1, "the 400-day-old row should be pruned");
        let stats = storage.app_usage_stats("30d", 50).unwrap();
        assert_eq!(stats.total_seconds, 5, "today's row survives");
    }

    #[test]
    fn app_name_refreshes_on_upsert() {
        let (_dir, storage) = fresh_storage();
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        storage
            .record_app_usage(&today, "code", "Code Old", 10, "t1")
            .unwrap();
        storage
            .record_app_usage(&today, "code", "Code New", 10, "t2")
            .unwrap();
        let stats = storage.app_usage_stats("today", 50).unwrap();
        assert_eq!(stats.by_app[0].app_name, "Code New");
        assert_eq!(stats.by_app[0].last_seen.as_deref(), Some("t2"));
        assert_eq!(stats.by_app[0].seconds, 20);
    }
}
