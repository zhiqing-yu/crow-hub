//! Crow Hub Adapter System
//!
//! Provides a unified interface for connecting different AI agents
//! to the Crow Hub system.

use ch_protocol::{AgentMessage, AgentStatus, Capability, HealthStatus};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub mod adapters;

/// Adapter error types
#[derive(Error, Debug, Clone)]
pub enum AdapterError {
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Authentication error: {0}")]
    Authentication(String),
    
    #[error("Request error: {0}")]
    Request(String),
    
    #[error("Response error: {0}")]
    Response(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
    
    #[error("Timeout after {0}s")]
    Timeout(u64),
    
    #[error("Not implemented: {0}")]
    NotImplemented(String),
    
    #[error("Unknown adapter type: {0}")]
    UnknownAdapter(String),
}

/// Result type for adapter operations
pub type Result<T> = std::result::Result<T, AdapterError>;

/// Adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    pub adapter_type: String,
    pub name: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub settings: HashMap<String, serde_json::Value>,
}

/// Message for agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Message roles
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Tool call from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Response from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: UsageInfo,
    pub finish_reason: FinishReason,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

/// Finish reason for response
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
    Error,
}

/// Stream chunk for streaming responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub content: String,
    pub is_finished: bool,
    pub finish_reason: Option<FinishReason>,
}

/// Core adapter trait - all agent adapters must implement this
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    /// Initialize the adapter with configuration
    async fn init(&mut self, config: AdapterConfig) -> Result<()>;
    
    /// Send a chat message and get response
    async fn chat(&self, messages: Vec<Message>, tools: Option<Vec<Tool>>) -> Result<Response>;
    
    /// Stream a response
    async fn stream(&self, messages: Vec<Message>) -> Result<Box<dyn Stream<Item = StreamChunk> + Send + Unpin>>;
    
    /// Get adapter status
    async fn status(&self) -> Result<AgentStatus>;
    
    /// Perform health check
    async fn health_check(&self) -> Result<HealthStatus>;
    
    /// Get adapter capabilities
    fn capabilities(&self) -> Vec<Capability>;
    
    /// Get adapter name
    fn name(&self) -> &str;
    
    /// Get adapter type
    fn adapter_type(&self) -> &str;
}

use futures::Stream;

/// Adapter factory for creating adapter instances
pub struct AdapterFactory;

impl AdapterFactory {
    /// Create an adapter by type name
    pub fn create(adapter_type: &str) -> Result<Box<dyn AgentAdapter>> {
        match adapter_type {
            "claude" => Ok(Box::new(adapters::ClaudeAdapter::new())),
            "kimi" => Ok(Box::new(adapters::KimiAdapter::new())),
            "gemini" => Ok(Box::new(adapters::GeminiAdapter::new())),
            "hermes" => Ok(Box::new(adapters::HermesAdapter::new())),
            "codebuddy" => Ok(Box::new(adapters::CodeBuddyAdapter::new())),
            _ => Err(AdapterError::UnknownAdapter(adapter_type.to_string())),
        }
    }
    
    /// List available adapter types
    pub fn available_adapters() -> Vec<&'static str> {
        vec!["claude", "kimi", "gemini", "hermes", "codebuddy"]
    }
}

/// Adapter registry for managing multiple adapters
pub struct AdapterRegistry {
    adapters: HashMap<String, Box<dyn AgentAdapter>>,
}

impl AdapterRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }
    
    /// Register an adapter
    pub fn register(&mut self, name: String, adapter: Box<dyn AgentAdapter>) {
        self.adapters.insert(name, adapter);
    }
    
    /// Get an adapter by name
    pub fn get(&self, name: &str) -> Option<&dyn AgentAdapter> {
        self.adapters.get(name).map(|a| a.as_ref())
    }
    
    /// Get mutable adapter by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Box<dyn AgentAdapter>> {
        self.adapters.get_mut(name)
    }
    
    /// Remove an adapter
    pub fn remove(&mut self, name: &str) -> Option<Box<dyn AgentAdapter>> {
        self.adapters.remove(name)
    }
    
    /// List all registered adapters
    pub fn list(&self) -> Vec<&str> {
        self.adapters.keys().map(|k| k.as_str()).collect()
    }
    
    /// Check if adapter exists
    pub fn contains(&self, name: &str) -> bool {
        self.adapters.contains_key(name)
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_factory() {
        let available = AdapterFactory::available_adapters();
        assert!(available.contains(&"claude"));
        assert!(available.contains(&"kimi"));
    }

    #[test]
    fn test_adapter_registry() {
        let mut registry = AdapterRegistry::new();
        assert!(registry.list().is_empty());
        
        // Note: We can't actually register without a real adapter instance
        // This is just a structural test
    }
}
