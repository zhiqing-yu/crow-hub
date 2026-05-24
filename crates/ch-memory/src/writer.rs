use crate::MemoryStore;
use ch_core::bus::MessageBus;
use ch_core::channel::ChannelVisibility;
use ch_protocol::{AgentId, MemoryEntry, MessageType, Payload};
use std::sync::Arc;
use tracing::{debug, info, warn};

pub fn spawn_memory_writer(
    bus: Arc<MessageBus>,
    store: Arc<dyn MemoryStore>,
) -> tokio::task::JoinHandle<()> {
    let writer_id = AgentId::new();
    tokio::spawn(async move {
        let mut rx = bus.subscribe(writer_id).await;
        let _ = bus.join_channel("general", writer_id, ChannelVisibility::Full);
        info!("memory writer subscribed to general channel");

        while let Some(msg) = rx.recv().await {
            // Decide what to persist for this message.  Two supported shapes:
            //   * Text + TaskRequest/TaskResponse → user/agent chat
            //   * Handoff(envelope) + Handoff     → JSON-serialised envelope
            // Anything else (heartbeats, metrics, etc.) is silently skipped.
            //
            // No SQLite schema change is needed for handoffs — the envelope
            // is stored as JSON in the existing `content` column with
            // `memory_type = "handoff"` so `crow memory tail` can recognise
            // and pretty-print them separately if it wants to.
            let (content_str, memory_type_str): (String, String) =
                match (&msg.payload, &msg.message_type) {
                    (Payload::Text(text), MessageType::TaskRequest)
                    | (Payload::Text(text), MessageType::TaskResponse) => {
                        let t = format!("{:?}", msg.message_type).to_lowercase();
                        (text.clone(), t)
                    }
                    (Payload::Handoff(env), MessageType::Handoff) => {
                        match serde_json::to_string(env) {
                            Ok(json) => (json, "handoff".to_string()),
                            Err(e) => {
                                warn!(
                                    "memory writer: failed to serialise handoff envelope: {}",
                                    e
                                );
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
        }
        // The bus channel closed — writer is shutting down.  Log so we know
        // the writer stopped intentionally vs panicked silently.
        info!("memory writer: bus rx closed, writer task exiting");
    })
}
