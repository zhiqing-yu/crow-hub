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
    /// Structured task handoff between agents (summary + decisions + open
    /// questions + continuation hint).  Inspired by Maestro's lifecycle
    /// envelopes — see `docs/journals/2026-05-19_brainstorming_maestro_lessons.md`.
    Handoff,
    /// Agent claim that a task has been completed — written to the
    /// `evidence` table with `status = pending` until a verifier flips it.
    /// Maestro Task 2.
    Evidence,
    /// Verifier flips an existing evidence row to `verified` or `failed`.
    /// Maestro Task 2.
    WorkflowClaim,
    EvidenceVerify,
    /// Custom message type
    /// Agent claims a workflow step (Pending -> Claimed)
    WorkflowClaim,
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
    /// Optional per-message model override
    #[serde(default)]
    pub model_override: Option<String>,
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
            model_override: None,
        }
    }

    /// Set model override for this message
    pub fn with_model_override(mut self, model: String) -> Self {
        self.model_override = Some(model);
        self
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
    /// Structured task handoff between agents
    Handoff(HandoffEnvelope),
    /// Agent's auditable claim that a task has been completed.  Maestro
    /// Task 2.  Persisted by the memory writer to the `evidence` table.
    Evidence(EvidenceClaim),
    /// Verifier flips a previously-emitted evidence row to verified or
    /// failed.  Maestro Task 2.
    EvidenceVerify(EvidenceVerifyMsg),
    WorkflowClaim(WorkflowClaimMsg),
    /// Empty payload
    WorkflowClaim(WorkflowClaimMsg),
    Empty,
}

/// Structured handoff envelope.
///
/// When an agent finishes a task (or wants to delegate one), it emits a
/// `Handoff` envelope rather than a free-form `Text` reply.  The envelope
/// captures the four things the next agent in the chain needs to pick up
/// the work without re-reading the entire chat scrollback:
///
/// * `summary` — one-line plain-English description of what was done
/// * `decisions` — short bullets the next agent should know about
/// * `open_questions` — what still needs to be answered
/// * `continuation_hint` — a concrete pointer to where to pick up
///
/// Inspired by Maestro's lifecycle envelopes (see the brainstorming doc
/// at `docs/journals/2026-05-19_brainstorming_maestro_lessons.md`).
///
/// Persisted by the memory writer with `memory_type = "handoff"` and the
/// envelope JSON-serialised into the existing `content` column — no
/// schema migration needed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HandoffEnvelope {
    /// Name of the agent emitting the handoff (e.g. "claude-wsl-ubuntu", "You")
    pub from_agent: String,
    /// One-line summary of what was accomplished
    pub summary: String,
    /// Key decisions made during the task
    #[serde(default)]
    pub decisions: Vec<String>,
    /// Open questions for the next agent to resolve
    #[serde(default)]
    pub open_questions: Vec<String>,
    /// Hint for where the next agent should pick up (e.g. "see PR #42", "step 3")
    #[serde(default)]
    pub continuation_hint: String,
}

/// Bus message payload for an agent submitting an evidence claim — "I did
/// X for task Y, witness is Z."  The memory writer persists this to the
/// `evidence` table with `status = 'pending'`.  Maestro Task 2.
///
/// Symmetric with `HandoffEnvelope`: additive variant, snake_case serde,
/// backward-compat preserved (no existing field changes).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvidenceClaim {
    /// Logical task this claim belongs to.  Multiple claims can share a
    /// task_id when a single task produces several pieces of evidence.
    pub task_id: String,
    /// The claim itself, free-form (e.g. "built auth module", "deployed
    /// to staging", "verified migration is safe").
    pub claim: String,
    /// Optional pointer to supporting material — URL, file path, commit
    /// hash, etc.  Verifier agents can chase this to validate the claim.
    #[serde(default)]
    pub witness: Option<String>,
}

