//! `disk_scan_cache` + `system_samples` — derived caches the tools write so
//! repeat calls don't re-scan / re-sample.

use super::{AppResult, Storage};
use chrono::Utc;
use rusqlite::params;

impl Storage {
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
}
