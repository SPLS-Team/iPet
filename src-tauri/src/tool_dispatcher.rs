use crate::app_error::{AppError, AppResult};
use crate::disk_scanner::{self, DiskScanRequest};
use crate::storage::{Storage, ToolConfig};
use crate::system_monitor::SystemMonitor;
use reqwest::Method;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ToolDispatcher {
    system: Arc<Mutex<SystemMonitor>>,
    storage: Arc<Storage>,
    http: reqwest::Client,
}

impl ToolDispatcher {
    pub fn new(system: Arc<Mutex<SystemMonitor>>, storage: Arc<Storage>) -> Self {
        Self {
            system,
            storage,
            http: reqwest::Client::new(),
        }
    }

    pub fn active_definitions(&self) -> AppResult<Vec<Value>> {
        Ok(self
            .storage
            .active_tools()?
            .into_iter()
            .map(tool_definition)
            .collect())
    }

    pub async fn dispatch(&self, name: &str, arguments: &str) -> AppResult<String> {
        let tool = self
            .storage
            .get_tool(name)?
            .ok_or_else(|| AppError::InvalidInput(format!("工具不存在: {name}")))?;

        if !tool.enabled {
            return Err(AppError::InvalidInput(format!("工具未启用: {name}")));
        }

        match tool.kind.as_str() {
            "builtin" => self.dispatch_builtin(name, arguments).await,
            "http" => self.dispatch_http(&tool, arguments).await,
            other => Err(AppError::InvalidInput(format!("不支持的工具类型: {other}"))),
        }
    }

    async fn dispatch_builtin(&self, name: &str, arguments: &str) -> AppResult<String> {
        match name {
            "get_system_status" => {
                let args = serde_json::from_str::<SystemStatusArgs>(arguments).unwrap_or_default();
                let mut monitor = self.system.lock().await;
                let snapshot = monitor.snapshot(args.process_limit.unwrap_or(10).clamp(3, 30));
                let json = serde_json::to_string(&snapshot)?;
                self.storage.cache_system_sample(&json)?;
                Ok(json)
            }
            "scan_disk" => {
                let args = serde_json::from_str::<DiskScanArgs>(arguments)
                    .map_err(|error| AppError::InvalidInput(error.to_string()))?;
                let request = DiskScanRequest {
                    path: args.path,
                    max_depth: args.max_depth,
                    max_children: args.max_children,
                };
                let result = tokio::task::spawn_blocking(move || disk_scanner::scan_path(request))
                    .await
                    .map_err(|error| AppError::Model(error.to_string()))??;
                let json = serde_json::to_string(&result)?;
                self.storage.cache_disk_scan(&result.root.path, &json)?;
                Ok(json)
            }
            other => Err(AppError::InvalidInput(format!("未知内置工具: {other}"))),
        }
    }

    async fn dispatch_http(&self, tool: &ToolConfig, arguments: &str) -> AppResult<String> {
        let http = tool
            .http
            .as_ref()
            .ok_or_else(|| AppError::InvalidInput("HTTP 工具缺少 http 配置".to_string()))?;
        let method = http
            .method
            .to_ascii_uppercase()
            .parse::<Method>()
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;
        let args = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}));

        let mut request = self.http.request(method.clone(), &http.url);
        for header in &http.headers {
            if !header.key.trim().is_empty() {
                request = request.header(header.key.trim(), header.value.trim());
            }
        }

        if method == Method::GET {
            if let Some(map) = args.as_object() {
                let query = map
                    .iter()
                    .map(|(key, value)| (key.as_str(), scalar_to_query(value)))
                    .collect::<Vec<_>>();
                request = request.query(&query);
            }
        } else {
            request = request.json(&args);
        }

        let response = request.send().await?.error_for_status()?;
        let status = response.status().as_u16();
        let text = response.text().await?;
        Ok(json!({
            "status": status,
            "body": text
        })
        .to_string())
    }
}

fn tool_definition(tool: ToolConfig) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters
        }
    })
}

fn scalar_to_query(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}

#[derive(Debug, Default, Deserialize)]
struct SystemStatusArgs {
    process_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct DiskScanArgs {
    path: String,
    max_depth: Option<usize>,
    max_children: Option<usize>,
}
