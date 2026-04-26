//! Crow Hub Communication Protocol
//! 
//! This crate defines the core message types and communication protocol
//! used for inter-agent communication in the Crow Hub system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub mod error;
pub mod types;

pub use error::{ProtocolError, Result};
pub use types::*;

/// Unique identifier for agents
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Agent address for routing messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAddress {
    pub agent_id: AgentId,
    pub agent_name: String,
    pub adapter_type: String,
}

impl AgentAddress {
    pub fn new(agent_name: impl Into<String>, adapter_type: impl Into<String>) -> Self {
        Self {
            agent_id: AgentId::new(),
            agent_name: agent_name.into(),
            adapter_type: adapter_type.into(),
        }
    }
}

/// Message types for inter-agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Task request from one agent to another
    TaskRequest,
    /// Task response
    TaskResponse,
    /// Task delegation
    TaskDelegate,
    /// Collaboration invitation
    CollabInvite,
    /// Collaboration acceptance
    CollabJoin,
    /// Memory sharing
    MemoryShare,
    /// Status heartbeat
    StatusHeartbeat,
    /// Metrics report
    StatusMetrics,
    /// Custom message type
    Custom(String),
}

/// Priority levels for messages
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// Core message structure for agent communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Unique message identifier
    pub message_id: Uuid,
    /// Timestamp when message was created
    pub timestamp: DateTime<Utc>,
    /// Correlation ID for request-response pairs
    pub correlation_id: Option<Uuid>,
    /// Sender address
    pub from: AgentAddress,
    /// Recipient address (None for broadcast)
    pub to: Option<AgentAddress>,
    /// Message type
    pub message_type: MessageType,
    /// Message payload
    pub payload: Payload,
    /// Session identifier
    pub session_id: String,
    /// Associated memory context IDs
    pub memory_context: Vec<String>,
    /// Message priority
    pub priority: Priority,
    /// Time-to-live in seconds
    pub ttl: Option<u32>,
}

impl AgentMessage {
    /// Create a new message
    pub fn new(
        from: AgentAddress,
        to: Option<AgentAddress>,
        message_type: MessageType,
        payload: Payload,
    ) -> Self {
        Self {
            message_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            correlation_id: None,
            from,
            to,
            message_type,
            payload,
            session_id: String::new(),
            memory_context: Vec::new(),
            priority: Priority::Normal,
            ttl: None,
        }
    }

    /// Set correlation ID for request-response pattern
    pub fn with_correlation(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Set session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = session_id.into();
        self
    }

    /// Set memory context
    pub fn with_memory_context(mut self, context: Vec<String>) -> Self {
        self.memory_context = context;
        self
    }

    /// Set priority
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Check if message is expired
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let elapsed = Utc::now().signed_duration_since(self.timestamp);
            elapsed.num_seconds() > ttl as i64
        } else {
            false
        }
    }
}

/// Message payload - can contain various types of data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Payload {
    /// Text content
    Text(String),
    /// Structured data
    Data(serde_json::Value),
    /// Task specification
    Task(TaskSpec),
    /// Task result
    Result(TaskResult),
    /// Status information
    Status(AgentStatus),
    /// Metrics data
    Metrics(MetricsData),
    /// Memory entry
    Memory(MemoryEntry),
    /// Empty payload
    Empty,
}

/// Task specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSpec {
    pub task_id: String,
    pub description: String,
    pub requirements: Vec<String>,
    pub deadline: Option<DateTime<Utc>>,
    pub dependencies: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Task result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub artifacts: Vec<Artifact>,
    pub metrics: TaskMetrics,
}

/// Artifact produced by task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub artifact_type: String,
    pub content: Vec<u8>,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Task execution metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetrics {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub tokens_used: u64,
    pub cost_usd: f64,
}

/// Agent status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: AgentId,
    pub state: AgentState,
    pub current_task: Option<String>,
    pub queue_depth: usize,
    pub health: HealthStatus,
}

/// Agent states
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Idle,
    Busy,
    Error,
    Offline,
}

/// Health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub last_check: DateTime<Utc>,
    pub message: Option<String>,
}

/// Metrics data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsData {
    pub agent_id: AgentId,
    pub timestamp: DateTime<Utc>,
    pub token_metrics: TokenMetrics,
    pub performance_metrics: PerformanceMetrics,
    pub resource_metrics: ResourceMetrics,
}

/// Token usage metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetrics {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub tokens_per_second: f64,
    pub cost_usd: f64,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub ttft_ms: u32,              // Time to first token
    pub throughput_tps: f64,       // Tokens per second
    pub latency_p50_ms: u32,
    pub latency_p99_ms: u32,
}

/// Resource metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetrics {
    pub cpu_usage_percent: f32,
    pub memory_usage_mb: u64,
    pub gpu_usage_percent: Option<f32>,
    pub gpu_memory_usage_mb: Option<u64>,
    pub kv_cache_usage: Option<f32>,
}

/// Memory entry for shared memory system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub memory_id: String,
    pub agent_id: AgentId,
    pub session_id: String,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub memory_type: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Protocol version
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Get protocol version info
pub fn version() -> &'static str {
    PROTOCOL_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_generation() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_message_creation() {
        let from = AgentAddress::new("test-agent", "test");
        let msg = AgentMessage::new(
            from.clone(),
            None,
            MessageType::StatusHeartbeat,
            Payload::Empty,
        );
        
        assert_eq!(msg.from.agent_name, "test-agent");
        assert!(msg.correlation_id.is_none());
    }

    #[test]
    fn test_message_expiration() {
        let from = AgentAddress::new("test", "test");
        let msg = AgentMessage {
            message_id: Uuid::new_v4(),
            timestamp: Utc::now() - chrono::Duration::seconds(100),
            correlation_id: None,
            from,
            to: None,
            message_type: MessageType::StatusHeartbeat,
            payload: Payload::Empty,
            session_id: String::new(),
            memory_context: Vec::new(),
            priority: Priority::Normal,
            ttl: Some(50), // 50 seconds
        };
        
        assert!(msg.is_expired());
    }
}
