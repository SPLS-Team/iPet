use crate::app_error::{AppError, AppResult};
use crate::config::LlmSettings;
use crate::tool_dispatcher::ToolDispatcher;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;

/// Upper bound on tool-call rounds. The final allowed round drops tools from
/// the request so the model is forced to produce a text answer instead of
/// asking for yet another tool call.
const MAX_TOOL_ROUNDS: usize = 6;

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

/// Result of running a full chat turn (with or without tools).
#[derive(Debug, Default)]
pub struct ChatTurnResult {
    pub text: String,
    pub usage: TokenUsage,
    pub tool_call_count: usize,
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

    /// Drive a chat turn through repeated tool calls until the model returns
    /// a plain text reply or the round cap is hit.
    ///
    /// Behavior:
    /// - Each round sends a non-streaming `chat/completions` request with the
    ///   active tool definitions and `tool_choice = "auto"`.
    /// - If the response carries `tool_calls`, every call is dispatched
    ///   serially (tool errors are surfaced to the model as a JSON `{"error":
    ///   "..."}` payload so it can adapt instead of failing the whole turn),
    ///   the assistant + tool messages are appended, and the loop repeats.
    /// - If the response contains no tool calls, the loop returns the text.
    /// - The last allowed round drops tools from the request, forcing a text
    ///   answer even when the model keeps asking for more tools.
    ///
    /// Tokens are summed across all rounds.
    pub async fn complete_with_tool_loop(
        &self,
        ui_messages: &[ChatUiMessage],
        dispatcher: &ToolDispatcher,
    ) -> AppResult<ChatTurnResult> {
        let mut messages = self.build_messages(ui_messages);
        let tools = dispatcher.active_definitions()?;
        let has_tools = !tools.is_empty();

        let mut usage_total = TokenUsage::default();
        let mut tool_call_count = 0usize;

        for round in 0..MAX_TOOL_ROUNDS {
            // Final allowed round: drop tools so the model must reply with
            // text. Without this guard, a misbehaving model could keep asking
            // for tools until we hit the cap and then error out.
            let allow_tools = has_tools && round + 1 < MAX_TOOL_ROUNDS;
            let response = if allow_tools {
                self.complete_once(&messages, Some(tools.clone()), Some(json!("auto")))
                    .await?
            } else {
                self.complete_once(&messages, None, None).await?
            };

            usage_total.add(response.usage.as_ref());
            let choice = response.choices.into_iter().next().ok_or_else(|| {
                AppError::Model("模型没有返回候选结果".to_string())
            })?;
            let message = choice.message;

            let pending_calls = message
                .tool_calls
                .as_ref()
                .map(|c| !c.is_empty())
                .unwrap_or(false);

            if !pending_calls {
                let text = message.content.unwrap_or_default();
                tracing::debug!(
                    rounds = round + 1,
                    tool_calls = tool_call_count,
                    chars = text.chars().count(),
                    "chat loop converged"
                );
                return Ok(ChatTurnResult {
                    text,
                    usage: usage_total,
                    tool_call_count,
                });
            }

            let tool_calls = message.tool_calls.unwrap_or_default();
            tool_call_count += tool_calls.len();

            messages.push(OpenAiMessage {
                role: "assistant".to_string(),
                content: message.content,
                tool_call_id: None,
                tool_calls: Some(tool_calls.clone()),
            });

            for call in tool_calls {
                let result = match dispatcher
                    .dispatch(&call.function.name, &call.function.arguments)
                    .await
                {
                    Ok(json) => json,
                    Err(err) => {
                        // Don't abort the whole turn — feed the error back to
                        // the model so it can apologize or try a different
                        // approach. ToolDispatcher::dispatch already logs the
                        // failure at warn! level.
                        json!({ "error": err.to_string() }).to_string()
                    }
                };
                messages.push(OpenAiMessage {
                    role: "tool".to_string(),
                    content: Some(result),
                    tool_call_id: Some(call.id),
                    tool_calls: None,
                });
            }
        }

        // The loop body always returns when round + 1 == MAX_TOOL_ROUNDS, so
        // this is unreachable. Keep an explicit error for safety.
        Err(AppError::Model(format!(
            "工具调用未在 {MAX_TOOL_ROUNDS} 轮内收敛"
        )))
    }

    /// Stream a response without using any tools. Used by the no-tools fast
    /// path where we don't need to inspect the response mid-stream for tool
    /// calls.
    pub async fn stream_simple<F, Fut>(
        &self,
        ui_messages: &[ChatUiMessage],
        on_delta: F,
    ) -> AppResult<ChatTurnResult>
    where
        F: FnMut(String) -> Fut,
        Fut: Future<Output = AppResult<()>>,
    {
        let messages = self.build_messages(ui_messages);
        let stream = self.stream_messages(messages, on_delta).await?;
        Ok(ChatTurnResult {
            text: stream.text,
            usage: stream.usage.unwrap_or_default(),
            tool_call_count: 0,
        })
    }

    async fn stream_messages<F, Fut>(
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

struct StreamResult {
    text: String,
    usage: Option<TokenUsage>,
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
