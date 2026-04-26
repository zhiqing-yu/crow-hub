//! Crow Hub Core Engine
//!
//! This crate provides the core orchestration engine, message bus,
//! and session management for the Crow Hub system.

use std::sync::Arc;

pub mod bus;
pub mod channel;
pub mod config;
pub mod orchestrator;
pub mod registry;
pub mod session;

pub use bus::MessageBus;
pub use channel::{Channel, ChannelInfo, ChannelVisibility};
pub use config::HubConfig;
pub use orchestrator::Orchestrator;
pub use registry::AgentRegistry;
pub use session::SessionManager;

/// Result type for core operations
pub type Result<T> = std::result::Result<T, CoreError>;

/// Core error types
#[derive(thiserror::Error, Debug, Clone)]
pub enum CoreError {
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Registry error: {0}")]
    Registry(String),
    
    #[error("Session error: {0}")]
    Session(String),
    
    #[error("Bus error: {0}")]
    Bus(String),
    
    #[error("Orchestration error: {0}")]
    Orchestration(String),
    
    #[error("Channel error: {0}")]
    Channel(String),
    
    #[error("IO error: {0}")]
    Io(String),
}

/// Main Crow Hub instance
pub struct CrowHub {
    /// Configuration
    pub config: Arc<HubConfig>,
    /// Message bus for inter-agent communication
    pub bus: Arc<MessageBus>,
    /// Agent registry
    pub registry: Arc<AgentRegistry>,
    /// Session manager
    pub sessions: Arc<SessionManager>,
    /// Orchestrator
    pub orchestrator: Arc<Orchestrator>,
}

impl CrowHub {
    /// Create a new Crow Hub instance
    pub async fn new(config: HubConfig) -> Result<Self> {
        let config = Arc::new(config);
        
        // Initialize components
        let bus = Arc::new(MessageBus::new());
        let registry = Arc::new(AgentRegistry::new());
        let sessions = Arc::new(SessionManager::new());
        let orchestrator = Arc::new(Orchestrator::new(
            bus.clone(),
            registry.clone(),
            sessions.clone(),
        ));
        
        Ok(Self {
            config,
            bus,
            registry,
            sessions,
            orchestrator,
        })
    }
    
    /// Start the hub
    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting Crow Hub v{}", env!("CARGO_PKG_VERSION"));
        
        // Start message bus
        self.bus.start().await?;
        
        // Start orchestrator
        self.orchestrator.start().await?;
        
        tracing::info!("Crow Hub started successfully");
        Ok(())
    }
    
    /// Shutdown the hub
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down Crow Hub");
        
        self.orchestrator.shutdown().await?;
        self.bus.shutdown().await?;
        
        tracing::info!("Crow Hub shutdown complete");
        Ok(())
    }
}

/// Version information
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Get build information
pub fn build_info() -> BuildInfo {
    BuildInfo {
        version: version(),
        target: option_env!("TARGET").unwrap_or("unknown"),
        profile: if cfg!(debug_assertions) { "debug" } else { "release" },
        rustc: option_env!("RUSTC_VERSION").unwrap_or("unknown"),
    }
}

/// Build information
#[derive(Debug, Clone)]
pub struct BuildInfo {
    pub version: &'static str,
    pub target: &'static str,
    pub profile: &'static str,
    pub rustc: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version() {
        assert!(!version().is_empty());
    }
    
    #[test]
    fn test_build_info() {
        let info = build_info();
        assert!(!info.version.is_empty());
        assert!(!info.target.is_empty());
    }
}
