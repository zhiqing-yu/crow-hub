//! Hermes Adapter
//!
//! Adapter for local Hermes agent

use crate::{AgentAdapter, AdapterConfig, AdapterError, Message, Response, Result, StreamChunk, Tool, UsageInfo, FinishReason};
use ch_protocol::{AgentStatus, AgentState, HealthStatus, Capability};
use async_trait::async_trait;
use reqwest::Client;
use futures::Stream;

/// Hermes local agent adapter
pub struct HermesAdapter {
    client: Client,
    endpoint: String,
    transport: String,
    name: String,
}

impl HermesAdapter {
    /// Create a new Hermes adapter
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            endpoint: "http://localhost:8080".to_string(),
            transport: "grpc".to_string(),
            name: "hermes".to_string(),
        }
    }
}

impl Default for HermesAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentAdapter for HermesAdapter {
    async fn init(&mut self, config: AdapterConfig) -> Result<()> {
        self.name = config.name;
        
        if let Some(endpoint) = config.settings.get("endpoint") {
            self.endpoint = endpoint.as_str().unwrap_or(&self.endpoint).to_string();
        }
        
        if let Some(transport) = config.settings.get("transport") {
            self.transport = transport.as_str().unwrap_or(&self.transport).to_string();
        }
        
        Ok(())
    }
    
    async fn chat(&self, _messages: Vec<Message>, _tools: Option<Vec<Tool>>) -> Result<Response> {
        Ok(Response {
            content: "Hermes adapter placeholder response".to_string(),
            tool_calls: vec![],
            usage: UsageInfo {
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
            },
            finish_reason: FinishReason::Stop,
        })
    }
    
    async fn stream(&self, _messages: Vec<Message>) -> Result<Box<dyn Stream<Item = StreamChunk> + Send + Unpin>> {
        Err(AdapterError::NotImplemented("Streaming not yet implemented".to_string()))
    }
    
    async fn status(&self) -> Result<AgentStatus> {
        Ok(AgentStatus {
            agent_id: ch_protocol::AgentId::new(),
            state: AgentState::Idle,
            current_task: None,
            queue_depth: 0,
            health: HealthStatus {
                healthy: true,
                last_check: chrono::Utc::now(),
                message: None,
            },
        })
    }
    
    async fn health_check(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            healthy: true,
            last_check: chrono::Utc::now(),
            message: Some("Hermes adapter is healthy".to_string()),
        })
    }
    
    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability {
                name: "chat".to_string(),
                description: "Chat completion".to_string(),
                parameters: vec![],
                returns: Some("text".to_string()),
            },
            Capability {
                name: "local_execution".to_string(),
                description: "Execute tasks locally".to_string(),
                parameters: vec![],
                returns: Some("result".to_string()),
            },
        ]
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn adapter_type(&self) -> &str {
        "hermes"
    }
}
