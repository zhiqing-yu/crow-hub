//! Model Registry
//!
//! Tracks available model backends and their models.

use crate::{BackendInfo, BackendType, ModelBackend, ModelInfo, Result, ModelError};
use chrono::Utc;
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Registry of model backends and their available models
pub struct ModelRegistry {
    /// Backend name → backend instance
    backends: DashMap<String, Arc<dyn ModelBackend>>,
    /// Backend name → backend info (cached)
    backend_info: DashMap<String, BackendInfo>,
    /// Model id → backend name (lookup index)
    model_index: DashMap<String, String>,
}

impl ModelRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            backends: DashMap::new(),
            backend_info: DashMap::new(),
            model_index: DashMap::new(),
        }
    }

    /// Register a backend and discover its models
    pub async fn register(&self, backend: Arc<dyn ModelBackend>) -> Result<()> {
        let name = backend.name().to_string();
        let base_url = backend.base_url().to_string();
        let backend_type = backend.backend_type();

        // Try to list models
        let models = match backend.list_models().await {
            Ok(models) => {
                let model_ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
                // Index each model → backend
                for model in &models {
                    let qualified = format!("{}/{}", name, model.id);
                    self.model_index.insert(qualified, name.clone());
                    // Also register the bare model name for convenience
                    self.model_index.insert(model.id.clone(), name.clone());
                }
                info!(
                    "Registered backend '{}' ({}) with {} models: {:?}",
                    name, base_url, models.len(), model_ids
                );
                model_ids
            }
            Err(e) => {
                warn!("Backend '{}' registered but failed to list models: {}", name, e);
                Vec::new()
            }
        };

        // Store backend info
        let info = BackendInfo {
            name: name.clone(),
            backend_type,
            base_url,
            host: extract_host(backend.base_url()),
            healthy: true,
            models,
            last_check: Some(Utc::now()),
        };

        self.backend_info.insert(name.clone(), info);
        self.backends.insert(name, backend);

        Ok(())
    }

    /// Remove a backend
    pub fn remove(&self, name: &str) {
        // Remove model index entries pointing to this backend
        self.model_index.retain(|_, v| v != name);
        self.backends.remove(name);
        self.backend_info.remove(name);
        info!("Removed backend '{}'", name);
    }

    /// Get a backend by name
    pub fn get_backend(&self, name: &str) -> Option<Arc<dyn ModelBackend>> {
        self.backends.get(name).map(|b| b.clone())
    }

    /// Find which backend serves a given model
    pub fn find_backend_for_model(&self, model: &str) -> Option<Arc<dyn ModelBackend>> {
        self.model_index
            .get(model)
            .and_then(|backend_name| self.backends.get(backend_name.as_str()).map(|b| b.clone()))
    }

    /// Get the backend name for a model
    pub fn get_backend_name_for_model(&self, model: &str) -> Option<String> {
        self.model_index.get(model).map(|v| v.clone())
    }

    /// List all registered backends
    pub fn list_backends(&self) -> Vec<BackendInfo> {
        self.backend_info.iter().map(|e| e.value().clone()).collect()
    }

    /// List all available models across all backends
    pub fn list_models(&self) -> Vec<ModelInfo> {
        let mut models = Vec::new();
        for entry in self.backend_info.iter() {
            let info = entry.value();
            for model_id in &info.models {
                models.push(ModelInfo {
                    id: model_id.clone(),
                    backend_name: info.name.clone(),
                    display_name: Some(format!("{}/{}", info.name, model_id)),
                    context_length: None,
                    registered_at: info.last_check.unwrap_or_else(Utc::now),
                });
            }
        }
        models
    }

    /// Get total number of backends
    pub fn backend_count(&self) -> usize {
        self.backends.len()
    }

    /// Get total number of models
    pub fn model_count(&self) -> usize {
        // Count unique entries that are qualified (backend/model) to avoid double counting
        self.backend_info
            .iter()
            .map(|e| e.value().models.len())
            .sum()
    }

    /// Refresh a backend's model list
    pub async fn refresh(&self, name: &str) -> Result<()> {
        let backend = self.backends.get(name)
            .ok_or_else(|| ModelError::BackendNotFound(name.to_string()))?
            .clone();

        let models = backend.list_models().await?;
        let model_ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();

        // Update model index
        self.model_index.retain(|_, v| v != name);
        for model in &models {
            let qualified = format!("{}/{}", name, model.id);
            self.model_index.insert(qualified, name.to_string());
            self.model_index.insert(model.id.clone(), name.to_string());
        }

        // Update backend info
        if let Some(mut info) = self.backend_info.get_mut(name) {
            info.models = model_ids;
            info.last_check = Some(Utc::now());
            info.healthy = true;
        }

        debug!("Refreshed backend '{}'", name);
        Ok(())
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract hostname from a URL
fn extract_host(url: &str) -> String {
    url.trim_start_matches("http://")
        .trim_start_matches("https://")
        .split(':')
        .next()
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_host() {
        assert_eq!(extract_host("http://localhost:1234"), "localhost");
        assert_eq!(extract_host("http://192.168.50.1:8000"), "192.168.50.1");
        assert_eq!(extract_host("https://api.anthropic.com"), "api.anthropic.com");
    }

    #[test]
    fn test_empty_registry() {
        let reg = ModelRegistry::new();
        assert_eq!(reg.backend_count(), 0);
        assert_eq!(reg.model_count(), 0);
        assert!(reg.list_backends().is_empty());
    }
}
