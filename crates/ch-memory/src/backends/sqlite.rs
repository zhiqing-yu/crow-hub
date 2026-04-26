//! SQLite Memory Backend
//!
//! Uses SQLite with sqlite-vec extension for vector storage

use crate::{MemoryStore, MemoryEntry, MemoryFilter, ExportFormat, ImportResult, SqliteConfig, Result, MemoryError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json;
use std::sync::Arc;
use tokio::sync::RwLock;

/// SQLite memory store
pub struct SqliteMemoryStore {
    config: SqliteConfig,
    // In a real implementation, this would hold a sqlx::SqlitePool
    // For now, we use a placeholder
    data: Arc<RwLock<Vec<MemoryEntry>>>,
}

impl SqliteMemoryStore {
    /// Create a new SQLite memory store
    pub async fn new(config: SqliteConfig) -> Result<Self> {
        // In real implementation:
        // 1. Connect to SQLite database
        // 2. Initialize vec0 virtual table
        // 3. Create indices
        
        Ok(Self {
            config,
            data: Arc::new(RwLock::new(Vec::new())),
        })
    }
    
    /// Generate embedding for content
    async fn embed(&self, content: &str) -> Result<Vec<f32>> {
        // In real implementation, this would call an embedding model
        // For now, return a dummy embedding
        let mut embedding = Vec::with_capacity(self.config.embedding_dim);
        let content_hash = content.len() as f32;
        for i in 0..self.config.embedding_dim {
            embedding.push((content_hash + i as f32) % 1.0);
        }
        Ok(embedding)
    }
    
    /// Calculate cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot_product / (norm_a * norm_b)
        }
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn init(&mut self) -> Result<()> {
        // Initialize database schema
        // CREATE VIRTUAL TABLE memories USING vec0(...)
        Ok(())
    }
    
    async fn write(&self, mut memory: MemoryEntry) -> Result<String> {
        let embedding = self.embed(&memory.content).await?;
        memory.embedding = Some(embedding);
        
        let mut data = self.data.write().await;
        data.push(memory.clone());
        
        Ok(memory.memory_id)
    }
    
    async fn read(&self, memory_id: &str) -> Result<MemoryEntry> {
        let data = self.data.read().await;
        data.iter()
            .find(|m| m.memory_id == memory_id)
            .cloned()
            .ok_or_else(|| MemoryError::NotFound(memory_id.to_string()))
    }
    
    async fn search(&self, query: &str, filter: MemoryFilter, top_k: usize) -> Result<Vec<MemoryEntry>> {
        let query_embedding = self.embed(query).await?;
        let data = self.data.read().await;
        
        let mut results: Vec<(MemoryEntry, f32)> = data
            .iter()
            .filter(|m| {
                // Apply filters
                if !filter.agent_ids.is_empty() && !filter.agent_ids.contains(&m.agent_id.to_string()) {
                    return false;
                }
                if !filter.session_ids.is_empty() && !filter.session_ids.contains(&m.session_id) {
                    return false;
                }
                if !filter.memory_types.is_empty() && !filter.memory_types.contains(&m.memory_type) {
                    return false;
                }
                true
            })
            .filter_map(|m| {
                // Calculate similarity
                m.embedding.as_ref().map(|emb| {
                    let similarity = Self::cosine_similarity(&query_embedding, emb);
                    (m.clone(), similarity)
                })
            })
            .collect();
        
        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Return top_k results
        Ok(results.into_iter().take(top_k).map(|(m, _)| m).collect())
    }
    
    async fn get_session_context(&self, session_id: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let data = self.data.read().await;
        let results: Vec<MemoryEntry> = data
            .iter()
            .filter(|m| m.session_id == session_id)
            .take(limit)
            .cloned()
            .collect();
        Ok(results)
    }
    
    async fn get_agent_memories(&self, agent_id: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let data = self.data.read().await;
        let results: Vec<MemoryEntry> = data
            .iter()
            .filter(|m| m.agent_id.to_string() == agent_id)
            .take(limit)
            .cloned()
            .collect();
        Ok(results)
    }
    
    async fn update(&self, memory_id: &str, content: &str) -> Result<()> {
        let mut data = self.data.write().await;
        if let Some(memory) = data.iter_mut().find(|m| m.memory_id == memory_id) {
            memory.content = content.to_string();
            memory.updated_at = Utc::now();
            Ok(())
        } else {
            Err(MemoryError::NotFound(memory_id.to_string()))
        }
    }
    
    async fn delete(&self, memory_id: &str) -> Result<()> {
        let mut data = self.data.write().await;
        let initial_len = data.len();
        data.retain(|m| m.memory_id != memory_id);
        
        if data.len() < initial_len {
            Ok(())
        } else {
            Err(MemoryError::NotFound(memory_id.to_string()))
        }
    }
    
    async fn export(&self, filter: MemoryFilter, format: ExportFormat) -> Result<Vec<u8>> {
        let data = self.data.read().await;
        
        let filtered: Vec<&MemoryEntry> = data
            .iter()
            .filter(|m| {
                if !filter.agent_ids.is_empty() && !filter.agent_ids.contains(&m.agent_id.to_string()) {
                    return false;
                }
                if !filter.session_ids.is_empty() && !filter.session_ids.contains(&m.session_id) {
                    return false;
                }
                true
            })
            .collect();
        
        match format {
            ExportFormat::Json => {
                serde_json::to_vec(&filtered)
                    .map_err(|e| MemoryError::Serialization(e.to_string()))
            }
            ExportFormat::Jsonl => {
                let mut output = Vec::new();
                for entry in filtered {
                    let line = serde_json::to_string(entry)
                        .map_err(|e| MemoryError::Serialization(e.to_string()))?;
                    output.extend_from_slice(line.as_bytes());
                    output.push(b'\n');
                }
                Ok(output)
            }
            _ => Err(MemoryError::Backend("Format not yet implemented".to_string())),
        }
    }
    
    async fn import(&self, data: &[u8], format: ExportFormat) -> Result<ImportResult> {
        match format {
            ExportFormat::Json => {
                let entries: Vec<MemoryEntry> = serde_json::from_slice(data)
                    .map_err(|e| MemoryError::Serialization(e.to_string()))?;
                
                let mut imported = 0;
                let mut errors = Vec::new();
                
                for entry in entries {
                    match self.write(entry).await {
                        Ok(_) => imported += 1,
                        Err(e) => errors.push(e.to_string()),
                    }
                }
                
                Ok(ImportResult {
                    imported,
                    failed: errors.len(),
                    errors,
                })
            }
            _ => Err(MemoryError::Backend("Format not yet implemented".to_string())),
        }
    }
    
    async fn count(&self) -> Result<usize> {
        let data = self.data.read().await;
        Ok(data.len())
    }
    
    async fn clear(&self) -> Result<()> {
        let mut data = self.data.write().await;
        data.clear();
        Ok(())
    }
    
    async fn close(&self) -> Result<()> {
        // Close database connection
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ch_protocol::{AgentId, MemoryEntry};

    #[tokio::test]
    async fn test_sqlite_store() {
        let config = SqliteConfig::default();
        let store = SqliteMemoryStore::new(config).await.unwrap();
        
        // Test write
        let memory = MemoryEntry {
            memory_id: "test-1".to_string(),
            agent_id: AgentId::new(),
            session_id: "session-1".to_string(),
            content: "Test memory content".to_string(),
            embedding: None,
            memory_type: "chat".to_string(),
            metadata: Default::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        let id = store.write(memory.clone()).await.unwrap();
        assert_eq!(id, memory.memory_id);
        
        // Test read
        let retrieved = store.read(&id).await.unwrap();
        assert_eq!(retrieved.content, memory.content);
        
        // Test count
        let count = store.count().await.unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((SqliteMemoryStore::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
        
        let c = vec![0.0, 1.0, 0.0];
        assert!(SqliteMemoryStore::cosine_similarity(&a, &c).abs() < 0.001);
    }
}
