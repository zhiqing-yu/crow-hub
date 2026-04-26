//! Mock Backend
//!
//! Returns configurable responses without any network calls. Used for testing.

use crate::{
    BackendType, ChatRequest, ChatResponse, FinishReason,
    ModelBackend, ModelInfo, Result, TokenUsage,
};
use chrono::Utc;
use parking_lot::RwLock;

/// A mock backend that returns predetermined responses
pub struct MockBackend {
    name: String,
    models: Vec<String>,
    /// Queued responses (FIFO)
    responses: RwLock<Vec<String>>,
    /// Default response when queue is empty
    default_response: String,
}

impl MockBackend {
    /// Create a new mock backend
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            models: vec!["mock-model".to_string()],
            responses: RwLock::new(Vec::new()),
            default_response: "Mock response from Crow Hub".to_string(),
        }
    }

    /// Set models this backend advertises
    pub fn with_models(mut self, models: Vec<String>) -> Self {
        self.models = models;
        self
    }

    /// Queue a response
    pub fn queue_response(&self, response: impl Into<String>) {
        self.responses.write().push(response.into());
    }

    /// Set the default response
    pub fn with_default_response(mut self, response: impl Into<String>) -> Self {
        self.default_response = response.into();
        self
    }
}

#[async_trait::async_trait]
impl ModelBackend for MockBackend {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let content = {
            let mut responses = self.responses.write();
            if responses.is_empty() {
                self.default_response.clone()
            } else {
                responses.remove(0)
            }
        };

        // Simulate token usage
        let input_tokens = request.messages.iter().map(|m| m.content.len() as u64 / 4).sum();
        let output_tokens = content.len() as u64 / 4;

        Ok(ChatResponse {
            content,
            model: request.model,
            backend: self.name.clone(),
            usage: TokenUsage {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
            },
            finish_reason: FinishReason::Stop,
        })
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        Ok(self
            .models
            .iter()
            .map(|id| ModelInfo {
                id: id.clone(),
                backend_name: self.name.clone(),
                display_name: Some(format!("mock/{}", id)),
                context_length: Some(4096),
                registered_at: Utc::now(),
            })
            .collect())
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(true)
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Mock
    }

    fn base_url(&self) -> &str {
        "mock://localhost"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChatMessage;

    #[tokio::test]
    async fn test_mock_default_response() {
        let backend = MockBackend::new("test-mock");
        let req = ChatRequest::simple("mock-model", "Hello");
        let resp = backend.chat(req).await.unwrap();
        assert_eq!(resp.content, "Mock response from Crow Hub");
        assert_eq!(resp.backend, "test-mock");
    }

    #[tokio::test]
    async fn test_mock_queued_responses() {
        let backend = MockBackend::new("test-mock");
        backend.queue_response("First response");
        backend.queue_response("Second response");

        let req1 = ChatRequest::simple("mock-model", "Hello");
        let resp1 = backend.chat(req1).await.unwrap();
        assert_eq!(resp1.content, "First response");

        let req2 = ChatRequest::simple("mock-model", "Hello again");
        let resp2 = backend.chat(req2).await.unwrap();
        assert_eq!(resp2.content, "Second response");

        // Queue empty, should return default
        let req3 = ChatRequest::simple("mock-model", "One more");
        let resp3 = backend.chat(req3).await.unwrap();
        assert_eq!(resp3.content, "Mock response from Crow Hub");
    }

    #[tokio::test]
    async fn test_mock_list_models() {
        let backend = MockBackend::new("test")
            .with_models(vec!["model-a".to_string(), "model-b".to_string()]);
        let models = backend.list_models().await.unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id, "model-a");
        assert_eq!(models[1].id, "model-b");
    }

    #[tokio::test]
    async fn test_mock_health() {
        let backend = MockBackend::new("test");
        assert!(backend.health_check().await.unwrap());
    }
}
