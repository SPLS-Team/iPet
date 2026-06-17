use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmSettings {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_context_messages: usize,
    pub system_prompt: String,
    #[serde(default)]
    pub auto_system_check_enabled: bool,
    #[serde(default = "default_auto_system_check_interval_minutes")]
    pub auto_system_check_interval_minutes: u64,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4.1-mini".to_string(),
            temperature: 0.7,
            max_context_messages: 18,
            system_prompt: "你是 iPet，一个常驻桌面的轻量助手。回答要简洁，必要时主动使用本地工具查看系统状态或分析目录占用。".to_string(),
            auto_system_check_enabled: false,
            auto_system_check_interval_minutes: default_auto_system_check_interval_minutes(),
        }
    }
}

impl LlmSettings {
    pub fn normalized_base_url(&self) -> String {
        self.base_url.trim().trim_end_matches('/').to_string()
    }

    pub fn validate_public_fields(&self) -> Result<(), String> {
        if self.normalized_base_url().is_empty() {
            return Err("Base URL 不能为空".to_string());
        }
        if self.model.trim().is_empty() {
            return Err("模型名不能为空".to_string());
        }
        if !(0.0..=2.0).contains(&self.temperature) {
            return Err("temperature 必须在 0 到 2 之间".to_string());
        }
        if !(4..=64).contains(&self.max_context_messages) {
            return Err("上下文消息数必须在 4 到 64 之间".to_string());
        }
        if !(1..=120).contains(&self.auto_system_check_interval_minutes) {
            return Err("自动检查间隔必须在 1 到 120 分钟之间".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettingsStatus {
    pub has_api_key: bool,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_context_messages: usize,
    pub system_prompt: String,
    pub auto_system_check_enabled: bool,
    pub auto_system_check_interval_minutes: u64,
    pub settings_path: String,
}

impl LlmSettingsStatus {
    pub fn from_settings(settings: &LlmSettings, settings_path: PathBuf) -> Self {
        Self {
            has_api_key: settings
                .api_key
                .as_ref()
                .map(|key| !key.trim().is_empty())
                .unwrap_or(false),
            base_url: settings.base_url.clone(),
            model: settings.model.clone(),
            temperature: settings.temperature,
            max_context_messages: settings.max_context_messages,
            system_prompt: settings.system_prompt.clone(),
            auto_system_check_enabled: settings.auto_system_check_enabled,
            auto_system_check_interval_minutes: settings.auto_system_check_interval_minutes,
            settings_path: settings_path.display().to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettingsInput {
    pub api_key: Option<String>,
    pub clear_api_key: bool,
    pub base_url: String,
    pub model: String,
    pub temperature: f32,
    pub max_context_messages: usize,
    pub system_prompt: String,
    pub auto_system_check_enabled: bool,
    pub auto_system_check_interval_minutes: u64,
}

impl LlmSettingsInput {
    pub fn merge_into(self, current: &mut LlmSettings) {
        current.base_url = self.base_url.trim().trim_end_matches('/').to_string();
        current.model = self.model.trim().to_string();
        current.temperature = self.temperature;
        current.max_context_messages = self.max_context_messages;
        current.system_prompt = self.system_prompt.trim().to_string();
        current.auto_system_check_enabled = self.auto_system_check_enabled;
        current.auto_system_check_interval_minutes = self.auto_system_check_interval_minutes;

        if self.clear_api_key {
            current.api_key = None;
        } else if let Some(api_key) = self.api_key {
            let trimmed = api_key.trim();
            if !trimmed.is_empty() {
                current.api_key = Some(trimmed.to_string());
            }
        }
    }
}

fn default_auto_system_check_interval_minutes() -> u64 {
    10
}
