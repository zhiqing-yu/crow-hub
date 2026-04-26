//! Ollama Native Backend
//!
//! Uses Ollama's native /api/chat and /api/tags endpoints.

use crate::{
    BackendType, ChatMessage, ChatRequest, ChatResponse, FinishReason,
    ModelBackend, ModelError, ModelInfo, Result, TokenUsage,
};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Ollama native API backend
pub struct OllamaBackend {
    name: String,
    base_url: String,
    client: Client,
}

impl OllamaBackend {
    /// Create a new Ollama backend
    pub fn new(name: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl ModelBackend for OllamaBackend {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/api/chat", self.base_url);

        let messages: Vec<OllamaMessage> = request
            .messages
            .iter()
            .map(|m| OllamaMessage {
                role: format!("{:?}", m.role).to_lowercase(),
                content: m.content.clone(),
            })
            .collect();

        let body = OllamaChatRequest {
            model: request.model.clone(),
            messages,
            stream: false,
            options: request.temperature.map(|t| OllamaOptions {
                temperature: Some(t),
            }),
        };

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ModelError::Connection(format!("{}: {}", self.base_url, e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ModelError::Backend(format!("HTTP {}: {}", status, body)));
        }

        let api_resp: OllamaChatResponse = resp
            .json()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;

        Ok(ChatResponse {
            content: api_resp.message.content,
            model: api_resp.model,
            backend: self.name.clone(),
            usage: TokenUsage {
                input_tokens: api_resp.prompt_eval_count.unwrap_or(0),
                output_tokens: api_resp.eval_count.unwrap_or(0),
                total_tokens: api_resp.prompt_eval_count.unwrap_or(0)
                    + api_resp.eval_count.unwrap_or(0),
            },
            finish_reason: if api_resp.done {
                FinishReason::Stop
            } else {
                FinishReason::Length
            },
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/api/tags", self.base_url);

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ModelError::Connection(format!("{}: {}", self.base_url, e)))?;

        if !resp.status().is_success() {
            return Err(ModelError::Backend(format!(
                "HTTP {} from {}",
                resp.status(),
                self.base_url
            )));
        }

        let tags: OllamaTagsResponse = resp
            .json()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;

        Ok(tags
            .models
            .into_iter()
            .map(|m| ModelInfo {
                id: m.name,
                backend_name: self.name.clone(),
                display_name: None,
                context_length: None,
                registered_at: Utc::now(),
            })
            .collect())
    }

    async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Ollama
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }
}

// ── Ollama API types ─────────────────────────────────────────

#[derive(Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Deserialize)]
struct OllamaChatResponse {
    model: String,
    message: OllamaMessage,
    done: bool,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModelEntry>,
}

#[derive(Deserialize)]
struct OllamaModelEntry {
    name: String,
}
