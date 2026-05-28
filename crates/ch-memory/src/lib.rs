//! Crow Hub Shared Memory Layer
//!
//! Provides a pluggable vector memory system for agents to share
//! context and knowledge across sessions.

use async_trait::async_trait;
use ch_protocol::{AgentId, MemoryEntry};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub mod backends;
pub mod embedder;
pub mod verifier;
pub mod writer;

/// Memory error types
#[derive(Error, Debug, Clone)]
pub enum MemoryError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    #[error("Backend error: {0}")]
    Backend(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Result type for memory operations
pub type Result<T> = std::result::Result<T, MemoryError>;

/// Memory filter for search operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryFilter {
    pub agent_ids: Vec<String>,
    pub session_ids: Vec<String>,
    pub memory_types: Vec<String>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl MemoryFilter {
    /// Create a new empty filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by agent ID
    pub fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_ids.push(agent_id.into());
        self
    }

    /// Filter by session ID
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_ids.push(session_id.into());
        self
    }

    /// Filter by memory type
    pub fn with_type(mut self, memory_type: impl Into<String>) -> Self {
        self.memory_types.push(memory_type.into());
        self
    }
}

/// Export format for memory
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Jsonl,
    Csv,
    Parquet,
}

/// Import result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub imported: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

// ─── Evidence (Maestro Task 2) ──────────────────────────────────────────────
//
// Evidence captures *auditable agent claims*: an agent says "I did X," the
// claim is stored with status=Pending, and a verifier agent (or human) can
// later flip it to Verified or Failed.  Together with Handoff Envelopes
// (shipped 2026-05-23), Evidence is the foundation of the "collaboration OS
// for agents" arc — Handoff = "what I'm passing along," Evidence = "what I
// claim I did + who can verify it."
//
// Stored in a separate `evidence` table (NOT the messages table) so the
// existing `MemoryEntry` schema stays untouched and the new write path can
// have its own indexes (by task_id, correlation_id, status).

/// Lifecycle of an evidence row.  Always starts at `Pending`; a successful
/// `verify()` transitions to `Verified`, a `fail()` transitions to `Failed`.
/// Once terminal (Verified|Failed), the row is immutable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Pending,
    Verified,
    Failed,
}

impl EvidenceStatus {
    /// String form used in SQL (`status` column).  Matches the snake_case
    /// serde form so JSON ↔ SQL round-trips are stable.
    pub fn as_str(&self) -> &'static str {
        match self {
            EvidenceStatus::Pending => "pending",
            EvidenceStatus::Verified => "verified",
            EvidenceStatus::Failed => "failed",
        }
    }

    /// Inverse of `as_str`.  Unknown strings default to `Pending` rather
    /// than failing — this keeps the system tolerant of forward-compat
    /// rows written by a future version with new statuses.
    pub fn from_str(s: &str) -> Self {
        match s {
            "verified" => EvidenceStatus::Verified,
            "failed" => EvidenceStatus::Failed,
            _ => EvidenceStatus::Pending,
        }
    }
}

/// One row in the `evidence` table.  Created on `write_evidence`; mutated
/// only by `verify()` or `fail()` (which set `status`, `verified_at`,
/// `verified_by`, and optionally the `witness`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRow {
    /// Unique row ID (UUID stringified).
    pub id: String,
    /// Logical task this evidence belongs to.  Multiple evidence rows can
    /// share a task_id when a single task produces several claims.
    pub task_id: String,
    /// Optional bus correlation_id linking this evidence back to the
    /// originating request/response cycle.
    pub correlation_id: Option<String>,
    /// Agent that submitted the claim.
    pub agent_id: AgentId,
    /// Human-readable agent name at submission time (cached for display).
    pub agent_name: String,
    /// The claim itself, free-form.
    pub claim: String,
    /// Lifecycle state.
    pub status: EvidenceStatus,
    /// Optional pointer to supporting material (URL, file path, hash, etc.).
    pub witness: Option<String>,
    /// Free-form JSON sidecar for tool-specific extension data.
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// When the claim was submitted.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the row transitioned to a terminal status (Verified|Failed).
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Who performed the verification (agent name or "human:<name>").
    pub verified_by: Option<String>,
}

