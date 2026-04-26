//! Kimi Adapter
//!
//! Adapter for Moonshot AI's Kimi API

use crate::{AgentAdapter, AdapterConfig, AdapterError, Message, Response, Result, StreamChunk, Tool, UsageInfo, FinishReason};
use ch_protocol::{AgentStatus, AgentState, HealthStatus, Capability};
use async_trait::async_trait;
use reqwest::Client;
use futures::Stream;

/// Kimi API adapter
pub struct KimiAdapter {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    name: String,
}

impl KimiAdapter {
    /// Create a new Kimi adapter
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            api_key: String::new(),
            base_url: "https://api.moonshot.cn/v1".to_string(),
            model: "moonshot-v1-128k".to_string(),
            name: "kimi".to_string(),
        }
    }
}

impl Default for KimiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentAdapter for KimiAdapter {
    async fn init(&mut self, config: AdapterConfig) -> Result<()> {
        self.name = config.name;
        
        if let Some(api_key) = config.settings.get("api_key") {
            self.api_key = api_key.as_str().unwrap_or("").to_string();
        }
        
        if let Some(base_url) = config.settings.get("base_url") {
            self.base_url = base_url.as_str().unwrap_or(&self.base_url).to_string();
        }
        
        if let Some(model) = config.settings.get("model") {
            self.model = model.as_str().unwrap_or(&self.model).to_string();
        }
        
        Ok(())
    }
    
    async fn chat(&self, _messages: Vec<Message>, _tools: Option<Vec<Tool>>) -> Result<Response> {
        Ok(Response {
            content: "Kimi adapter placeholder response".to_string(),
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
            message: Some("Kimi adapter is healthy".to_string()),
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
                name: "long_context".to_string(),
                description: "Support for long context windows".to_string(),
                parameters: vec![],
                returns: Some("text".to_string()),
            },
        ]
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn adapter_type(&self) -> &str {
        "kimi"
    }
}
