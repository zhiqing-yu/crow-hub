//! API Driver
//!
//! Connects to agents via HTTP APIs using the ModelRouter.
//! Works for cloud APIs (Anthropic, Google, Moonshot) and
//! local model servers (Ollama, vLLM, LM Studio).

use crate::drivers::AgentDriver;
use crate::manifest::{AgentManifest, AuthSection};
use crate::{AgentError, Result};
use ch_model::{ChatRequest, ChatResponse, ChatStreamChunk, ModelRouter};
use futures::stream::{self, BoxStream, StreamExt};
use std::sync::Arc;
use tracing::debug;

/// Driver that routes chat requests through the ModelRouter
pub struct APIDriver {
    /// Agent name
    name: String,
    /// Default model to use
    default_model: String,
    /// Model router for sending requests
    router: Arc<ModelRouter>,
}

impl APIDriver {
    /// Create a new API driver
    pub fn new(
        name: impl Into<String>,
        default_model: impl Into<String>,
        router: Arc<ModelRouter>,
    ) -> Self {
        Self {
            name: name.into(),
            default_model: default_model.into(),
            router,
        }
    }

    /// Create from a manifest
    pub fn from_manifest(manifest: &AgentManifest, router: Arc<ModelRouter>) -> Result<Self> {
        let model_section = manifest.model.as_ref()
            .ok_or_else(|| AgentError::Manifest("API driver requires [model] section".to_string()))?;

        Ok(Self::new(
            &manifest.agent.name,
            &model_section.default,
            router,
        ))
    }
}

#[async_trait::async_trait]
impl AgentDriver for APIDriver {
    async fn chat(&self, mut request: ChatRequest) -> Result<ChatResponse> {
        // If no model specified in request, use default
        if request.model.is_empty() {
            request.model = self.default_model.clone();
        }

        debug!("APIDriver '{}' sending request to model '{}'", self.name, request.model);

        self.router.chat(request).await
            .map_err(|e| AgentError::Driver(e.to_string()))
    }

    async fn stream_chat(&self, request: ChatRequest) -> Result<BoxStream<'static, Result<ChatStreamChunk>>> {
        let resp = self.chat(request).await?;
        let chunk = ChatStreamChunk {
            content: resp.content,
            is_final: true,
            finish_reason: Some(resp.finish_reason),
            usage: Some(resp.usage),
        };
        Ok(stream::once(async move { Ok(chunk) }).boxed())
    }

    async fn health_check(&self) -> Result<bool> {
        // Check if our default model is available
        let models = self.router.list_models();
        Ok(models.iter().any(|m| m.id == self.default_model))
    }

    fn driver_type(&self) -> &str {
        "api"
    }

    async fn stop(&self) -> Result<()> {
        Ok(()) // Nothing to clean up for API driver
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ch_model::{ModelRegistry, backends::MockBackend};

    #[tokio::test]
    async fn test_api_driver_chat() {
        let registry = Arc::new(ModelRegistry::new());
        let mock = MockBackend::new("test")
            .with_models(vec!["test-model".to_string()])
            .with_default_response("Hello from API driver!");
        registry.register(Arc::new(mock)).await.unwrap();

        let router = Arc::new(ModelRouter::new(registry));
        let driver = APIDriver::new("test-agent", "test-model", router);

        let request = ChatRequest::simple("test-model", "Hi");
        let response = driver.chat(request).await.unwrap();
        assert_eq!(response.content, "Hello from API driver!");
    }

    #[tokio::test]
    async fn test_api_driver_uses_default_model() {
        let registry = Arc::new(ModelRegistry::new());
        let mock = MockBackend::new("test")
            .with_models(vec!["my-default".to_string()])
            .with_default_response("Default model response");
        registry.register(Arc::new(mock)).await.unwrap();

        let router = Arc::new(ModelRouter::new(registry));
        let driver = APIDriver::new("test-agent", "my-default", router);

        // Empty model name → should use default
        let request = ChatRequest::simple("", "Hi");
        let response = driver.chat(request).await.unwrap();
        assert_eq!(response.content, "Default model response");
    }

    #[tokio::test]
    async fn test_api_driver_health_check() {
        let registry = Arc::new(ModelRegistry::new());
        let mock = MockBackend::new("test")
            .with_models(vec!["available-model".to_string()]);
        registry.register(Arc::new(mock)).await.unwrap();

        let router = Arc::new(ModelRouter::new(registry));

        let healthy = APIDriver::new("a", "available-model", router.clone());
        assert!(healthy.health_check().await.unwrap());

        let unhealthy = APIDriver::new("b", "nonexistent-model", router);
        assert!(!unhealthy.health_check().await.unwrap());
    }
}
