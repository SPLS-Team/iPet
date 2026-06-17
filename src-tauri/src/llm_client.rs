use crate::app_error::{AppError, AppResult};
use crate::config::LlmSettings;
use crate::tool_dispatcher::ToolDispatcher;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatUiMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub request_id: String,
    pub messages: Vec<ChatUiMessage>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub total_tokens: i64,
}

impl TokenUsage {
    pub fn add(&mut self, other: Option<&TokenUsage>) {
        if let Some(other) = other {
            self.prompt_tokens += other.prompt_tokens;
            self.completion_tokens += other.completion_tokens;
            self.total_tokens += other.total_tokens;
        }
    }
}

#[derive(Debug)]
pub enum PreparedTurn {
    DirectText {
        text: String,
        usage: Option<TokenUsage>,
    },
    ToolAugmented {
        messages: Vec<OpenAiMessage>,
        usage: Option<TokenUsage>,
        tool_call_count: usize,
    },
}

#[derive(Debug)]
pub struct StreamResult {
    pub text: String,
    pub usage: Option<TokenUsage>,
}

pub struct LlmClient {
    settings: LlmSettings,
    http: reqwest::Client,
}

impl LlmClient {
    pub fn new(settings: LlmSettings) -> AppResult<Self> {
        if settings
            .api_key
            .as_ref()
            .map(|key| key.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(AppError::Config(
                "请先在设置页面配置 API Key".to_string(),
            ));
        }
        settings.validate_public_fields().map_err(AppError::Config)?;
        Ok(Self {
            settings,
            http: reqwest::Client::new(),
        })
    }

    pub fn model(&self) -> &str {
        &self.settings.model
    }

    pub async fn prepare_turn_with_tools(
        &self,
        ui_messages: &[ChatUiMessage],
        dispatcher: &ToolDispatcher,
    ) -> AppResult<PreparedTurn> {
        let mut messages = self.build_messages(ui_messages);
        let tools = dispatcher.active_definitions()?;
        let response = if tools.is_empty() {
            self.complete_once(&messages, None, None).await?
        } else {
            self.complete_once(&messages, Some(tools), Some(json!("auto")))
                .await?
        };

        let usage = response.usage.clone();
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Model("模型没有返回候选结果".to_string()))?;
        let message = choice.message;

        if let Some(tool_calls) = message.tool_calls.filter(|calls| !calls.is_empty()) {
            let tool_call_count = tool_calls.len();
            messages.push(OpenAiMessage {
                role: "assistant".to_string(),
                content: message.content,
                tool_call_id: None,
                tool_calls: Some(tool_calls.clone()),
            });

            for call in tool_calls {
                let result = dispatcher
                    .dispatch(&call.function.name, &call.function.arguments)
                    .await?;
                messages.push(OpenAiMessage {
                    role: "tool".to_string(),
                    content: Some(result),
                    tool_call_id: Some(call.id),
                    tool_calls: None,
                });
            }

            Ok(PreparedTurn::ToolAugmented {
                messages,
                usage,
                tool_call_count,
            })
        } else {
            Ok(PreparedTurn::DirectText {
                text: message.content.unwrap_or_default(),
                usage,
            })
        }
    }

    pub async fn stream_final_response<F, Fut>(
        &self,
        messages: Vec<OpenAiMessage>,
        mut on_delta: F,
    ) -> AppResult<StreamResult>
    where
        F: FnMut(String) -> Fut,
        Fut: Future<Output = AppResult<()>>,
    {
        let body = json!({
            "model": self.settings.model,
            "messages": messages,
            "temperature": self.settings.temperature,
            "stream": true,
            "stream_options": {
                "include_usage": true
            }
        });

        let response = self
            .http
            .post(self.chat_completions_url())
            .bearer_auth(self.settings.api_key.as_deref().unwrap_or_default())
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut final_text = String::new();
        let mut usage = None;

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(index) = buffer.find("\n\n") {
                let event = buffer[..index].to_string();
                buffer = buffer[index + 2..].to_string();

                for line in event.lines() {
                    let Some(data) = line.strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim();
                    if data == "[DONE]" {
                        return Ok(StreamResult {
                            text: final_text,
                            usage,
                        });
                    }
                    if data.is_empty() {
                        continue;
                    }

                    let chunk: StreamChunk = serde_json::from_str(data)?;
                    if chunk.usage.is_some() {
                        usage = chunk.usage;
                    }
                    for choice in chunk.choices {
                        if let Some(content) = choice.delta.content {
                            final_text.push_str(&content);
                            on_delta(content).await?;
                        }
                    }
                }
            }
        }

        Ok(StreamResult {
            text: final_text,
            usage,
        })
    }

    fn build_messages(&self, ui_messages: &[ChatUiMessage]) -> Vec<OpenAiMessage> {
        let mut messages = vec![OpenAiMessage {
            role: "system".to_string(),
            content: Some(self.settings.system_prompt.clone()),
            tool_call_id: None,
            tool_calls: None,
        }];

        let keep = self.settings.max_context_messages.min(ui_messages.len());
        for message in ui_messages.iter().skip(ui_messages.len().saturating_sub(keep)) {
            let role = match message.role.as_str() {
                "assistant" => "assistant",
                "system" => "system",
                _ => "user",
            };
            messages.push(OpenAiMessage {
                role: role.to_string(),
                content: Some(message.content.clone()),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        messages
    }

    async fn complete_once(
        &self,
        messages: &[OpenAiMessage],
        tools: Option<Vec<Value>>,
        tool_choice: Option<Value>,
    ) -> AppResult<ChatCompletionResponse> {
        let mut body = json!({
            "model": self.settings.model,
            "messages": messages,
            "temperature": self.settings.temperature,
            "stream": false
        });

        if let Some(tools) = tools {
            body["tools"] = Value::Array(tools);
        }
        if let Some(tool_choice) = tool_choice {
            body["tool_choice"] = tool_choice;
        }

        let response = self
            .http
            .post(self.chat_completions_url())
            .bearer_auth(self.settings.api_key.as_deref().unwrap_or_default())
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<ChatCompletionResponse>()
            .await?;
        Ok(response)
    }

    fn chat_completions_url(&self) -> String {
        format!("{}/chat/completions", self.settings.normalized_base_url())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
    usage: Option<TokenUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
    usage: Option<TokenUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}
