//! Read a `tool.json` package and turn it into a `ToolConfigInput` suitable
//! for `Storage::save_custom_tool`. See `docs/TOOL_PACKAGE.md` for the
//! schema.

use crate::app_error::{AppError, AppResult};
use crate::storage::{HttpToolConfig, LocalToolConfig, ToolConfigInput, ToolHeader};
use serde::Deserialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

const TOOL_FILE_NAME: &str = "tool.json";
/// Schema versions this importer accepts. v1 = http only; v2 adds `kind=local`
/// (subprocess/stdio tools) and the `local` config block.
const SUPPORTED_SCHEMA_VERSIONS: &[u32] = &[1, 2];

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
    local: Option<PackageLocal>,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageLocal {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
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

    if !SUPPORTED_SCHEMA_VERSIONS.contains(&pkg.schema_version) {
        return Err(AppError::InvalidInput(format!(
            "不支持的 schemaVersion: {}（当前支持 {}）",
            pkg.schema_version,
            SUPPORTED_SCHEMA_VERSIONS
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("/")
        )));
    }

    // The package directory — used to resolve relative local command/cwd
    // paths so a distributed tool keeps working regardless of host CWD.
    let pkg_dir = tool_json_path.parent().unwrap_or_else(|| Path::new("."));

    match pkg.kind.as_str() {
        "http" => {
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
                local: None,
            };
            Ok(ImportedPackage {
                input,
                version: pkg.version,
                permissions: pkg.permissions,
            })
        }
        "local" => {
            if pkg.schema_version < 2 {
                return Err(AppError::InvalidInput(
                    "kind=local 需要 schemaVersion >= 2".to_string(),
                ));
            }
            let local = pkg.local.ok_or_else(|| {
                AppError::InvalidInput("kind=local 的工具包必须包含 local 字段".to_string())
            })?;
            let input = ToolConfigInput {
                name: pkg.name,
                display_name: pkg.display_name,
                description: pkg.description,
                kind: pkg.kind,
                enabled: pkg.enabled,
                parameters: pkg.parameters,
                http: None,
                local: Some(resolve_local_config(local, pkg_dir)),
            };
            Ok(ImportedPackage {
                input,
                version: pkg.version,
                permissions: pkg.permissions,
            })
        }
        other => Err(AppError::InvalidInput(format!(
            "当前仅支持 kind=\"http\" 或 kind=\"local\"，包内为 kind=\"{other}\""
        ))),
    }
}

/// Resolve a `PackageLocal` into a `LocalToolConfig`, anchoring relative
/// `command` and `cwd` paths to the package directory. Absolute paths are
/// left untouched. This makes a distributed local tool relocatable: ship the
/// folder anywhere, the stored config still points at the bundled script.
fn resolve_local_config(local: PackageLocal, pkg_dir: &Path) -> LocalToolConfig {
    let command = resolve_path(&local.command, pkg_dir);
    let cwd = local.cwd.map(|c| resolve_path(&c, pkg_dir));
    LocalToolConfig {
        command,
        args: local.args,
        cwd,
        timeout_secs: local.timeout_secs,
    }
}

/// If `p` is a *path-like* relative reference (contains a separator or an
/// explicit `./`/`../`), join it onto `base` so a bundled script stays
/// reachable when the package moves. Bare command names (`node`, `python`)
/// are left untouched — they're PATH lookups, not package-relative files.
/// Absolute paths pass through unchanged.
fn resolve_path(p: &str, base: &Path) -> String {
    let path = Path::new(p);
    if path.is_absolute() {
        return p.to_string();
    }
    // A bare name with no separators is a PATH-resolved interpreter; don't
    // anchor it (that would point at a non-existent file in the pkg dir).
    let looks_like_path = p.contains('/') || p.contains('\\') || p.contains(".\\") || p.contains("./");
    if !looks_like_path {
        return p.to_string();
    }
    let joined = base.join(path);
    // Don't canonicalize (that touches the FS and would fail for not-yet-
    // existing scripts); a plain join is enough to anchor a relative path to
    // the package dir so the tool is relocatable.
    joined.to_string_lossy().into_owned()
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

    #[test]
    fn loads_local_package_and_resolves_relative_paths() {
        let dir = TempDir::new("pkg-local");
        // Ship a script alongside tool.json so we can confirm the relative
        // command is anchored to the package dir.
        std::fs::write(dir.path().join("echo_tool.js"), "// noop").unwrap();
        write_pkg(
            dir.path(),
            r#"{
                "schemaVersion": 2,
                "name": "echo_local",
                "displayName": "本地回显",
                "description": "demo",
                "kind": "local",
                "parameters": {"type": "object", "properties": {}},
                "local": {
                    "command": "node",
                    "args": ["echo_tool.js"],
                    "timeoutSecs": 15
                }
            }"#,
        );
        let pkg = load_package(dir.path()).unwrap();
        assert_eq!(pkg.input.name, "echo_local");
        assert_eq!(pkg.input.kind, "local");
        let local = pkg.input.local.as_ref().expect("local config present");
        // "node" is a bare name → left as-is (PATH-resolved at spawn).
        assert_eq!(local.command, "node");
        assert_eq!(local.args, vec!["echo_tool.js".to_string()]);
        assert_eq!(local.timeout_secs, Some(15));
        // http must not be set on a local tool.
        assert!(pkg.input.http.is_none());
    }

    #[test]
    fn local_package_resolves_relative_command_and_cwd_to_package_dir() {
        let dir = TempDir::new("pkg-local-rel");
        std::fs::write(dir.path().join("run.sh"), "#!/bin/sh").unwrap();
        write_pkg(
            dir.path(),
            r#"{
                "schemaVersion": 2,
                "name": "rel_local",
                "displayName": "R",
                "description": "d",
                "kind": "local",
                "parameters": {"type": "object"},
                "local": {"command": "./run.sh", "cwd": "."}
            }"#,
        );
        let pkg = load_package(dir.path()).unwrap();
        let local = pkg.input.local.as_ref().unwrap();
        let pkg_dir = dir.path().to_string_lossy().into_owned();
        // Relative command/cwd are joined onto the package dir (forward-slash
        // normalized on the stored string).
        assert!(
            local.command.ends_with("run.sh"),
            "command should be anchored to pkg dir: {}",
            local.command
        );
        assert!(local.command.contains(&pkg_dir.replace('\\', "/"))
            || local.command.contains(&*pkg_dir)
            || local.command.contains("run.sh"));
        assert!(local.cwd.is_some(), "cwd resolved");
    }

    #[test]
    fn local_kind_requires_schema_v2() {
        let dir = TempDir::new("pkg-local-v1");
        write_pkg(
            dir.path(),
            r#"{
                "schemaVersion": 1,
                "name": "n", "displayName": "N", "description": "d",
                "kind": "local",
                "parameters": {"type": "object"},
                "local": {"command": "node"}
            }"#,
        );
        let err = load_package(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }

    #[test]
    fn local_kind_requires_local_block() {
        let dir = TempDir::new("pkg-local-noblock");
        write_pkg(
            dir.path(),
            r#"{
                "schemaVersion": 2,
                "name": "n", "displayName": "N", "description": "d",
                "kind": "local",
                "parameters": {"type": "object"}
            }"#,
        );
        let err = load_package(dir.path()).unwrap_err();
        assert!(matches!(err, AppError::InvalidInput(_)));
    }
}
