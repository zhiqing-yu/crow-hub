//! Agent Registry
//!
//! Manages agent registration, discovery, and lifecycle

use ch_protocol::{AgentAddress, AgentId, AgentStatus, Capability};
use dashmap::DashMap;
use tracing::{debug, info};

use crate::Result;
use crate::CoreError;

/// Registered agent information
#[derive(Debug, Clone)]
pub struct RegisteredAgent {
    pub address: AgentAddress,
    pub status: AgentStatus,
    pub capabilities: Vec<Capability>,
    pub metadata: std::collections::HashMap<String, String>,
    pub registered_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

/// Agent registry
pub struct AgentRegistry {
    /// Registered agents
    agents: DashMap<AgentId, RegisteredAgent>,
    /// Name to ID mapping
    name_index: DashMap<String, AgentId>,
}

impl AgentRegistry {
    /// Create a new registry
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
            name_index: DashMap::new(),
        }
    }
    
    /// Register a new agent
    pub async fn register(
        &self,
        address: AgentAddress,
        capabilities: Vec<Capability>,
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<AgentId> {
        let agent_id = address.agent_id;
        
        // Check if name already exists
        if self.name_index.contains_key(&address.agent_name) {
            return Err(CoreError::Registry(
                format!("Agent with name '{}' already exists", address.agent_name)
            ));
        }
        
        let now = chrono::Utc::now();
        let agent = RegisteredAgent {
            address: address.clone(),
            status: AgentStatus {
                agent_id,
                state: ch_protocol::AgentState::Idle,
                current_task: None,
                queue_depth: 0,
                health: ch_protocol::HealthStatus {
                    healthy: true,
                    last_check: now,
                    message: None,
                },
            },
            capabilities,
            metadata,
            registered_at: now,
            last_heartbeat: now,
        };
        
        self.agents.insert(agent_id, agent);
        self.name_index.insert(address.agent_name.clone(), agent_id);
        
        info!("Agent '{}' registered with ID {}", address.agent_name, agent_id);
        Ok(agent_id)
    }
    
    /// Unregister an agent
    pub fn unregister(&self, agent_id: &AgentId) -> Result<()> {
        if let Some((_, agent)) = self.agents.remove(agent_id) {
            self.name_index.remove(&agent.address.agent_name);
            info!("Agent '{}' unregistered", agent.address.agent_name);
        }
        Ok(())
    }
    
    /// Get agent by ID
    pub fn get(&self, agent_id: &AgentId) -> Option<RegisteredAgent> {
        self.agents.get(agent_id).map(|a| a.clone())
    }
    
    /// Get agent by name
    pub fn get_by_name(&self, name: &str) -> Option<RegisteredAgent> {
        self.name_index
            .get(name)
            .and_then(|id| self.get(&id))
    }
    
    /// Update agent status
    pub fn update_status(&self, agent_id: &AgentId, status: AgentStatus) -> Result<()> {
        if let Some(mut agent) = self.agents.get_mut(agent_id) {
            agent.status = status;
            agent.last_heartbeat = chrono::Utc::now();
            Ok(())
        } else {
            Err(CoreError::Registry(format!("Agent {} not found", agent_id)))
        }
    }
    
    /// List all agents
    pub fn list_all(&self) -> Vec<RegisteredAgent> {
        self.agents.iter().map(|a| a.clone()).collect()
    }
    
    /// Find agents by capability
    pub fn find_by_capability(&self, capability_name: &str) -> Vec<RegisteredAgent> {
        self.agents
            .iter()
            .filter(|a| {
                a.capabilities.iter().any(|c| c.name == capability_name)
            })
            .map(|a| a.clone())
            .collect()
    }
    
    /// Get agent count
    pub fn count(&self) -> usize {
        self.agents.len()
    }
    
    /// Check if agent exists
    pub fn contains(&self, agent_id: &AgentId) -> bool {
        self.agents.contains_key(agent_id)
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
