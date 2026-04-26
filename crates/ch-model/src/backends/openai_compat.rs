//! OpenAI-Compatible Backend
//!
//! Works with vLLM, LM Studio, Ollama's /v1 endpoint, and any OpenAI-compatible server.

use crate::{
    BackendType, ChatMessage, ChatRequest, ChatResponse, FinishReason,
    ModelBackend, ModelError, ModelInfo, Result, TokenUsage,
};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Backend for OpenAI-compatible APIs
pub struct OpenAICompatBackend {
    name: String,
    base_url: String,
    api_key: Option<String>,
    client: Client,
}

impl OpenAICompatBackend {
    /// Create a new OpenAI-compatible backend
    pub fn new(
        name: impl Into<String>,
        base_url: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait::async_trait]
impl ModelBackend for OpenAICompatBackend {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let api_messages: Vec<APIMessage> = request
            .messages
            .iter()
            .map(|m| APIMessage {
                role: format!("{:?}", m.role).to_lowercase(),
                content: m.content.clone(),
            })
            .collect();

        let body = APIRequest {
            model: request.model.clone(),
            messages: api_messages,
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            stop: request.stop.clone(),
        };

        let mut req = self.client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ModelError::Connection(format!("{}: {}", self.base_url, e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ModelError::Backend(format!(
                "HTTP {} from {}: {}",
                status, self.base_url, body
            )));
        }

        let api_resp: APIResponse = resp
            .json()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;

        let choice = api_resp
            .choices
            .first()
            .ok_or_else(|| ModelError::InvalidResponse("No choices in response".to_string()))?;

        Ok(ChatResponse {
            content: choice.message.content.clone().unwrap_or_default(),
            model: api_resp.model,
            backend: self.name.clone(),
            usage: TokenUsage {
                input_tokens: api_resp.usage.as_ref().map(|u| u.prompt_tokens).unwrap_or(0),
                output_tokens: api_resp.usage.as_ref().map(|u| u.completion_tokens).unwrap_or(0),
                total_tokens: api_resp.usage.as_ref().map(|u| u.total_tokens).unwrap_or(0),
            },
            finish_reason: match choice.finish_reason.as_deref() {
                Some("stop") => FinishReason::Stop,
                Some("length") => FinishReason::Length,
                Some("tool_calls") => FinishReason::ToolCalls,
                Some("content_filter") => FinishReason::ContentFilter,
                _ => FinishReason::Stop,
            },
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let url = format!("{}/v1/models", self.base_url);

        let mut req = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req
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

        let models_resp: ModelsResponse = resp
            .json()
            .await
            .map_err(|e| ModelError::InvalidResponse(e.to_string()))?;

        Ok(models_resp
            .data
            .into_iter()
            .map(|m| ModelInfo {
                id: m.id,
                backend_name: self.name.clone(),
                display_name: None,
                context_length: None,
                registered_at: Utc::now(),
            })
            .collect())
    }

    async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/v1/models", self.base_url);
        match self.client.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn backend_type(&self) -> BackendType {
        BackendType::OpenAICompat
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }
}

// ── OpenAI API types ─────────────────────────────────────────

#[derive(Serialize)]
struct APIRequest {
    model: String,
    messages: Vec<APIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize)]
struct APIMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct APIResponse {
    model: String,
    choices: Vec<APIChoice>,
    usage: Option<APIUsage>,
}

#[derive(Deserialize)]
struct APIChoice {
    message: APIChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct APIChoiceMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct APIUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelsEntry>,
}

#[derive(Deserialize)]
struct ModelsEntry {
    id: String,
}
