//! Crow Hub Model Routing System
//!
//! Provides a unified interface for routing AI model requests
//! to different backends (OpenAI-compat, Anthropic, Ollama, etc.)
//! with automatic discovery of local model servers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub mod backends;
pub mod discovery;
pub mod registry;
pub mod router;

pub use discovery::AutoDiscovery;
pub use registry::ModelRegistry;
pub use router::ModelRouter;

// ── Error types ──────────────────────────────────────────────

/// Model system errors
#[derive(Error, Debug, Clone)]
pub enum ModelError {
    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Backend not found: {0}")]
    BackendNotFound(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    #[error("Request timeout after {0}s")]
    Timeout(u64),

    #[error("Discovery error: {0}")]
    Discovery(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// Result type for model operations
pub type Result<T> = std::result::Result<T, ModelError>;

// ── Core types ───────────────────────────────────────────────

/// A chat message in the unified format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

/// Message roles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A chat completion request (backend-agnostic)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Model identifier (e.g. "claude-sonnet-4-6", "llama3")
    pub model: String,
    /// Conversation messages
    pub messages: Vec<ChatMessage>,
    /// Optional temperature (0.0 - 2.0)
    pub temperature: Option<f32>,
    /// Optional max tokens for response
    pub max_tokens: Option<u32>,
    /// Optional stop sequences
    pub stop: Option<Vec<String>>,
}

impl ChatRequest {
    /// Create a simple single-message request
    pub fn simple(model: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            messages: vec![ChatMessage {
                role: ChatRole::User,
                content: message.into(),
            }],
            temperature: None,
            max_tokens: None,
            stop: None,
        }
    }
}

/// A chat completion response (backend-agnostic)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    /// The response content
    pub content: String,
    /// Which model produced this
    pub model: String,
    /// Which backend served it
    pub backend: String,
    /// Token usage
    pub usage: TokenUsage,
    /// Finish reason
    pub finish_reason: FinishReason,
}

/// Token usage information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// Why the model stopped generating
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error,
}

/// A chunk of a streaming chat completion response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatStreamChunk {
    /// The incremental content chunk
    pub content: String,
    /// Whether this is the final chunk
    pub is_final: bool,
    /// Finish reason (if final)
    pub finish_reason: Option<FinishReason>,
    /// Token usage (if final)
    pub usage: Option<TokenUsage>,
}

/// Information about an available model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Model identifier (e.g. "llama3", "claude-sonnet-4-6")
    pub id: String,
    /// Backend that serves this model
    pub backend_name: String,
    /// Human-readable display name
    pub display_name: Option<String>,
    /// Model size/context info
    pub context_length: Option<u64>,
    /// When this model was discovered/registered
    pub registered_at: DateTime<Utc>,
}

/// Information about a backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendInfo {
    /// Unique name for this backend (e.g. "spark-ollama", "local-lmstudio")
    pub name: String,
    /// Backend type
    pub backend_type: BackendType,
    /// Base URL
    pub base_url: String,
    /// Host that this backend runs on
    pub host: String,
    /// Whether the backend is currently healthy
    pub healthy: bool,
    /// Models available on this backend
    pub models: Vec<String>,
    /// When last health check passed
    pub last_check: Option<DateTime<Utc>>,
}

/// Types of model backends
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackendType {
    /// OpenAI-compatible API (vLLM, LM Studio, etc.)
    #[serde(rename = "openai_compat")]
    OpenAICompat,
    /// Anthropic Claude API
    #[serde(rename = "anthropic")]
    Anthropic,
    /// Ollama native API
    #[serde(rename = "ollama")]
    Ollama,
    /// Mock backend for testing
    #[serde(rename = "mock")]
    Mock,
}

// ── Model Backend trait ──────────────────────────────────────

/// Trait that all model backends must implement
#[async_trait::async_trait]
pub trait ModelBackend: Send + Sync {
    /// Send a chat completion request
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;

    /// List available models on this backend
    async fn list_models(&self) -> Result<Vec<ModelInfo>>;

    /// Check if the backend is reachable and healthy
    async fn health_check(&self) -> Result<bool>;

    /// Get the backend name
    fn name(&self) -> &str;

    /// Get the backend type
    fn backend_type(&self) -> BackendType;

    /// Get the base URL
    fn base_url(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_simple() {
        let req = ChatRequest::simple("llama3", "Hello world");
        assert_eq!(req.model, "llama3");
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, ChatRole::User);
    }

    #[test]
    fn test_backend_type_serialization() {
        let bt = BackendType::OpenAICompat;
        let json = serde_json::to_string(&bt).unwrap();
        assert_eq!(json, "\"openai_compat\"");
    }
}
