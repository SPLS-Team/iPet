//! Read a `tool.json` package and turn it into a `ToolConfigInput` suitable
//! for `Storage::save_custom_tool`. See `docs/TOOL_PACKAGE.md` for the
//! schema.

use crate::app_error::{AppError, AppResult};
use crate::storage::{HttpToolConfig, ToolConfigInput, ToolHeader};
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const TOOL_FILE_NAME: &str = "tool.json";
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolPackage {
    schema_version: u32,
    name: String,
    display_name: String,
    description: String,
    #[serde(default)]
    version: Option<String>,
    kind: String,
    parameters: Value,
    http: Option<PackageHttp>,
    #[serde(default)]
    permissions: Vec<String>,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageHttp {
    method: String,
    url: String,
    #[serde(default)]
    headers: Vec<ToolHeader>,
}

/// Parse a tool package from disk and return a `ToolConfigInput` plus the
/// package metadata that doesn't fit on the storage struct (version,
/// permissions list) so the caller can log it.
#[derive(Debug)]
pub struct ImportedPackage {
    pub input: ToolConfigInput,
    pub version: Option<String>,
    pub permissions: Vec<String>,
}

pub fn load_package(path: &Path) -> AppResult<ImportedPackage> {
    let tool_json_path = resolve_tool_json(path)?;
    let raw = fs::read_to_string(&tool_json_path)?;
    let pkg: ToolPackage = serde_json::from_str(&raw).map_err(|err| {
        AppError::InvalidInput(format!("tool.json 解析失败 ({}): {err}", tool_json_path.display()))
    })?;

    if pkg.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(AppError::InvalidInput(format!(
            "不支持的 schemaVersion: {}（当前仅支持 {}）",
            pkg.schema_version, SUPPORTED_SCHEMA_VERSION
        )));
    }
    if pkg.kind != "http" {
        return Err(AppError::InvalidInput(format!(
            "当前仅支持 kind=\"http\"，包内为 kind=\"{}\"",
            pkg.kind
        )));
    }
    let http = pkg.http.ok_or_else(|| {
        AppError::InvalidInput("kind=http 的工具包必须包含 http 字段".to_string())
    })?;

    let input = ToolConfigInput {
        name: pkg.name,
        display_name: pkg.display_name,
        description: pkg.description,
        kind: pkg.kind,
        enabled: pkg.enabled,
        parameters: pkg.parameters,
        http: Some(HttpToolConfig {
            method: http.method,
            url: http.url,
            headers: http.headers,
        }),
    };

    Ok(ImportedPackage {
        input,
        version: pkg.version,
        permissions: pkg.permissions,
    })
}

/// Accept either a directory containing `tool.json`, or a path pointing at
/// `tool.json` itself.
fn resolve_tool_json(path: &Path) -> AppResult<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    if path.is_dir() {
        let candidate = path.join(TOOL_FILE_NAME);
        if candidate.is_file() {
            return Ok(candidate);
        }
        return Err(AppError::InvalidInput(format!(
            "目录 {} 中缺少 {TOOL_FILE_NAME}",
            path.display()
        )));
    }
    Err(AppError::InvalidInput(format!(
        "路径不存在或不可读: {}",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn write_pkg(dir: &Path, body: &str) {
        std::fs::write(dir.join(TOOL_FILE_NAME), body).unwrap();
    }

    #[test]
    fn loads_minimal_http_package() {
        let dir = TempDir::new("pkg-min");
        write_pkg(
            dir.path(),
            r#"{
                "schemaVersion": 1,
                "name": "weather_lookup",
                "displayName": "天气查询",
                "description": "demo",
                "kind": "http",
                "parameters": {"type": "object", "properties": {}},
                "http": {"method": "GET", "url": "https://example.com/api"}
            }"#,
        );
        let pkg = load_package(dir.path()).unwrap();
        assert_eq!(pkg.input.name, "weather_lookup");
        assert!(pkg.input.enabled, "enabled defaults to true");
        assert_eq!(pkg.input.http.as_ref().unwrap().headers.len(), 0);
        assert!(pkg.permissions.is_empty());
    }

    #[test]
    fn accepts_pointing_at_tool_json_directly() {
        let dir = TempDir::new("pkg-file");
        let path = dir.path().join(TOOL_FILE_NAME);
        std::fs::write(
            &path,
            r#"{
                "schemaVersion": 1,
                "name": "t",
                "displayName": "T",
                "description": "d",
                "kind": "http",
                "parameters": {"type": "object", "properties": {}},
                "http": {"method": "GET", "url": "https://example.com/"}
            }"#,
        )
        .unwrap();
        let pkg = load_package(&path).unwrap();
        assert_eq!(pkg.input.name, "t");
    }

    #[test]
    fn rejects_wrong_schema_version() {
        let dir = TempDir::new("pkg-bad-ver");
        write_pkg(
            dir.path(),
            r#"{
                "schemaVersion": 99,
                "name": "x", "displayName": "X", "description": "d",
                "kind": "http",
                "parameters": {"type": "object"},
                "http": {"method": "GET", "url": "https://example.com/"}
            }"#,
        );
        let err = load_package(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn rejects_non_http_kind() {
        let dir = TempDir::new("pkg-rust");
        write_pkg(
            dir.path(),
            r#"{
                "schemaVersion": 1,
                "name": "n", "displayName": "N", "description": "d",
                "kind": "rust",
                "parameters": {"type": "object"}
            }"#,
        );
        let err = load_package(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn rejects_missing_tool_json() {
        let dir = TempDir::new("pkg-empty");
        let err = load_package(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }
}