/// Storage trait for evidence rows.  Implemented by `SqliteMemoryStore`
/// alongside `MemoryStore` so a single store handle can persist both
/// regular messages and evidence.
#[async_trait]
pub trait EvidenceStore: Send + Sync {
    /// Persist a new evidence row.  `row.status` SHOULD be `Pending` at
    /// write time; implementations may accept others for replay/import but
    /// the normal write path is always Pending.
    async fn write_evidence(&self, row: EvidenceRow) -> Result<()>;

    /// Transition an evidence row from Pending → Verified.  Stamps
    /// `verified_at` with the current time and `verified_by = by`.
    /// Optionally overwrites `witness`.  Returns `NotFound` if `id`
    /// doesn't exist; behavior on an already-terminal row is
    /// implementation-defined (sqlite backend treats it as a no-op).
    async fn verify(&self, id: &str, by: &str, witness: Option<String>) -> Result<()>;

    /// Transition Pending → Failed.  Stores `reason` in the `metadata`
    /// JSON under key `failure_reason`.
    async fn fail(&self, id: &str, by: &str, reason: String) -> Result<()>;

    /// All evidence rows for a task, oldest first (created_at ASC).
    async fn by_task(&self, task_id: &str) -> Result<Vec<EvidenceRow>>;

    /// Up to `limit` evidence rows still in Pending status, oldest first.
    /// Convenient for a verifier agent's poll loop.  Equivalent to
    /// `by_status(EvidenceStatus::Pending, limit)`; kept as a named
    /// shortcut because it's the common case.
    async fn pending(&self, limit: usize) -> Result<Vec<EvidenceRow>>;

    /// Up to `limit` evidence rows matching the given status, oldest first.
    /// Used by `crow memory evidence --status verified|failed|pending`.
    async fn by_status(&self, status: EvidenceStatus, limit: usize) -> Result<Vec<EvidenceRow>>;
}

/// Memory store trait - all backends must implement this
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Initialize the store
    async fn init(&mut self) -> Result<()>;

    /// Write a memory entry
    async fn write(&self, memory: MemoryEntry) -> Result<String>;

    /// Read a memory entry by ID
    async fn read(&self, memory_id: &str) -> Result<MemoryEntry>;

    /// Search memories by semantic similarity
    async fn search(
        &self,
        query: &str,
        filter: MemoryFilter,
        top_k: usize,
    ) -> Result<Vec<MemoryEntry>>;

    /// Get memories for a session
    async fn get_session_context(&self, session_id: &str, limit: usize)
        -> Result<Vec<MemoryEntry>>;

    /// Get memories for an agent
    async fn get_agent_memories(&self, agent_id: &str, limit: usize) -> Result<Vec<MemoryEntry>>;

    /// Get recent memories from a channel
    async fn recent(&self, channel: &str, limit: usize) -> Result<Vec<MemoryEntry>>;

    /// Get memories by correlation ID
    async fn by_correlation(&self, id: &str) -> Result<Vec<MemoryEntry>>;

    /// Update a memory entry
    async fn update(&self, memory_id: &str, content: &str) -> Result<()>;

    /// Delete a memory entry
    async fn delete(&self, memory_id: &str) -> Result<()>;

    /// Export memories
    async fn export(&self, filter: MemoryFilter, format: ExportFormat) -> Result<Vec<u8>>;

    /// Import memories
    async fn import(&self, data: &[u8], format: ExportFormat) -> Result<ImportResult>;

    /// Get memory count
    async fn count(&self) -> Result<usize>;

    /// Clear all memories
    async fn clear(&self) -> Result<()>;

    /// Close the store
    async fn close(&self) -> Result<()>;
}

/// Memory backend type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryBackend {
    Sqlite(SqliteConfig),
    Chroma(ChromaConfig),
    Qdrant(QdrantConfig),
    Milvus(MilvusConfig),
    PgVector(PgVectorConfig),
}

