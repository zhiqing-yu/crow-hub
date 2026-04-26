//! Configuration management

use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::Result;

/// Main hub configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubConfig {
    /// Server configuration
    pub server: ServerConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Memory configuration
    pub memory: MemoryConfig,
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
    /// Adapter configurations
    pub adapters: HashMap<String, AdapterConfig>,
}

impl Default for HubConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            logging: LoggingConfig::default(),
            memory: MemoryConfig::default(),
            monitoring: MonitoringConfig::default(),
            adapters: HashMap::new(),
        }
    }
}

impl HubConfig {
    /// Load configuration from file and environment
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config: HubConfig = Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("AGENTHUB_"))
            .extract()
            .map_err(|e| crate::CoreError::Config(e.to_string()))?;
        
        Ok(config)
    }
    
    /// Load from default locations
    pub fn load_default() -> Result<Self> {
        // Try common config locations
        let paths = [
            "./agenthub.toml",
            "./config/agenthub.toml",
            "~/.config/agenthub/config.toml",
            "/etc/agenthub/config.toml",
        ];
        
        for path in &paths {
            let expanded = shellexpand::tilde(path);
            if std::path::Path::new(expanded.as_ref()).exists() {
                return Self::load(expanded.as_ref());
            }
        }
        
        // Return default if no config found
        Ok(Self::default())
    }
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Bind address
    pub bind: String,
    /// Port number
    pub port: u16,
    /// WebSocket port
    pub ws_port: u16,
    /// API key for authentication
    pub api_key: Option<String>,
    /// CORS origins
    pub cors_origins: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".to_string(),
            port: 8080,
            ws_port: 8081,
            api_key: None,
            cors_origins: vec!["*".to_string()],
        }
    }
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    pub level: String,
    /// Log format (json, pretty, compact)
    pub format: String,
    /// Log file path (optional)
    pub file: Option<String>,
    /// Enable console output
    pub console: bool,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            file: None,
            console: true,
        }
    }
}

/// Memory configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Memory backend type
    pub backend: String,
    /// Connection string or path
    pub connection: String,
    /// Embedding model
    pub embedding_model: String,
    /// Max memory entries per session
    pub max_entries: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: "sqlite".to_string(),
            connection: "./data/memory.db".to_string(),
            embedding_model: "local".to_string(),
            max_entries: 10000,
        }
    }
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Metrics export interval (seconds)
    pub export_interval: u64,
    /// Prometheus endpoint
    pub prometheus: Option<String>,
    /// Enable tracing
    pub tracing: bool,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            export_interval: 60,
            prometheus: Some("0.0.0.0:9090".to_string()),
            tracing: true,
        }
    }
}

/// Adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    /// Adapter type
    pub adapter_type: String,
    /// Whether adapter is enabled
    pub enabled: bool,
    /// Adapter-specific configuration
    #[serde(flatten)]
    pub config: HashMap<String, toml::Value>,
}

// Helper for shell expansion
mod shellexpand {
    pub fn tilde(path: &str) -> std::borrow::Cow<str> {
        if path.starts_with("~/") {
            if let Ok(home) = std::env::var("HOME") {
                return std::borrow::Cow::Owned(format!("{}{}", home, &path[1..]));
            }
            if let Ok(home) = std::env::var("USERPROFILE") {
                return std::borrow::Cow::Owned(format!("{}{}", home, &path[1..]));
            }
        }
        std::borrow::Cow::Borrowed(path)
    }
}
