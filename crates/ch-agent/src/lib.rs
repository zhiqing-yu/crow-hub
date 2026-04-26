//! Crow Hub Agent Plugin System
//!
//! Provides a TOML-manifest-based plugin system for connecting
//! different AI agents (Claude, Gemini, Ollama-based models, CLI tools, etc.)
//! to the Crow Hub communication layer.
//!
//! Three driver types handle different agent connection methods:
//! - **APIDriver**: For agents accessible via HTTP APIs (cloud or local model servers)
//! - **SubprocessDriver**: For CLI agents (claude code, hermes, etc.) via stdin/stdout
//! - **MCPDriver**: For MCP-compatible desktop apps (future)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod drivers;
pub mod loader;
pub mod manifest;
pub mod runtime;
pub mod scanner;

pub use loader::PluginLoader;
pub use manifest::AgentManifest;
pub use runtime::AgentRuntime;
pub use scanner::EnvironmentScanner;

// ── Error types ──────────────────────────────────────────────

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Manifest error: {0}")]
    Manifest(String),

    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Driver error: {0}")]
    Driver(String),

    #[error("Lifecycle error: {0}")]
    Lifecycle(String),

    #[error("Communication error: {0}")]
    Communication(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
}

pub type Result<T> = std::result::Result<T, AgentError>;

// ── Agent state ──────────────────────────────────────────────

/// Lifecycle state of an agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Discovered but not yet initialized
    Discovered,
    /// Manifest loaded, initializing
    Loading,
    /// Ready to receive messages
    Ready,
    /// Actively processing
    Running,
    /// Temporarily stopped
    Stopped,
    /// Error state
    Error,
}

/// Runtime info about a loaded agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent name from manifest
    pub name: String,
    /// Current state
    pub state: AgentState,
    /// Driver type
    pub driver_type: String,
    /// Which model backend this agent uses (if any)
    pub model_backend: Option<String>,
    /// Default model
    pub default_model: Option<String>,
    /// When the agent was loaded
    pub loaded_at: DateTime<Utc>,
    /// Channels this agent is in
    pub channels: Vec<String>,
    /// Description from manifest
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_state_serialization() {
        let state = AgentState::Running;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"running\"");

        let deserialized: AgentState = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, AgentState::Running);
    }
}
