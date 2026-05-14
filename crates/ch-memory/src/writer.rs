use std::sync::Arc;
use ch_core::bus::MessageBus;
use ch_core::channel::ChannelVisibility;
use ch_protocol::{AgentId, MessageType, Payload, MemoryEntry};
use crate::MemoryStore;
use uuid::Uuid;

pub fn spawn_memory_writer(
    bus: Arc<MessageBus>,
    store: Arc<dyn MemoryStore>,
) -> tokio::task::JoinHandle<()> {
    let writer_id = AgentId::new();
    tokio::spawn(async move {
        let mut rx = bus.subscribe(writer_id).await;
        let _ = bus.join_channel("general", writer_id, ChannelVisibility::Full);
        
        while let Some(msg) = rx.recv().await {
            if let Payload::Text(ref text) = msg.payload {
                if matches!(msg.message_type, MessageType::TaskRequest | MessageType::TaskResponse) {
                    let mut metadata = std::collections::HashMap::new();
                    if let Some(corr_id) = msg.correlation_id {
                        metadata.insert("correlation_id".to_string(), serde_json::Value::String(corr_id.to_string()));
                    }
                    if let Some(ref to) = msg.to {
                        metadata.insert("to_agent".to_string(), serde_json::Value::String(to.agent_id.to_string()));
                    }

                    let memory = MemoryEntry {
                        memory_id: msg.message_id.to_string(),
                        agent_id: msg.from.agent_id,
                        session_id: msg.session_id.clone(),
                        content: text.clone(),
                        embedding: None,
                        memory_type: format!("{:?}", msg.message_type).to_lowercase(),
                        metadata,
                        created_at: msg.timestamp,
                        updated_at: msg.timestamp,
                    };

                    let _ = store.write(memory).await;
                }
            }
        }
    })
}
