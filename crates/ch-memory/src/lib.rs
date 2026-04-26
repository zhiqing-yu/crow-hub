//! Crow Hub Shared Memory Layer
//!
//! Provides a pluggable vector memory system for agents to share
//! context and knowledge across sessions.

use ch_protocol::MemoryEntry;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

pub mod backends;
pub mod embedder;

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
    async fn search(&self, query: &str, filter: MemoryFilter, top_k: usize) -> Result<Vec<MemoryEntry>>;
    
    /// Get memories for a session
    async fn get_session_context(&self, session_id: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
    
    /// Get memories for an agent
    async fn get_agent_memories(&self, agent_id: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
    
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
            _ => Err(MemoryError::Backend("Backend not yet implemented".to_string())),
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
