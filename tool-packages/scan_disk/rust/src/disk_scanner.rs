use crate::app_error::{AppError, AppResult};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use walkdir::WalkDir;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskScanRequest {
    pub path: String,
    pub max_depth: Option<usize>,
    pub max_children: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub children: Vec<DiskNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskScanResult {
    pub root: DiskNode,
    pub scanned_entries: u64,
    pub elapsed_ms: u128,
    pub truncated: bool,
    pub scanned_at: String,
}

#[derive(Debug, Clone)]
struct ScanOptions {
    max_depth: usize,
    max_children: usize,
}

impl DiskScanRequest {
    fn options(&self) -> ScanOptions {
        ScanOptions {
            max_depth: self.max_depth.unwrap_or(4).clamp(1, 12),
            max_children: self.max_children.unwrap_or(12).clamp(1, 64),
        }
    }
}

pub fn scan_path(request: DiskScanRequest) -> AppResult<DiskScanResult> {
    let root = PathBuf::from(request.path.trim());
    if !root.exists() {
        return Err(AppError::InvalidInput(format!(
            "路径不存在: {}",
            root.display()
        )));
    }

    let options = request.options();
    let started = Instant::now();
    let scanned_entries = AtomicU64::new(0);
    let (root, truncated) = scan_node(&root, 0, &options, &scanned_entries)?;

    Ok(DiskScanResult {
        root,
        scanned_entries: scanned_entries.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis(),
        truncated,
        scanned_at: chrono::Utc::now().to_rfc3339(),
    })
}

fn scan_node(
    path: &Path,
    depth: usize,
    options: &ScanOptions,
    scanned_entries: &AtomicU64,
) -> AppResult<(DiskNode, bool)> {
    scanned_entries.fetch_add(1, Ordering::Relaxed);
    let metadata = fs::symlink_metadata(path)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string());

    if metadata.is_file() {
        return Ok((
            DiskNode {
                name,
                path: path.display().to_string(),
                is_dir: false,
                size_bytes: metadata.len(),
                file_count: 1,
                dir_count: 0,
                children: Vec::new(),
            },
            false,
        ));
    }

    if !metadata.is_dir() {
        return Ok((
            DiskNode {
                name,
                path: path.display().to_string(),
                is_dir: false,
                size_bytes: 0,
                file_count: 0,
                dir_count: 0,
                children: Vec::new(),
            },
            false,
        ));
    }

    if depth >= options.max_depth {
        let summary = summarize_dir(path, scanned_entries);
        return Ok((
            DiskNode {
                name,
                path: path.display().to_string(),
                is_dir: true,
                size_bytes: summary.size_bytes,
                file_count: summary.file_count,
                dir_count: summary.dir_count,
                children: Vec::new(),
            },
            true,
        ));
    }

    let entries = fs::read_dir(path)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    let child_results = entries
        .par_iter()
        .filter_map(|entry| scan_node(entry, depth + 1, options, scanned_entries).ok())
        .collect::<Vec<_>>();

    let mut children = child_results
        .iter()
        .map(|(node, _)| node.clone())
        .collect::<Vec<_>>();
    let mut truncated = child_results.iter().any(|(_, was_truncated)| *was_truncated);

    let size_bytes = children.iter().map(|child| child.size_bytes).sum();
    let file_count = children.iter().map(|child| child.file_count).sum();
    let dir_count = 1 + children.iter().map(|child| child.dir_count).sum::<u64>();

    children.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    if children.len() > options.max_children {
        children.truncate(options.max_children);
        truncated = true;
    }

    Ok((
        DiskNode {
            name,
            path: path.display().to_string(),
            is_dir: true,
            size_bytes,
            file_count,
            dir_count,
            children,
        },
        truncated,
    ))
}

#[derive(Default)]
struct DirSummary {
    size_bytes: u64,
    file_count: u64,
    dir_count: u64,
}

fn summarize_dir(path: &Path, scanned_entries: &AtomicU64) -> DirSummary {
    let mut summary = DirSummary::default();
    for entry in WalkDir::new(path).follow_links(false).into_iter().filter_map(Result::ok) {
        scanned_entries.fetch_add(1, Ordering::Relaxed);
        match entry.metadata() {
            Ok(metadata) if metadata.is_file() => {
                summary.file_count += 1;
                summary.size_bytes = summary.size_bytes.saturating_add(metadata.len());
            }
            Ok(metadata) if metadata.is_dir() => {
                summary.dir_count += 1;
            }
            _ => {}
        }
    }
    summary
}
