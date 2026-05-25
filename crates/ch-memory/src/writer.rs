use crate::{EvidenceRow, EvidenceStatus, EvidenceStore, MemoryStore};
use ch_core::bus::MessageBus;
use ch_core::channel::ChannelVisibility;
use ch_protocol::{AgentId, MemoryEntry, MessageType, Payload};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Spawn the background task that persists bus traffic to the memory + evidence
/// stores.  Subscribes to the "general" channel and dispatches by payload kind:
///
///   * `Payload::Text` + TaskRequest/TaskResponse → `messages` table (chat)
///   * `Payload::Handoff` + Handoff               → `messages` table (JSON in content, memory_type=handoff)
///   * `Payload::Evidence` + Evidence             → `evidence` table (write_evidence)
///   * `Payload::EvidenceVerify` + EvidenceVerify → `evidence` table (verify/fail)
///   * anything else (heartbeats, metrics, etc.)  → silently skipped
///
/// `memory_store` and `evidence_store` are usually the same concrete
/// `SqliteMemoryStore` cloned into two trait objects — callers can pass
/// `store.clone()` for both arguments.
pub fn spawn_memory_writer(
    bus: Arc<MessageBus>,
    memory_store: Arc<dyn MemoryStore>,
    evidence_store: Arc<dyn EvidenceStore>,
) -> tokio::task::JoinHandle<()> {
    let writer_id = AgentId::new();
    let store = memory_store;
    tokio::spawn(async move {
        let mut rx = bus.subscribe(writer_id).await;
        let _ = bus.join_channel("general", writer_id, ChannelVisibility::Full);
        info!("memory writer subscribed to general channel");

        while let Some(msg) = rx.recv().await {
            // Dispatch evidence payloads BEFORE the message-table tuple match
            // so they write to the evidence table and skip the rest of the loop.
            match (&msg.payload, &msg.message_type) {
                (Payload::Evidence(claim), MessageType::Evidence) => {
                    let row = EvidenceRow {
                        id: msg.message_id.to_string(),
                        task_id: claim.task_id.clone(),
                        correlation_id: msg.correlation_id.map(|c| c.to_string()),
                        agent_id: msg.from.agent_id,
                        agent_name: msg.from.agent_name.clone(),
                        claim: claim.claim.clone(),
                        status: EvidenceStatus::Pending,
                        witness: claim.witness.clone(),
                        metadata: serde_json::Value::Null,
                        created_at: msg.timestamp,
                        verified_at: None,
                        verified_by: None,
                    };
                    let row_id = row.id.clone();
                    match evidence_store.write_evidence(row).await {
                        Ok(_) => debug!(
                            "memory writer: persisted evidence (id={}, task={}, from={})",
                            row_id, claim.task_id, msg.from.agent_name
                        ),
                        Err(e) => warn!(
                            "memory writer: failed to persist evidence (id={}, from={}): {}",
                            row_id, msg.from.agent_name, e
                        ),
                    }
                    continue;
                }
                (Payload::EvidenceVerify(v), MessageType::EvidenceVerify) => {
                    let by = &msg.from.agent_name;
                    let result = if v.outcome {
                        evidence_store
                            .verify(&v.evidence_id, by, v.note.clone())
                            .await
                    } else {
                        evidence_store
                            .fail(
                                &v.evidence_id,
                                by,
                                v.note.clone().unwrap_or_else(|| "(no reason)".to_string()),
                            )
                            .await
                    };
                    match result {
                        Ok(_) => debug!(
                            "memory writer: evidence {} (id={}, by={})",
                            if v.outcome { "verified" } else { "failed" },
                            v.evidence_id,
                            by
                        ),
                        Err(e) => warn!(
                            "memory writer: failed to update evidence (id={}, by={}): {}",
                            v.evidence_id, by, e
                        ),
                    }
                    continue;
                }
                _ => {}
            }

            // Fall through to the chat/handoff dispatch — writes to the
            // `messages` table.
            let mut handoff_env: Option<ch_protocol::HandoffEnvelope> = None;
            let (content_str, memory_type_str): (String, String) =
                match (&msg.payload, &msg.message_type) {
                    (Payload::Text(text), MessageType::TaskRequest)
                    | (Payload::Text(text), MessageType::TaskResponse) => {
                        let t = format!("{:?}", msg.message_type).to_lowercase();
                        (text.clone(), t)
                    }
                    (Payload::Handoff(env), MessageType::Handoff) => {
                        let env_clone = env.clone();
                        match serde_json::to_string(env) {
                            Ok(json) => {
                                handoff_env = Some(env_clone);
                                (json, "handoff".to_string())
                            }
                            Err(e) => {
                                warn!("memory writer: failed to serialise handoff envelope: {}", e);
                                continue;
                            }
                        }
                    }
                    _ => continue,
                };

            let mut metadata = std::collections::HashMap::new();
            if let Some(corr_id) = msg.correlation_id {
                metadata.insert(
                    "correlation_id".to_string(),
                    serde_json::Value::String(corr_id.to_string()),
                );
            }
            if let Some(ref to) = msg.to {
                metadata.insert(
                    "to_agent".to_string(),
                    serde_json::Value::String(to.agent_id.to_string()),
                );
                metadata.insert(
                    "to_agent_name".to_string(),
                    serde_json::Value::String(to.agent_name.clone()),
                );
            }
            // Capture the sender's display name (e.g. "claude-wsl-ubuntu",
            // "You") so `crow memory tail` and the future memory browser
            // can show readable labels instead of raw AgentId UUIDs.
            metadata.insert(
                "from_agent_name".to_string(),
                serde_json::Value::String(msg.from.agent_name.clone()),
            );

            let memory = MemoryEntry {
                memory_id: msg.message_id.to_string(),
                agent_id: msg.from.agent_id,
                session_id: msg.session_id.clone(),
                content: content_str,
                embedding: None,
                memory_type: memory_type_str.clone(),
                metadata,
                created_at: msg.timestamp,
                updated_at: msg.timestamp,
            };

            let msg_id = memory.memory_id.clone();
            match store.write(memory).await {
                Ok(_) => {
                    // DEBUG level (not INFO) because there's one of these
                    // per chunk of every streaming response — can be very
                    // chatty.  Enable via `RUST_LOG=ch_memory=debug` when
                    // diagnosing.
                    debug!(
                        "memory writer: persisted {} (msg_id={}, from={})",
                        memory_type_str, msg_id, msg.from.agent_name
                    );
                }
                Err(e) => {
                    warn!(
                        "memory writer: failed to persist message (msg_id={}, from={}): {}",
                        msg_id, msg.from.agent_name, e
                    );
                }
            }

            // Fan out Handoff decisions as pending Evidence rows
            if let Some(env) = handoff_env {
                if !env.decisions.is_empty() {
                    let task_id = msg
                        .correlation_id
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| msg.message_id.to_string());
                    let evidence_store = evidence_store.clone();
                    for (idx, decision) in env.decisions.iter().enumerate() {
                        let row = EvidenceRow {
                            id: format!("{}-d{}", msg.message_id, idx),
                            task_id: task_id.clone(),
                            correlation_id: msg.correlation_id.map(|c| c.to_string()),
                            agent_id: msg.from.agent_id,
                            agent_name: msg.from.agent_name.clone(),
                            claim: decision.clone(),
                            status: EvidenceStatus::Pending,
                            witness: None,
                            metadata: serde_json::json!({
                                "source": "handoff",
                                "handoff_message_id": msg.message_id.to_string(),
                            }),
                            created_at: msg.timestamp,
                            verified_at: None,
                            verified_by: None,
                        };
                        if let Err(e) = evidence_store.write_evidence(row).await {
                            warn!("memory writer: failed to fan out handoff evidence: {}", e);
                        }
                    }
                }
            }
        }
        // The bus channel closed — writer is shutting down.  Log so we know
        // the writer stopped intentionally vs panicked silently.
        info!("memory writer: bus rx closed, writer task exiting");
    })
}