/// Bus message payload for a verifier flipping an existing evidence row.
/// `outcome = true` → verified, `false` → failed (note carries the reason
/// when failing).  Maestro Task 2.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvidenceVerifyMsg {
    /// ID of the evidence row to update (matches `EvidenceClaim` row id).
    pub evidence_id: String,
    /// `true` → verify, `false` → fail.
    pub outcome: bool,
    /// Optional note — required-ish for fail outcomes (stored as
    /// `failure_reason` in the row's metadata JSON), informational for
    /// verify outcomes.
    #[serde(default)]
    pub note: Option<String>,
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
    pub ttft_ms: u32,        // Time to first token
    pub throughput_tps: f64, // Tokens per second
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
    WorkflowClaim(WorkflowClaimMsg),
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
    WorkflowClaim(WorkflowClaimMsg),
            payload: Payload::Empty,
            session_id: String::new(),
            memory_context: Vec::new(),
            priority: Priority::Normal,
            ttl: Some(50), // 50 seconds
            model_override: None,
        };

        assert!(msg.is_expired());
    }

    // ── Handoff envelope ─────────────────────────────────────

    #[test]
    fn handoff_envelope_serde_round_trip() {
        let env = HandoffEnvelope {
            from_agent: "claude-wsl-ubuntu".to_string(),
            summary: "Refactored auth module".to_string(),
            decisions: vec!["use JWT not sessions".to_string()],
            open_questions: vec!["add password reset?".to_string()],
            continuation_hint: "see PR #42".to_string(),
        };
        let json = serde_json::to_string(&env).unwrap();
        let back: HandoffEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(back.from_agent, "claude-wsl-ubuntu");
        assert_eq!(back.summary, "Refactored auth module");
        assert_eq!(back.decisions.len(), 1);
        assert_eq!(back.open_questions.len(), 1);
        assert_eq!(back.continuation_hint, "see PR #42");
    }

    #[test]
    fn handoff_envelope_serde_with_only_summary() {
        // Minimal envelope as the /handoff slash command would emit it —
        // summary only, other vec/string fields default-empty.  Defaults
        // must come through round-trip cleanly.
        let env = HandoffEnvelope {
            from_agent: "You".to_string(),
            summary: "test".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&env).unwrap();
        let back: HandoffEnvelope = serde_json::from_str(&json).unwrap();
        assert!(back.decisions.is_empty());
        assert!(back.open_questions.is_empty());
        assert_eq!(back.continuation_hint, "");
    }

    #[test]
    fn message_type_handoff_serializes_snake_case() {
        // serde rename_all = "snake_case" → Handoff becomes "handoff".
        // Memory writer keys off this lowercase string when storing.
        let json = serde_json::to_string(&MessageType::Handoff).unwrap();
        assert_eq!(json, "\"handoff\"");
    }

    #[test]
    fn payload_handoff_round_trip_in_message() {
        let env = HandoffEnvelope {
            from_agent: "You".to_string(),
            summary: "ship it".to_string(),
            ..Default::default()
        };
        let from = AgentAddress::new("You", "tui");
        let msg = AgentMessage::new(from, None, MessageType::Handoff, Payload::Handoff(env));
        let json = serde_json::to_string(&msg).unwrap();
        let back: AgentMessage = serde_json::from_str(&json).unwrap();
        if let Payload::Handoff(e) = back.payload {
            assert_eq!(e.summary, "ship it");
        } else {
            panic!("expected Payload::Handoff after round-trip");
        }
    }

    #[test]
    fn old_message_without_model_override_still_parses() {
        // Backward compat: a message JSON missing the model_override field
        // (e.g. from a cached row written before b671d40) must still parse.
        let json = r#"{
            "message_id": "00000000-0000-0000-0000-000000000001",
            "timestamp": "2026-05-22T00:00:00Z",
            "correlation_id": null,
            "from": {"agent_id": "00000000-0000-0000-0000-000000000002", "agent_name": "x", "adapter_type": "y"},
            "to": null,
            "message_type": "task_request",
            "payload": {"text": "hi"},
            "session_id": "",
            "memory_context": [],
            "priority": "normal",
            "ttl": null
        }"#;
        let msg: AgentMessage = serde_json::from_str(json).unwrap();
        assert!(msg.model_override.is_none());
    }

    // ── Evidence (Maestro Task 2) ─────────────────────────────────────────

    #[test]
    fn message_type_evidence_variants_serialize_snake_case() {
        // Memory writer keys off these lowercase strings to dispatch.
        assert_eq!(
            serde_json::to_string(&MessageType::Evidence).unwrap(),
            "\"evidence\""
        );
        assert_eq!(
            serde_json::to_string(&MessageType::EvidenceVerify).unwrap(),
            "\"evidence_verify\""
        );
    }

    #[test]
    fn payload_evidence_round_trip_in_message() {
        let claim = EvidenceClaim {
            task_id: "task-42".to_string(),
            claim: "built auth module".to_string(),
            witness: Some("PR #42".to_string()),
        };
        let from = AgentAddress::new("claude", "tui");
        let msg = AgentMessage::new(from, None, MessageType::Evidence, Payload::Evidence(claim));
        let json = serde_json::to_string(&msg).unwrap();
        let back: AgentMessage = serde_json::from_str(&json).unwrap();
        if let Payload::Evidence(c) = back.payload {
            assert_eq!(c.task_id, "task-42");
            assert_eq!(c.claim, "built auth module");
            assert_eq!(c.witness.as_deref(), Some("PR #42"));
        } else {
            panic!("expected Payload::Evidence after round-trip");
        }
    }

    #[test]
    fn payload_evidence_verify_round_trip_in_message() {
        // Both outcomes — verify (true) and fail (false) — must round-trip.
        for (outcome, note) in [(true, None), (false, Some("rollback".to_string()))] {
            let v = EvidenceVerifyMsg {
                evidence_id: "ev-1".to_string(),
                outcome,
                note: note.clone(),
            };
            let from = AgentAddress::new("verifier", "api");
            let msg = AgentMessage::new(
                from,
                None,
                MessageType::EvidenceVerify,
                Payload::EvidenceVerify(v),
            );
            let json = serde_json::to_string(&msg).unwrap();
            let back: AgentMessage = serde_json::from_str(&json).unwrap();
            if let Payload::EvidenceVerify(v) = back.payload {
                assert_eq!(v.evidence_id, "ev-1");
                assert_eq!(v.outcome, outcome);
                assert_eq!(v.note, note);
            } else {
                panic!("expected Payload::EvidenceVerify after round-trip");
            }
        }
    }

    #[test]
    fn evidence_claim_serde_with_only_required_fields() {
        // Minimal EvidenceClaim — task_id + claim only, no witness.  The
        // /evidence slash command emits this shape; old DB rows / cached
        // messages without `witness` must still parse.
        let json = r#"{"task_id":"t","claim":"c"}"#;
        let back: EvidenceClaim = serde_json::from_str(json).unwrap();
        assert_eq!(back.task_id, "t");
#[derive(Debug, Clone, Serialize, Deserialize)]
        assert_eq!(back.claim, "c");
        assert!(back.witness.is_none());
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowClaimMsg {
    pub step_id: String,
    pub workflow_id: String,
    pub agent_id: String,
}
pub struct WorkflowClaimMsg { pub step_id: String, pub workflow_id: String, pub agent_id: String }
