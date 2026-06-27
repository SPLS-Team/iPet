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
    /// Fire an OS notification when a chat reply completes (user-facing toggle
    /// in the System view). Off by default.
    #[serde(default)]
    pub notify_on_reply: bool,
    /// Fire an OS notification when the auto system-check detects a high-load
    /// condition (CPU or memory above the alert threshold).
    #[serde(default)]
    pub notify_on_system_alert: bool,
    /// Sample the foreground window in the background and accumulate per-app
    /// usage time (the desktop analogue of mobile "screen time"). On by
    /// default; the System view exposes a toggle.
    #[serde(default = "default_track_app_usage")]
    pub track_app_usage: bool,
    /// When the user has been idle (no input) for at least this many minutes,
    /// the sampler skips crediting the foreground app. 0 disables idle
    /// filtering. Defaults to 5.
    #[serde(default = "default_app_usage_idle_minutes")]
    pub app_usage_idle_minutes: u64,
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
            notify_on_reply: false,
            notify_on_system_alert: false,
            track_app_usage: default_track_app_usage(),
            app_usage_idle_minutes: default_app_usage_idle_minutes(),
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
        if !(0..=240).contains(&self.app_usage_idle_minutes) {
            return Err("使用时长空闲阈值必须在 0 到 240 分钟之间".to_string());
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
    pub notify_on_reply: bool,
    pub notify_on_system_alert: bool,
    pub track_app_usage: bool,
    pub app_usage_idle_minutes: u64,
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
            notify_on_reply: settings.notify_on_reply,
            notify_on_system_alert: settings.notify_on_system_alert,
            track_app_usage: settings.track_app_usage,
            app_usage_idle_minutes: settings.app_usage_idle_minutes,
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
    pub notify_on_reply: bool,
    pub notify_on_system_alert: bool,
    #[serde(default = "default_track_app_usage")]
    pub track_app_usage: bool,
    #[serde(default = "default_app_usage_idle_minutes")]
    pub app_usage_idle_minutes: u64,
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
        current.notify_on_reply = self.notify_on_reply;
        current.notify_on_system_alert = self.notify_on_system_alert;
        current.track_app_usage = self.track_app_usage;
        current.app_usage_idle_minutes = self.app_usage_idle_minutes;

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

fn default_track_app_usage() -> bool {
    true
}

fn default_app_usage_idle_minutes() -> u64 {
    5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> LlmSettings {
        LlmSettings::default()
    }

    #[test]
    fn default_validates_ok() {
        defaults().validate_public_fields().expect("defaults must validate");
    }

    #[test]
    fn normalized_base_url_strips_trailing_slash_and_whitespace() {
        let mut s = defaults();
        s.base_url = "  https://api.example.com/v1///   ".to_string();
        assert_eq!(s.normalized_base_url(), "https://api.example.com/v1");
    }

    #[test]
    fn validate_rejects_blank_base_url() {
        let mut s = defaults();
        s.base_url = "   /".to_string();
        let err = s.validate_public_fields().unwrap_err();
        assert!(err.contains("Base URL"), "unexpected error: {err}");
    }

    #[test]
    fn validate_rejects_blank_model() {
        let mut s = defaults();
        s.model = "   ".to_string();
        assert!(s.validate_public_fields().is_err());
    }

    #[test]
    fn validate_rejects_out_of_range_temperature() {
        let mut s = defaults();
        s.temperature = -0.1;
        assert!(s.validate_public_fields().is_err());
        s.temperature = 2.5;
        assert!(s.validate_public_fields().is_err());
    }

    #[test]
    fn validate_rejects_out_of_range_context() {
        let mut s = defaults();
        s.max_context_messages = 2;
        assert!(s.validate_public_fields().is_err());
        s.max_context_messages = 200;
        assert!(s.validate_public_fields().is_err());
    }

    #[test]
    fn validate_rejects_out_of_range_auto_check_interval() {
        let mut s = defaults();
        s.auto_system_check_interval_minutes = 0;
        assert!(s.validate_public_fields().is_err());
        s.auto_system_check_interval_minutes = 9999;
        assert!(s.validate_public_fields().is_err());
    }

    #[test]
    fn validate_rejects_out_of_range_idle_minutes() {
        let mut s = defaults();
        s.app_usage_idle_minutes = 0;
        assert!(s.validate_public_fields().is_ok(), "0 disables idle filter");
        s.app_usage_idle_minutes = 241;
        assert!(s.validate_public_fields().is_err());
    }

    #[test]
    fn defaults_track_app_usage_enabled() {
        let s = defaults();
        assert!(s.track_app_usage, "usage tracking defaults on");
        assert_eq!(s.app_usage_idle_minutes, 5);
    }

    #[test]
    fn merge_input_clears_api_key_when_requested() {
        let mut s = defaults();
        s.api_key = Some("secret".into());
        let input = LlmSettingsInput {
            api_key: Some("ignored".into()),
            clear_api_key: true,
            base_url: s.base_url.clone(),
            model: s.model.clone(),
            temperature: s.temperature,
            max_context_messages: s.max_context_messages,
            system_prompt: s.system_prompt.clone(),
            auto_system_check_enabled: false,
            auto_system_check_interval_minutes: 10,
            notify_on_reply: false,
            notify_on_system_alert: false,
            track_app_usage: true,
            app_usage_idle_minutes: 5,
        };
        input.merge_into(&mut s);
        assert!(s.api_key.is_none(), "clear_api_key should win over a provided key");
    }

    #[test]
    fn merge_input_keeps_existing_key_when_blank_provided() {
        let mut s = defaults();
        s.api_key = Some("keepme".into());
        let input = LlmSettingsInput {
            api_key: Some("   ".into()),
            clear_api_key: false,
            base_url: s.base_url.clone(),
            model: s.model.clone(),
            temperature: s.temperature,
            max_context_messages: s.max_context_messages,
            system_prompt: s.system_prompt.clone(),
            auto_system_check_enabled: false,
            auto_system_check_interval_minutes: 10,
            notify_on_reply: false,
            notify_on_system_alert: false,
            track_app_usage: true,
            app_usage_idle_minutes: 5,
        };
        input.merge_into(&mut s);
        assert_eq!(s.api_key.as_deref(), Some("keepme"));
    }

    #[test]
    fn settings_status_reports_api_key_presence_without_leaking_value() {
        let mut s = defaults();
        s.api_key = Some("sk-secret".into());
        let status = LlmSettingsStatus::from_settings(&s, PathBuf::from("/tmp/x.sqlite"));
        assert!(status.has_api_key);

        s.api_key = Some("   ".into());
        let status = LlmSettingsStatus::from_settings(&s, PathBuf::from("/tmp/x.sqlite"));
        assert!(
            !status.has_api_key,
            "blank-only api_key should be reported as absent"
        );

        s.api_key = None;
        let status = LlmSettingsStatus::from_settings(&s, PathBuf::from("/tmp/x.sqlite"));
        assert!(!status.has_api_key);
    }
}