/// SQLite backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    pub path: String,
    pub embedding_dim: usize,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            path: "./data/memory.db".to_string(),
            embedding_dim: 768,
        }
    }
}

/// Chroma backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromaConfig {
    pub host: String,
    pub port: u16,
    pub collection: String,
}

impl Default for ChromaConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8000,
            collection: "agenthub".to_string(),
        }
    }
}

/// Qdrant backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantConfig {
    pub url: String,
    pub api_key: Option<String>,
    pub collection: String,
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            url: "http://localhost:6333".to_string(),
            api_key: None,
            collection: "agenthub".to_string(),
        }
    }
}

/// Milvus backend configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilvusConfig {
    pub host: String,
    pub port: u16,
    pub collection: String,
}

/// PostgreSQL pgvector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgVectorConfig {
    pub connection_string: String,
    pub table: String,
}

/// Memory manager for handling multiple stores
pub struct MemoryManager {
    stores: HashMap<String, Box<dyn MemoryStore>>,
    default_store: String,
}

impl MemoryManager {
    /// Create a new memory manager
    pub fn new() -> Self {
        Self {
            stores: HashMap::new(),
            default_store: "default".to_string(),
        }
    }

    /// Register a store
    pub fn register(&mut self, name: String, store: Box<dyn MemoryStore>) {
        self.stores.insert(name, store);
    }

    /// Get a store by name
    pub fn get(&self, name: &str) -> Option<&dyn MemoryStore> {
        self.stores.get(name).map(|s| s.as_ref())
    }

    /// Get the default store
    pub fn default_store(&self) -> Option<&dyn MemoryStore> {
        self.stores.get(&self.default_store).map(|s| s.as_ref())
    }

    /// Set the default store
    pub fn set_default(&mut self, name: &str) {
        if self.stores.contains_key(name) {
            self.default_store = name.to_string();
        }
    }

    /// Create a store from backend configuration
    pub async fn create_store(backend: MemoryBackend) -> Result<Box<dyn MemoryStore>> {
        match backend {
            MemoryBackend::Sqlite(config) => {
                let store = backends::sqlite::SqliteMemoryStore::new(config).await?;
                Ok(Box::new(store))
            }
            _ => Err(MemoryError::Backend(
                "Backend not yet implemented".to_string(),
            )),
        }
    }
}

impl Default for MemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_filter() {
        let filter = MemoryFilter::new()
            .with_agent("agent-1")
            .with_session("session-1")
            .with_type("chat");

        assert_eq!(filter.agent_ids, vec!["agent-1"]);
        assert_eq!(filter.session_ids, vec!["session-1"]);
        assert_eq!(filter.memory_types, vec!["chat"]);
    }

    #[test]
    fn test_memory_backend_config() {
        let sqlite = SqliteConfig::default();
        assert_eq!(sqlite.embedding_dim, 768);

        let chroma = ChromaConfig::default();
        assert_eq!(chroma.port, 8000);
    }
}

// ─── Workflow steps (Maestro Task 3) ────────────────────────────────────────
//
// Storage for the workflow step lifecycle.  Companion to the bus protocol
// types in `ch_protocol` (`WorkflowStepState`, `WorkflowClaimMsg`).  Steps
// transition `Pending → Claimed` on a `claim_step` call (the only transition
// wired in slice 1 — others land in subsequent narrow PRs per the brainstorm
// at docs/journals/2026-05-26_workflow_design_brainstorm.md).
//
// Sed-driven scaffolding earlier in this session left TWO copies of the
// struct + trait here; this is the surviving one, using the fully-qualified
// `ch_protocol::WorkflowStepState` so we don't need to update the file's
// import block.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepRow {
    pub step_id: String,
    pub workflow_id: String,
    pub state: ch_protocol::WorkflowStepState,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait WorkflowStore: Send + Sync {
    async fn claim_step(&self, workflow_id: &str, step_id: &str, agent_id: &str) -> Result<()>;
    async fn by_workflow(&self, workflow_id: &str) -> Result<Vec<WorkflowStepRow>>;
    async fn pending_steps(&self, limit: usize) -> Result<Vec<WorkflowStepRow>>;
}
