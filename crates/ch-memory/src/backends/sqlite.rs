use crate::{
    EvidenceRow, EvidenceStatus, EvidenceStore, ExportFormat, ImportResult, MemoryError,
    MemoryFilter, MemoryStore, Result, SqliteConfig, WorkflowStepRow, WorkflowStore,
};
use async_trait::async_trait;
use ch_protocol::AgentId;
use ch_protocol::MemoryEntry;
use ch_protocol::WorkflowStepState;
use chrono::{DateTime, Utc};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use std::str::FromStr;
use uuid::Uuid;

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
        // Two tables live in the same DB:
        //   * messages: existing chat / handoff records (MemoryStore)
        //   * evidence: Maestro Task 2 — auditable agent claims (EvidenceStore)
        // Both use IF NOT EXISTS so init() is idempotent — old DBs upgrade
        // in place when the binary first opens them.
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

        CREATE TABLE IF NOT EXISTS evidence (
            id              TEXT PRIMARY KEY,
            task_id         TEXT NOT NULL,
            correlation_id  TEXT,
            agent_id        TEXT NOT NULL,
            agent_name      TEXT,
            claim           TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'pending',
            witness         TEXT,
            metadata        TEXT,
            created_at      INTEGER NOT NULL,
            verified_at     INTEGER,
            verified_by     TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_evidence_task_id ON evidence(task_id);
        CREATE INDEX IF NOT EXISTS idx_evidence_correlation ON evidence(correlation_id);
        CREATE INDEX IF NOT EXISTS idx_evidence_status ON evidence(status);

        CREATE TABLE IF NOT EXISTS workflow_steps (
            step_id      TEXT PRIMARY KEY,
            workflow_id  TEXT NOT NULL,
            state        TEXT NOT NULL DEFAULT 'pending',
            claimed_by   TEXT,
            claimed_at   TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_steps_workflow_id ON workflow_steps(workflow_id);
        CREATE INDEX IF NOT EXISTS idx_workflow_steps_state ON workflow_steps(state);
        ";
        sqlx::query(schema)
            .execute(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn write(&self, memory: MemoryEntry) -> Result<String> {
        let correlation_id = memory
            .metadata
            .get("correlation_id")
            .and_then(|v| v.as_str());
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

        Ok(memory.memory_id.clone())
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

    async fn search(
        &self,
        _query: &str,
        _filter: MemoryFilter,
        _top_k: usize,
    ) -> Result<Vec<MemoryEntry>> {
        // No embeddings yet
        Ok(Vec::new())
    }

    async fn get_session_context(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        self.recent(session_id, limit).await
    }

    async fn get_agent_memories(&self, agent_id: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let rows = sqlx::query(
            "SELECT * FROM messages WHERE from_agent = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(agent_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;

        rows.into_iter().map(Self::row_to_memory).collect()
    }

    async fn recent(&self, channel: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        let rows = sqlx::query(
            "SELECT * FROM messages WHERE channel = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(channel)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;

        rows.into_iter().map(Self::row_to_memory).collect()
    }

    async fn by_correlation(&self, id: &str) -> Result<Vec<MemoryEntry>> {
        let rows =
            sqlx::query("SELECT * FROM messages WHERE correlation_id = ? ORDER BY created_at ASC")
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

// ─── EvidenceStore impl ─────────────────────────────────────────────────────
//
// Lives on the same `SqliteMemoryStore` so a single `Arc` carries both
// MemoryStore + EvidenceStore capabilities.  Writer / CLI / TUI hold the
// concrete `Arc<SqliteMemoryStore>` and dispatch to whichever trait they
// need.

impl SqliteMemoryStore {
    /// Decode one row from the `evidence` table.  Symmetric with
    /// `row_to_memory` above.  Unknown status strings default to Pending
    /// (forward-compat — see `EvidenceStatus::from_str`).
    fn row_to_evidence(row: sqlx::sqlite::SqliteRow) -> Result<EvidenceRow> {
        let metadata_str: Option<String> = row.try_get("metadata").unwrap_or(None);
        let metadata = metadata_str
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::Value::Null);

        let agent_id_str: String = row.try_get("agent_id").unwrap_or_default();
        let agent_id = AgentId(Uuid::parse_str(&agent_id_str).unwrap_or_else(|_| Uuid::new_v4()));

        let created_at: i64 = row.try_get("created_at").unwrap_or(0);
        let created_at = DateTime::from_timestamp(created_at, 0).unwrap_or_default();

        let verified_at: Option<i64> = row.try_get("verified_at").unwrap_or(None);
        let verified_at = verified_at.and_then(|t| DateTime::from_timestamp(t, 0));

        let status_str: String = row
            .try_get("status")
            .unwrap_or_else(|_| "pending".to_string());

        Ok(EvidenceRow {
            id: row.try_get("id").unwrap_or_default(),
            task_id: row.try_get("task_id").unwrap_or_default(),
            correlation_id: row.try_get("correlation_id").unwrap_or(None),
            agent_id,
            agent_name: row.try_get("agent_name").unwrap_or_default(),
            claim: row.try_get("claim").unwrap_or_default(),
            status: EvidenceStatus::from_str(&status_str),
            witness: row.try_get("witness").unwrap_or(None),
            metadata,
            created_at,
            verified_at,
            verified_by: row.try_get("verified_by").unwrap_or(None),
        })
    }
}

#[async_trait]
impl EvidenceStore for SqliteMemoryStore {
    async fn write_evidence(&self, row: EvidenceRow) -> Result<()> {
        let metadata_str = serde_json::to_string(&row.metadata).unwrap_or_default();
        sqlx::query(
            "INSERT OR REPLACE INTO evidence
            (id, task_id, correlation_id, agent_id, agent_name, claim,
             status, witness, metadata, created_at, verified_at, verified_by)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&row.id)
        .bind(&row.task_id)
        .bind(&row.correlation_id)
        .bind(row.agent_id.to_string())
        .bind(&row.agent_name)
        .bind(&row.claim)
        .bind(row.status.as_str())
        .bind(&row.witness)
        .bind(&metadata_str)
        .bind(row.created_at.timestamp())
        .bind(row.verified_at.map(|t| t.timestamp()))
        .bind(&row.verified_by)
        .execute(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn verify(&self, id: &str, by: &str, witness: Option<String>) -> Result<()> {
        let now = Utc::now().timestamp();
        // If witness is None, we want to keep whatever was already there.
        // COALESCE on the bind side: ? IS NULL ? existing : new.
        let rows_affected = sqlx::query(
            "UPDATE evidence
             SET status      = 'verified',
                 verified_at = ?,
                 verified_by = ?,
                 witness     = COALESCE(?, witness)
             WHERE id = ?",
        )
        .bind(now)
        .bind(by)
        .bind(witness)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?
        .rows_affected();

        if rows_affected > 0 {
            Ok(())
        } else {
            Err(MemoryError::NotFound(id.to_string()))
        }
    }

    async fn fail(&self, id: &str, by: &str, reason: String) -> Result<()> {
        let now = Utc::now().timestamp();
        // Merge `failure_reason` into the metadata JSON.  Read-modify-write
        // pattern — fine for the expected volume (one transition per claim).
        let existing_meta: Option<String> =
            sqlx::query_scalar("SELECT metadata FROM evidence WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| MemoryError::Backend(e.to_string()))?
                .ok_or_else(|| MemoryError::NotFound(id.to_string()))?;

        let mut meta: serde_json::Value = existing_meta
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        if !meta.is_object() {
            meta = serde_json::Value::Object(serde_json::Map::new());
        }
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                "failure_reason".to_string(),
                serde_json::Value::String(reason),
            );
        }
        let new_meta = serde_json::to_string(&meta).unwrap_or_default();

        let rows_affected = sqlx::query(
            "UPDATE evidence
             SET status      = 'failed',
                 verified_at = ?,
                 verified_by = ?,
                 metadata    = ?
             WHERE id = ?",
        )
        .bind(now)
        .bind(by)
        .bind(&new_meta)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?
        .rows_affected();

        if rows_affected > 0 {
            Ok(())
        } else {
            Err(MemoryError::NotFound(id.to_string()))
        }
    }

    async fn by_task(&self, task_id: &str) -> Result<Vec<EvidenceRow>> {
        let rows = sqlx::query("SELECT * FROM evidence WHERE task_id = ? ORDER BY created_at ASC")
            .bind(task_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| MemoryError::Backend(e.to_string()))?;

        rows.into_iter().map(Self::row_to_evidence).collect()
    }

    async fn pending(&self, limit: usize) -> Result<Vec<EvidenceRow>> {
        self.by_status(EvidenceStatus::Pending, limit).await
    }

    async fn by_status(&self, status: EvidenceStatus, limit: usize) -> Result<Vec<EvidenceRow>> {
        let rows =
            sqlx::query("SELECT * FROM evidence WHERE status = ? ORDER BY created_at ASC LIMIT ?")
                .bind(status.as_str())
                .bind(limit as i64)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| MemoryError::Backend(e.to_string()))?;

        rows.into_iter().map(Self::row_to_evidence).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ch_protocol::AgentId;
    use chrono::Utc;

    #[tokio::test]
    async fn test_sqlite_store() {
        let config = SqliteConfig {
            path: ":memory:".to_string(),
            embedding_dim: 768,
        };
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

        // Test recent
        let recent = store.recent("session-1", 10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].content, memory.content);

        // Test empty recent
        let empty_recent = store.recent("session-2", 10).await.unwrap();
        assert_eq!(empty_recent.len(), 0);
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((SqliteMemoryStore::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![0.0, 1.0, 0.0];
        assert!(SqliteMemoryStore::cosine_similarity(&a, &c).abs() < 0.001);
    }

    // ─── Evidence (Maestro Task 2) ──────────────────────────────────────────

    async fn fresh_store() -> SqliteMemoryStore {
        let config = SqliteConfig {
            path: ":memory:".to_string(),
            embedding_dim: 768,
        };
        SqliteMemoryStore::new(config).await.unwrap()
    }

    fn sample_evidence(id: &str, task_id: &str, claim: &str) -> EvidenceRow {
        EvidenceRow {
            id: id.to_string(),
            task_id: task_id.to_string(),
            correlation_id: None,
            agent_id: AgentId::new(),
            agent_name: "test-agent".to_string(),
            claim: claim.to_string(),
            status: EvidenceStatus::Pending,
            witness: None,
            metadata: serde_json::Value::Null,
            created_at: Utc::now(),
            verified_at: None,
            verified_by: None,
        }
    }

    #[tokio::test]
    async fn evidence_schema_round_trip() {
        let store = fresh_store().await;
        let row = sample_evidence("ev-1", "task-1", "built the thing");
        store.write_evidence(row.clone()).await.unwrap();

        let fetched = store.by_task("task-1").await.unwrap();
        assert_eq!(fetched.len(), 1);
        assert_eq!(fetched[0].id, "ev-1");
        assert_eq!(fetched[0].claim, "built the thing");
        assert_eq!(fetched[0].status, EvidenceStatus::Pending);
        assert!(fetched[0].verified_at.is_none());
        assert!(fetched[0].verified_by.is_none());
    }

    #[tokio::test]
    async fn evidence_verify_transitions_pending_to_verified() {
        let store = fresh_store().await;
        let row = sample_evidence("ev-2", "task-1", "shipped");
        store.write_evidence(row).await.unwrap();

        store
            .verify("ev-2", "human:zhiqing", Some("PR #42".to_string()))
            .await
            .unwrap();

        let fetched = store.by_task("task-1").await.unwrap();
        assert_eq!(fetched[0].status, EvidenceStatus::Verified);
        assert_eq!(fetched[0].verified_by.as_deref(), Some("human:zhiqing"));
        assert_eq!(fetched[0].witness.as_deref(), Some("PR #42"));
        assert!(fetched[0].verified_at.is_some());
    }

    #[tokio::test]
    async fn evidence_fail_transitions_pending_to_failed_with_reason() {
        let store = fresh_store().await;
        let row = sample_evidence("ev-3", "task-1", "deployed");
        store.write_evidence(row).await.unwrap();

        store
            .fail("ev-3", "verifier-agent", "rollback at T+5min".to_string())
            .await
            .unwrap();

        let fetched = store.by_task("task-1").await.unwrap();
        assert_eq!(fetched[0].status, EvidenceStatus::Failed);
        assert_eq!(fetched[0].verified_by.as_deref(), Some("verifier-agent"));
        assert!(fetched[0].verified_at.is_some());
        // failure_reason is merged into metadata
        assert_eq!(
            fetched[0]
                .metadata
                .get("failure_reason")
                .and_then(|v| v.as_str()),
            Some("rollback at T+5min")
        );
    }

    #[tokio::test]
    async fn evidence_by_task_returns_only_matching_task_in_order() {
        let store = fresh_store().await;
        // Write three rows: two for task-A, one for task-B.  Sleep between
        // writes so created_at timestamps are distinct enough for ORDER BY.
        let mut a1 = sample_evidence("a1", "task-A", "first A");
        a1.created_at = DateTime::from_timestamp(1_000, 0).unwrap();
        let mut a2 = sample_evidence("a2", "task-A", "second A");
        a2.created_at = DateTime::from_timestamp(2_000, 0).unwrap();
        let mut b1 = sample_evidence("b1", "task-B", "first B");
        b1.created_at = DateTime::from_timestamp(1_500, 0).unwrap();

        // Write in non-chronological order to prove ORDER BY does the work.
        store.write_evidence(a2).await.unwrap();
        store.write_evidence(b1).await.unwrap();
        store.write_evidence(a1).await.unwrap();

        let a_rows = store.by_task("task-A").await.unwrap();
        assert_eq!(
            a_rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["a1", "a2"]
        );

        let b_rows = store.by_task("task-B").await.unwrap();
        assert_eq!(b_rows.len(), 1);
        assert_eq!(b_rows[0].id, "b1");
    }

    #[tokio::test]
    async fn evidence_pending_returns_only_pending_with_limit() {
        let store = fresh_store().await;
        for i in 0..5 {
            let mut row = sample_evidence(&format!("ev-{}", i), "task-1", "claim");
            row.created_at = DateTime::from_timestamp(1_000 + i as i64, 0).unwrap();
            store.write_evidence(row).await.unwrap();
        }
        // Verify two of them
        store.verify("ev-0", "v", None).await.unwrap();
        store.verify("ev-1", "v", None).await.unwrap();

        let pending = store.pending(10).await.unwrap();
        assert_eq!(pending.len(), 3);
        assert!(pending.iter().all(|r| r.status == EvidenceStatus::Pending));

        // Limit is honored
        let limited = store.pending(2).await.unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[tokio::test]
    async fn evidence_verify_unknown_id_returns_not_found() {
        let store = fresh_store().await;
        let result = store.verify("does-not-exist", "v", None).await;
        assert!(matches!(result, Err(MemoryError::NotFound(_))));
    }

    #[tokio::test]
    async fn evidence_by_status_filters_correctly() {
        let store = fresh_store().await;
        // Seed: 2 pending, 1 verified, 1 failed.
        for i in 0..4 {
            let mut row = sample_evidence(&format!("ev-{}", i), "task-x", "c");
            row.created_at = DateTime::from_timestamp(1_000 + i as i64, 0).unwrap();
            store.write_evidence(row).await.unwrap();
        }
        store.verify("ev-2", "v", None).await.unwrap();
        store.fail("ev-3", "v", "bad".to_string()).await.unwrap();

        let pending = store.by_status(EvidenceStatus::Pending, 10).await.unwrap();
        assert_eq!(pending.len(), 2);
        let verified = store.by_status(EvidenceStatus::Verified, 10).await.unwrap();
        assert_eq!(verified.len(), 1);
        assert_eq!(verified[0].id, "ev-2");
        let failed = store.by_status(EvidenceStatus::Failed, 10).await.unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, "ev-3");
    }
}

// ── WorkflowStore impl (Maestro Task 3) ────────────────────────────────────
//
// Single source of truth — earlier in this file there were TWO `impl
// WorkflowStore for SqliteMemoryStore` blocks left over from sed-driven
// scaffolding.  Removed the stub second copy, kept the real implementation
// here.  See `claimed_at` TODO below — SQLite's `datetime('now')` returns
// `YYYY-MM-DD HH:MM:SS` (not RFC3339); we tolerate the parse failure by
// returning epoch via `unwrap_or_default()` until we switch to INTEGER
// timestamps to match `messages` / `evidence`.

#[async_trait]
impl WorkflowStore for SqliteMemoryStore {
    async fn claim_step(&self, workflow_id: &str, step_id: &str, agent_id: &str) -> Result<()> {
        // ON CONFLICT uses `excluded.claimed_by` so the parameter only needs
        // to be bound once (idiomatic SQLite upsert).
        sqlx::query(
            "INSERT INTO workflow_steps (step_id, workflow_id, state, claimed_by, claimed_at) \
             VALUES (?, ?, 'claimed', ?, datetime('now')) \
             ON CONFLICT(step_id) DO UPDATE SET \
                 state = 'claimed', \
                 claimed_by = excluded.claimed_by, \
                 claimed_at = datetime('now')",
        )
        .bind(step_id)
        .bind(workflow_id)
        .bind(agent_id)
        .execute(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn by_workflow(&self, workflow_id: &str) -> Result<Vec<WorkflowStepRow>> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>)>(
            "SELECT step_id, workflow_id, state, claimed_by, claimed_at \
             FROM workflow_steps WHERE workflow_id = ?",
        )
        .bind(workflow_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(sid, wid, st, cb, ca)| WorkflowStepRow {
                step_id: sid,
                workflow_id: wid,
                state: match st.as_str() {
                    "claimed" => WorkflowStepState::Claimed,
                    _ => WorkflowStepState::Pending,
                },
                claimed_by: cb,
                // TODO: schema stores claimed_at via `datetime('now')` which
                // is YYYY-MM-DD HH:MM:SS, not RFC3339.  parse_from_rfc3339
                // will fail and unwrap_or_default() yields epoch zero.
                // Switch the column to INTEGER (unix ts) to match
                // `messages.created_at` / `evidence.created_at` for
                // consistency.
                claimed_at: ca.map(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .unwrap_or_default()
                        .with_timezone(&chrono::Utc)
                }),
            })
            .collect())
    }

    async fn pending_steps(&self, limit: usize) -> Result<Vec<WorkflowStepRow>> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>)>(
            "SELECT step_id, workflow_id, state, claimed_by, claimed_at \
             FROM workflow_steps WHERE state = 'pending' LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MemoryError::Backend(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(sid, wid, _st, cb, _ca)| WorkflowStepRow {
                step_id: sid,
                workflow_id: wid,
                state: WorkflowStepState::Pending,
                claimed_by: cb,
                // Pending steps haven't been claimed; claimed_at is NULL.
                claimed_at: None,
            })
            .collect())
    }
}
