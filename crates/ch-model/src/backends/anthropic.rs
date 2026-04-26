//! Anthropic Claude Backend
//!
//! Handles the Anthropic-specific API format.

use crate::{
    BackendType, ChatMessage, ChatRequest, ChatResponse, ChatRole, FinishReason,
    ModelBackend, ModelError, ModelInfo, Result, TokenUsage,
};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Anthropic API backend
pub struct AnthropicBackend {
    name: String,
    base_url: String,
    api_key: String,
    client: Client,
    /// Default models to advertise
    models: Vec<String>,
}

impl AnthropicBackend {
    /// Create a new Anthropic backend
    pub fn new(
        name: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key: api_key.into(),
            client: Client::new(),
            models: vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-opus-4-20250514".to_string(),
                "claude-haiku-3-5-20241022".to_string(),
            ],
        }
    }

    /// Set custom base URL (for proxy etc.)
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into().trim_end_matches('/').to_string();
        self
    }

    /// Set the list of models to advertise
    pub fn with_models(mut self, models: Vec<String>) -> Self {
        self.models = models;
        self
    }
}

#[async_trait::async_trait]
impl ModelBackend for AnthropicBackend {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/v1/messages", self.base_url);

        // Separate system message from conversation
        let mut system_msg = None;
        let mut messages = Vec::new();

        for msg in &request.messages {
            match msg.role {
                ChatRole::System => {
                    system_msg = Some(msg.content.clone());
                }
                ChatRole::User => {
                    messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: msg.content.clone(),
                    });
                }
                ChatRole::Assistant => {
                    messages.push(AnthropicMessage {
                        role: "assistant".to_string(),
                        content: msg.content.clone(),
                    });
                }
                ChatRole::Tool => {
                    // Map tool to user for now
                    messages.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: msg.content.clone(),
                    });
                }
            }
        }

        let body = AnthropicRequest {
            model: request.model.clone(),
            messages,
            system: system_msg,
            max_tokens: request.max_tokens.unwrap_or(4096),
            temperature: request.temperature,
        };

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ModelError::Connection(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if status.as_u16() == 401 {
                return Err(ModelError::Authentication("Invalid API key".to_string()));
            }
            if status.as_u16() == 429 {
                return Err(ModelError::RateLimit(body));
            }
            return Err(ModelError::Backend(format!("HTTP {}: {}", status, body)));
        }

        let api_resp: AnthropicResponse = resp
            .json()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;

        let content = api_resp
            .content
            .iter()
            .filter_map(|c| {
                if c.content_type == "text" {
                    Some(c.text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        Ok(ChatResponse {
            content,
            model: api_resp.model,
            backend: self.name.clone(),
            usage: TokenUsage {
                input_tokens: api_resp.usage.input_tokens,
                output_tokens: api_resp.usage.output_tokens,
                total_tokens: api_resp.usage.input_tokens + api_resp.usage.output_tokens,
            },
            finish_reason: match api_resp.stop_reason.as_deref() {
                Some("end_turn") => FinishReason::Stop,
                Some("max_tokens") => FinishReason::Length,
                Some("tool_use") => FinishReason::ToolCalls,
                _ => FinishReason::Stop,
            },
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(self
            .models
            .iter()
            .map(|id| ModelInfo {
                id: id.clone(),
                backend_name: self.name.clone(),
                display_name: Some(format!("anthropic/{}", id)),
                context_length: Some(200000),
                registered_at: Utc::now(),
            })
            .collect())
    }

    async fn health_check(&self) -> Result<bool> {
        // For cloud APIs, we just check if we have a key
        Ok(!self.api_key.is_empty())
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Anthropic
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }
}

// ── Anthropic API types ──────────────────────────────────────

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    model: String,
    content: Vec<AnthropicContent>,
    usage: AnthropicUsage,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: u64,
    output_tokens: u64,
}
