use ch_protocol::MemoryEntry;
use ch_memory::{MemoryStore, MemoryFilter, ExportFormat, ImportResult, SqliteConfig, Result, MemoryError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{sqlite::{SqliteConnectOptions, SqlitePoolOptions}, SqlitePool, Row};
use std::str::FromStr;
use ch_protocol::AgentId;
use uuid::Uuid;
use std::collections::HashMap;

pub struct SqliteMemoryStore {
    config: SqliteConfig,
    pool: SqlitePool,
}

impl SqliteMemoryStore {
    pub async fn new(config: SqliteConfig) -> Result<Self> {
        let path = std::env::var("CROW_HUB_MEMORY_PATH").unwrap_or_else(|_| config.path.clone());
        if path != ":memory:" {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
        }

        let conn_str = if path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite://{}", path)
        };

        let options = SqliteConnectOptions::from_str(&conn_str)
            .map_err(|e| MemoryError::Backend(e.to_string()))?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        let mut store = Self { config, pool };
        store.init().await?;
        Ok(store)
    }

    fn row_to_memory(row: sqlx::sqlite::SqliteRow) -> Result<MemoryEntry> {
        let metadata_str: Option<String> = row.try_get("metadata").unwrap_or(None);
        let metadata = metadata_str
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        
        let agent_id_str: String = row.try_get("from_agent").unwrap_or_default();
        let agent_id = AgentId(Uuid::parse_str(&agent_id_str).unwrap_or_else(|_| Uuid::new_v4()));

        let created_at: i64 = row.try_get("created_at").unwrap_or(0);
        let timestamp = DateTime::from_timestamp(created_at, 0).unwrap_or_default();

        Ok(MemoryEntry {
            memory_id: row.try_get("message_id").unwrap_or_default(),
            agent_id,
            session_id: row.try_get("channel").unwrap_or_default(),
            content: row.try_get("content").unwrap_or_default(),
            embedding: None,
            memory_type: row.try_get("message_type").unwrap_or_default(),
            metadata,
            created_at: timestamp,
            updated_at: timestamp,
        })
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn init(&mut self) -> Result<()> {
        let schema = "
        CREATE TABLE IF NOT EXISTS messages (
            message_id      TEXT PRIMARY KEY,
            correlation_id  TEXT,
            from_agent      TEXT NOT NULL,
            to_agent        TEXT,
            channel         TEXT,
            message_type    TEXT NOT NULL,
            content         TEXT NOT NULL,
            embedding       BLOB,
            metadata        TEXT,
            created_at      INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_messages_correlation ON messages(correlation_id);
        CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);
        CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel);
        ";
        sqlx::query(schema)
            .execute(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn write(&self, memory: MemoryEntry) -> Result<String> {
        let correlation_id = memory.metadata.get("correlation_id").and_then(|v| v.as_str());
        let to_agent = memory.metadata.get("to_agent").and_then(|v| v.as_str());
        let channel = &memory.session_id;
        let metadata_str = serde_json::to_string(&memory.metadata).unwrap_or_default();

        sqlx::query(
            "INSERT OR REPLACE INTO messages 
            (message_id, correlation_id, from_agent, to_agent, channel, message_type, content, metadata, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&memory.memory_id)
        .bind(correlation_id)
        .bind(memory.agent_id.to_string())
        .bind(to_agent)
        .bind(channel)
        .bind(&memory.memory_type)
        .bind(&memory.content)
        .bind(&metadata_str)
        .bind(memory.created_at.timestamp())
        .execute(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;

        Ok(memory.memory_id)
    }

    async fn read(&self, memory_id: &str) -> Result<MemoryEntry> {
        let row = sqlx::query("SELECT * FROM messages WHERE message_id = ?")
            .bind(memory_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        if let Some(row) = row {
            Self::row_to_memory(row)
        } else {
            Err(MemoryError::NotFound(memory_id.to_string()))
        }
    }

    async fn search(&self, _query: &str, _filter: MemoryFilter, _top_k: usize) -> Result<Vec<MemoryEntry>> {
        // No embeddings yet
        Ok(Vec::new())
    }

    async fn get_session_context(&self, session_id: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        self.recent(session_id, limit).await
    }

    async fn get_agent_memories(&self, agent_id: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let rows = sqlx::query("SELECT * FROM messages WHERE from_agent = ? ORDER BY created_at DESC LIMIT ?")
            .bind(agent_id)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        rows.into_iter().map(Self::row_to_memory).collect()
    }

    async fn recent(&self, channel: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let rows = sqlx::query("SELECT * FROM messages WHERE channel = ? ORDER BY created_at DESC LIMIT ?")
            .bind(channel)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        rows.into_iter().map(Self::row_to_memory).collect()
    }

    async fn by_correlation(&self, id: &str) -> Result<Vec<MemoryEntry>> {
        let rows = sqlx::query("SELECT * FROM messages WHERE correlation_id = ? ORDER BY created_at ASC")
            .bind(id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        rows.into_iter().map(Self::row_to_memory).collect()
    }

    async fn update(&self, memory_id: &str, content: &str) -> Result<()> {
        let rows_affected = sqlx::query("UPDATE messages SET content = ? WHERE message_id = ?")
            .bind(content)
            .bind(memory_id)
            .execute(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?
            .rows_affected();

        if rows_affected > 0 {
            Ok(())
        } else {
            Err(MemoryError::NotFound(memory_id.to_string()))
        }
    }

    async fn delete(&self, memory_id: &str) -> Result<()> {
        let rows_affected = sqlx::query("DELETE FROM messages WHERE message_id = ?")
            .bind(memory_id)
            .execute(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?
            .rows_affected();

        if rows_affected > 0 {
            Ok(())
        } else {
            Err(MemoryError::NotFound(memory_id.to_string()))
        }
    }

    async fn export(&self, _filter: MemoryFilter, _format: ExportFormat) -> Result<Vec<u8>> {
        Err(MemoryError::Backend("Not implemented".to_string()))
    }

    async fn import(&self, _data: &[u8], _format: ExportFormat) -> Result<ImportResult> {
        Err(MemoryError::Backend("Not implemented".to_string()))
    }

    async fn count(&self) -> Result<usize> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM messages")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(row.0 as usize)
    }

    async fn clear(&self) -> Result<()> {
        sqlx::query("DELETE FROM messages")
            .execute(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        self.pool.close().await;
        Ok(())
    }
}
