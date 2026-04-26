//! Auto-Discovery
//!
//! Scans configured hosts for running model servers.

use crate::{ModelError, Result, BackendType};
use crate::backends;
use crate::registry::ModelRegistry;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for auto-discovery
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Hosts to scan
    pub hosts: Vec<HostConfig>,
    /// Timeout for health check probes (ms)
    pub probe_timeout_ms: u64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            hosts: vec![HostConfig::localhost()],
            probe_timeout_ms: 2000,
        }
    }
}

/// A host to scan for model servers
#[derive(Debug, Clone)]
pub struct HostConfig {
    /// Human-readable name (e.g. "local", "spark")
    pub name: String,
    /// IP address or hostname
    pub address: String,
    /// Ports to scan
    pub ports: Vec<u16>,
}

impl HostConfig {
    /// Default localhost scanning common model server ports
    pub fn localhost() -> Self {
        Self {
            name: "local".to_string(),
            address: "localhost".to_string(),
            ports: vec![1234, 8000, 8080, 11434],
        }
    }

    /// Create a remote host config
    pub fn remote(name: impl Into<String>, address: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            address: address.into(),
            ports: vec![1234, 8000, 8080, 11434],
        }
    }

    /// Set custom ports
    pub fn with_ports(mut self, ports: Vec<u16>) -> Self {
        self.ports = ports;
        self
    }
}

/// Model server discovery
pub struct AutoDiscovery {
    client: Client,
    config: DiscoveryConfig,
}

impl AutoDiscovery {
    /// Create a new discovery instance
    pub fn new(config: DiscoveryConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.probe_timeout_ms))
            .build()
            .unwrap_or_default();

        Self { client, config }
    }

    /// Scan all configured hosts and register discovered backends
    pub async fn discover(&self, registry: &ModelRegistry) -> Result<Vec<DiscoveryResult>> {
        let mut results = Vec::new();

        for host in &self.config.hosts {
            info!("Scanning host '{}' ({})", host.name, host.address);

            for &port in &host.ports {
                match self.probe_port(&host.address, port).await {
                    Some(server_type) => {
                        let backend_name = format!("{}-{}", host.name, server_type.name());
                        let base_url = format!("http://{}:{}", host.address, port);

                        info!(
                            "✓ Discovered {} at {}:{}",
                            server_type.name(), host.address, port
                        );

                        // Create and register the appropriate backend
                        let backend: Arc<dyn crate::ModelBackend> = match server_type {
                            ServerType::Ollama => {
                                Arc::new(backends::OllamaBackend::new(
                                    &backend_name,
                                    &base_url,
                                ))
                            }
                            ServerType::OpenAICompat(ref _name) => {
                                Arc::new(backends::OpenAICompatBackend::new(
                                    &backend_name,
                                    &base_url,
                                    None, // no API key for local servers
                                ))
                            }
                        };

                        if let Err(e) = registry.register(backend).await {
                            warn!("Failed to register discovered backend: {}", e);
                        }

                        results.push(DiscoveryResult {
                            host: host.name.clone(),
                            address: host.address.clone(),
                            port,
                            server_type: server_type.name().to_string(),
                            backend_name,
                        });
                    }
                    None => {
                        debug!("  No server at {}:{}", host.address, port);
                    }
                }
            }
        }

        if results.is_empty() {
            info!("No local model servers discovered");
        } else {
            info!("Discovered {} model server(s)", results.len());
        }

        Ok(results)
    }

    /// Probe a single port to determine what's running there
    async fn probe_port(&self, address: &str, port: u16) -> Option<ServerType> {
        // Try Ollama first (has a distinctive /api/tags endpoint)
        if let Some(st) = self.probe_ollama(address, port).await {
            return Some(st);
        }

        // Try OpenAI-compatible /v1/models
        if let Some(st) = self.probe_openai_compat(address, port).await {
            return Some(st);
        }

        None
    }

    /// Probe for Ollama server
    async fn probe_ollama(&self, address: &str, port: u16) -> Option<ServerType> {
        let url = format!("http://{}:{}/api/tags", address, port);
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                // Parse the response to confirm it's Ollama
                if let Ok(body) = resp.text().await {
                    if body.contains("models") {
                        return Some(ServerType::Ollama);
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Probe for OpenAI-compatible server (vLLM, LM Studio, etc.)
    async fn probe_openai_compat(&self, address: &str, port: u16) -> Option<ServerType> {
        let url = format!("http://{}:{}/v1/models", address, port);
        match self.client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                // Try to determine the server name
                let server_name = if port == 1234 {
                    "lm-studio".to_string()
                } else if port == 8000 {
                    "vllm".to_string()
                } else {
                    "openai-compat".to_string()
                };
                Some(ServerType::OpenAICompat(server_name))
            }
            _ => None,
        }
    }
}

/// Type of server discovered
#[derive(Debug, Clone)]
enum ServerType {
    Ollama,
    OpenAICompat(String), // name hint
}

impl ServerType {
    fn name(&self) -> &str {
        match self {
            ServerType::Ollama => "ollama",
            ServerType::OpenAICompat(name) => name,
        }
    }
}

/// Result of a single discovery probe
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    pub host: String,
    pub address: String,
    pub port: u16,
    pub server_type: String,
    pub backend_name: String,
}

impl std::fmt::Display for DiscoveryResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at {}:{} ({})",
            self.server_type, self.address, self.port, self.backend_name
        )
    }
}
