//! Model Router
//!
//! Routes chat requests to the appropriate backend based on model name.

use crate::{ChatRequest, ChatResponse, ModelBackend, ModelRegistry, Result, ModelError, ModelInfo, BackendInfo};
use std::sync::Arc;
use tracing::{debug, info};

/// Routes model requests to the correct backend
pub struct ModelRouter {
    /// Registry of available backends and models
    registry: Arc<ModelRegistry>,
}

impl ModelRouter {
    /// Create a new router with the given registry
    pub fn new(registry: Arc<ModelRegistry>) -> Self {
        Self { registry }
    }

    /// Send a chat request, routing to the appropriate backend
    pub async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let model = &request.model;

        let backend = self.registry
            .find_backend_for_model(model)
            .ok_or_else(|| ModelError::ModelNotFound(format!(
                "No backend found for model '{}'. Available: {:?}",
                model,
                self.list_models().iter().map(|m| &m.id).collect::<Vec<_>>()
            )))?;

        debug!(
            "Routing '{}' request to backend '{}'",
            model,
            backend.name()
        );

        backend.chat(request).await
    }

    /// Register a backend with the router
    pub async fn register_backend(&self, backend: Arc<dyn ModelBackend>) -> Result<()> {
        self.registry.register(backend).await
    }

    /// List all available models
    pub fn list_models(&self) -> Vec<ModelInfo> {
        self.registry.list_models()
    }

    /// List all backends
    pub fn list_backends(&self) -> Vec<BackendInfo> {
        self.registry.list_backends()
    }

    /// Get the underlying registry
    pub fn registry(&self) -> &Arc<ModelRegistry> {
        &self.registry
    }

    /// Check health of all backends
    pub async fn health_check_all(&self) -> Vec<(String, bool)> {
        let mut results = Vec::new();
        for info in self.registry.list_backends() {
            if let Some(backend) = self.registry.get_backend(&info.name) {
                let healthy = backend.health_check().await.unwrap_or(false);
                results.push((info.name, healthy));
            }
        }
        results
    }

    /// Summary of current state for display
    pub fn summary(&self) -> RouterSummary {
        let backends = self.registry.list_backends();
        RouterSummary {
            total_backends: backends.len(),
            healthy_backends: backends.iter().filter(|b| b.healthy).count(),
            total_models: self.registry.model_count(),
            backends: backends
                .iter()
                .map(|b| format!("{} ({}, {} models)", b.name, b.base_url, b.models.len()))
                .collect(),
        }
    }
}

/// Summary of router state
#[derive(Debug, Clone)]
pub struct RouterSummary {
    pub total_backends: usize,
    pub healthy_backends: usize,
    pub total_models: usize,
    pub backends: Vec<String>,
}

impl std::fmt::Display for RouterSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Model Router: {} backends, {} models", self.total_backends, self.total_models)?;
        for b in &self.backends {
            writeln!(f, "  • {}", b)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::MockBackend;
    use crate::ChatRequest;

    #[tokio::test]
    async fn test_router_with_mock_backend() {
        let registry = Arc::new(ModelRegistry::new());
        let router = ModelRouter::new(registry);

        let mock = MockBackend::new("test-backend")
            .with_models(vec!["model-a".to_string(), "model-b".to_string()])
            .with_default_response("Hello from mock!".to_string());

        router.register_backend(Arc::new(mock)).await.unwrap();

        assert_eq!(router.list_models().len(), 2);
        assert_eq!(router.list_backends().len(), 1);

        let req = ChatRequest::simple("model-a", "Hi");
        let resp = router.chat(req).await.unwrap();
        assert_eq!(resp.content, "Hello from mock!");
        assert_eq!(resp.backend, "test-backend");
    }

    #[tokio::test]
    async fn test_router_model_not_found() {
        let registry = Arc::new(ModelRegistry::new());
        let router = ModelRouter::new(registry);

        let req = ChatRequest::simple("nonexistent-model", "Hi");
        assert!(router.chat(req).await.is_err());
    }

    #[tokio::test]
    async fn test_router_multiple_backends() {
        let registry = Arc::new(ModelRegistry::new());
        let router = ModelRouter::new(registry);

        let mock1 = MockBackend::new("backend-1")
            .with_models(vec!["llama3".to_string()])
            .with_default_response("From backend 1");
        let mock2 = MockBackend::new("backend-2")
            .with_models(vec!["gpt-4".to_string()])
            .with_default_response("From backend 2");

        router.register_backend(Arc::new(mock1)).await.unwrap();
        router.register_backend(Arc::new(mock2)).await.unwrap();

        let resp1 = router.chat(ChatRequest::simple("llama3", "Hi")).await.unwrap();
        assert_eq!(resp1.content, "From backend 1");

        let resp2 = router.chat(ChatRequest::simple("gpt-4", "Hi")).await.unwrap();
        assert_eq!(resp2.content, "From backend 2");
    }

    #[tokio::test]
    async fn test_router_summary() {
        let registry = Arc::new(ModelRegistry::new());
        let router = ModelRouter::new(registry);

        let mock = MockBackend::new("test")
            .with_models(vec!["m1".to_string(), "m2".to_string()]);
        router.register_backend(Arc::new(mock)).await.unwrap();

        let summary = router.summary();
        assert_eq!(summary.total_backends, 1);
        assert_eq!(summary.total_models, 2);
    }
}
