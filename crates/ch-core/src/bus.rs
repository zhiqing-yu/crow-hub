//! Message Bus implementation
//!
//! Provides pub/sub messaging, named channels, and direct messages
//! for inter-agent communication.

use ch_protocol::{AgentMessage, AgentId};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, info, warn};

use crate::channel::{Channel, ChannelInfo, ChannelVisibility};
use crate::Result;
use crate::CoreError;

/// Message bus for agent communication
pub struct MessageBus {
    /// Subscribers: agent_id -> sender channel
    subscribers: DashMap<AgentId, mpsc::Sender<AgentMessage>>,
    /// Broadcast channel for all messages (for monitoring)
    broadcast: broadcast::Sender<AgentMessage>,
    /// Message history (circular buffer)
    history: Arc<RwLock<Vec<AgentMessage>>>,
    /// Maximum history size
    max_history: usize,
    /// Running state
    running: Arc<RwLock<bool>>,
    /// Named channels
    channels: DashMap<String, Channel>,
    /// Per-channel message history
    channel_history: DashMap<String, Arc<RwLock<Vec<AgentMessage>>>>,
}

impl MessageBus {
    /// Create a new message bus
    pub fn new() -> Self {
        let (broadcast, _) = broadcast::channel(1000);
        
        Self {
            subscribers: DashMap::new(),
            broadcast,
            history: Arc::new(RwLock::new(Vec::new())),
            max_history: 10000,
            running: Arc::new(RwLock::new(false)),
            channels: DashMap::new(),
            channel_history: DashMap::new(),
        }
    }
    
    /// Start the message bus
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write().await;
        *running = true;
        info!("Message bus started");
        Ok(())
    }
    
    /// Shutdown the message bus
    pub async fn shutdown(&self) -> Result<()> {
        let mut running = self.running.write().await;
        *running = false;
        self.subscribers.clear();
        info!("Message bus shutdown");
        Ok(())
    }
    
    /// Check if bus is running
    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }
    
    // ── Agent Subscription ───────────────────────────────────

    /// Register a subscriber
    pub async fn subscribe(&self, agent_id: AgentId) -> mpsc::Receiver<AgentMessage> {
        let (tx, rx) = mpsc::channel(100);
        self.subscribers.insert(agent_id, tx);
        debug!("Agent {} subscribed to message bus", agent_id);
        rx
    }
    
    /// Unregister a subscriber
    pub fn unsubscribe(&self, agent_id: &AgentId) {
        self.subscribers.remove(agent_id);
        debug!("Agent {} unsubscribed from message bus", agent_id);
    }

    /// Get subscriber count
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    /// Get broadcast receiver for monitoring
    pub fn subscribe_broadcast(&self) -> broadcast::Receiver<AgentMessage> {
        self.broadcast.subscribe()
    }
    
    // ── Direct Messaging ─────────────────────────────────────

    /// Publish a message (direct or broadcast)
    pub async fn publish(&self, message: AgentMessage) -> Result<()> {
        if !self.is_running().await {
            return Err(CoreError::Bus("Bus not running".to_string()));
        }
        
        // Add to history
        {
            let mut history = self.history.write().await;
            history.push(message.clone());
            if history.len() > self.max_history {
                history.remove(0);
            }
        }
        
        // Send to broadcast channel (for monitors)
        let _ = self.broadcast.send(message.clone());
        
        // Route to specific recipient if specified
        if let Some(ref to) = message.to {
            if let Some(subscriber) = self.subscribers.get(&to.agent_id) {
                if let Err(e) = subscriber.send(message.clone()).await {
                    warn!("Failed to send message to {}: {}", to.agent_id, e);
                }
            }
        } else {
            // Broadcast to all subscribers
            for entry in self.subscribers.iter() {
                if entry.key() != &message.from.agent_id {
                    if let Err(e) = entry.value().send(message.clone()).await {
                        warn!("Failed to send message to {}: {}", entry.key(), e);
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Send a direct message to a specific agent
    pub async fn send_dm(
        &self,
        from_id: &AgentId,
        to_id: &AgentId,
        message: AgentMessage,
    ) -> Result<()> {
        if !self.is_running().await {
            return Err(CoreError::Bus("Bus not running".to_string()));
        }

        // Record in history
        {
            let mut history = self.history.write().await;
            history.push(message.clone());
            if history.len() > self.max_history {
                history.remove(0);
            }
        }

        // Broadcast for monitors
        let _ = self.broadcast.send(message.clone());

        // Deliver to target
        if let Some(subscriber) = self.subscribers.get(to_id) {
            if let Err(e) = subscriber.send(message).await {
                warn!("Failed to send DM to {}: {}", to_id, e);
            }
        } else {
            return Err(CoreError::Bus(format!("Agent {} not subscribed", to_id)));
        }

        Ok(())
    }
    
    /// Send message with response expectation
    pub async fn request(
        &self,
        message: AgentMessage,
        timeout: std::time::Duration,
    ) -> Result<AgentMessage> {
        let correlation_id = message.message_id;
        
        self.publish(message).await?;
        
        let mut rx = self.broadcast.subscribe();
        
        let result = tokio::time::timeout(timeout, async {
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        if msg.correlation_id == Some(correlation_id) {
                            return Ok(msg);
                        }
                    }
                    Err(e) => return Err(CoreError::Bus(e.to_string())),
                }
            }
        }).await;
        
        match result {
            Ok(Ok(msg)) => Ok(msg),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(CoreError::Bus("Request timeout".to_string())),
        }
    }
    
    // ── Channel Operations ───────────────────────────────────

    /// Create a named channel
    pub fn create_channel(&self, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        if self.channels.contains_key(&name) {
            return Err(CoreError::Channel(format!("Channel '{}' already exists", name)));
        }
        self.channels.insert(name.clone(), Channel::new(&name));
        self.channel_history
            .insert(name.clone(), Arc::new(RwLock::new(Vec::new())));
        info!("Created channel #{}", name);
        Ok(())
    }

    /// Delete a channel
    pub fn delete_channel(&self, name: &str) -> Result<()> {
        if self.channels.remove(name).is_none() {
            return Err(CoreError::Channel(format!("Channel '{}' not found", name)));
        }
        self.channel_history.remove(name);
        info!("Deleted channel #{}", name);
        Ok(())
    }

    /// Join an agent to a channel
    pub fn join_channel(
        &self,
        channel_name: &str,
        agent_id: AgentId,
        visibility: ChannelVisibility,
    ) -> Result<()> {
        let channel = self.channels
            .get(channel_name)
            .ok_or_else(|| CoreError::Channel(format!("Channel '{}' not found", channel_name)))?;

        channel.join(agent_id, visibility);
        debug!("Agent {} joined #{} with {:?} visibility", agent_id, channel_name, visibility);
        Ok(())
    }

    /// Remove an agent from a channel
    pub fn leave_channel(&self, channel_name: &str, agent_id: &AgentId) -> Result<()> {
        let channel = self.channels
            .get(channel_name)
            .ok_or_else(|| CoreError::Channel(format!("Channel '{}' not found", channel_name)))?;

        if !channel.leave(agent_id) {
            return Err(CoreError::Channel(format!(
                "Agent {} is not in channel '{}'", agent_id, channel_name
            )));
        }
        debug!("Agent {} left #{}", agent_id, channel_name);
        Ok(())
    }

    /// Send a message to a channel (delivers based on each member's visibility)
    pub async fn send_to_channel(
        &self,
        channel_name: &str,
        sender_id: &AgentId,
        message: AgentMessage,
    ) -> Result<()> {
        if !self.is_running().await {
            return Err(CoreError::Bus("Bus not running".to_string()));
        }

        let channel = self.channels
            .get(channel_name)
            .ok_or_else(|| CoreError::Channel(format!("Channel '{}' not found", channel_name)))?;

        // Record in channel history
        if let Some(history) = self.channel_history.get(channel_name) {
            let mut h = history.write().await;
            h.push(message.clone());
            if h.len() > self.max_history {
                h.remove(0);
            }
        }

        // Also record in global history + broadcast for monitors
        let _ = self.broadcast.send(message.clone());

        // Deliver to Full-visibility members (except sender)
        for member_id in channel.full_members() {
            if &member_id != sender_id {
                if let Some(subscriber) = self.subscribers.get(&member_id) {
                    if let Err(e) = subscriber.send(message.clone()).await {
                        warn!("Failed to send channel msg to {}: {}", member_id, e);
                    }
                }
            }
        }

        // Notify members get a stripped-down notification (same message for now,
        // a real implementation could create a summary message)
        for member_id in channel.notify_members() {
            if &member_id != sender_id {
                if let Some(subscriber) = self.subscribers.get(&member_id) {
                    // For notify, we still send the message — the receiver can decide
                    // how to handle it based on message metadata
                    if let Err(e) = subscriber.send(message.clone()).await {
                        warn!("Failed to notify {}: {}", member_id, e);
                    }
                }
            }
        }

        // Agents with ChannelVisibility::None receive nothing

        Ok(())
    }

    /// List all channels
    pub fn list_channels(&self) -> Vec<ChannelInfo> {
        self.channels
            .iter()
            .map(|entry| ChannelInfo::from(entry.value()))
            .collect()
    }

    /// Get channel info
    pub fn get_channel_info(&self, name: &str) -> Option<ChannelInfo> {
        self.channels.get(name).map(|ch| ChannelInfo::from(ch.value()))
    }

    /// Get channel message history
    pub async fn get_channel_history(
        &self,
        channel_name: &str,
        limit: usize,
    ) -> Result<Vec<AgentMessage>> {
        let history = self.channel_history
            .get(channel_name)
            .ok_or_else(|| CoreError::Channel(format!("Channel '{}' not found", channel_name)))?;

        let h = history.read().await;
        Ok(h.iter().rev().take(limit).cloned().collect())
    }

    /// Get number of channels
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    // ── History ──────────────────────────────────────────────

    /// Get message history
    pub async fn get_history(&self, limit: usize) -> Vec<AgentMessage> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ch_protocol::{AgentAddress, MessageType, Payload};
    
    #[tokio::test]
    async fn test_subscribe_publish() {
        let bus = MessageBus::new();
        bus.start().await.unwrap();
        
        let agent_id = AgentId::new();
        let mut _rx = bus.subscribe(agent_id).await;
        
        let from = AgentAddress::new("sender", "test");
        let to = AgentAddress::new("receiver", "test");
        let msg = AgentMessage::new(
            from,
            Some(to),
            MessageType::StatusHeartbeat,
            Payload::Empty,
        );
        
        bus.publish(msg.clone()).await.unwrap();
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[tokio::test]
    async fn test_channel_create_join_send() {
        let bus = MessageBus::new();
        bus.start().await.unwrap();

        // Create channel
        bus.create_channel("general").unwrap();
        assert_eq!(bus.channel_count(), 1);

        // Two agents join
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let agent3 = AgentId::new();

        let mut rx1 = bus.subscribe(agent1).await;
        let mut rx2 = bus.subscribe(agent2).await;
        let mut rx3 = bus.subscribe(agent3).await;

        bus.join_channel("general", agent1, ChannelVisibility::Full).unwrap();
        bus.join_channel("general", agent2, ChannelVisibility::Full).unwrap();
        bus.join_channel("general", agent3, ChannelVisibility::None).unwrap();

        // Send message from agent1
        let from = AgentAddress::new("agent1", "test");
        let msg = AgentMessage::new(from, None, MessageType::StatusHeartbeat, Payload::Empty);
        bus.send_to_channel("general", &agent1, msg).await.unwrap();

        // agent2 (Full) should receive it
        let received = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx2.recv(),
        ).await;
        assert!(received.is_ok());

        // agent3 (None) should NOT receive it
        let not_received = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            rx3.recv(),
        ).await;
        assert!(not_received.is_err()); // timeout = no message
    }

    #[tokio::test]
    async fn test_channel_duplicate_error() {
        let bus = MessageBus::new();
        bus.create_channel("test").unwrap();
        assert!(bus.create_channel("test").is_err());
    }

    #[tokio::test]
    async fn test_channel_not_found_error() {
        let bus = MessageBus::new();
        let agent = AgentId::new();
        assert!(bus.join_channel("nope", agent, ChannelVisibility::Full).is_err());
    }

    #[tokio::test]
    async fn test_list_channels() {
        let bus = MessageBus::new();
        bus.create_channel("general").unwrap();
        bus.create_channel("code-review").unwrap();

        let channels = bus.list_channels();
        assert_eq!(channels.len(), 2);

        let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"general"));
        assert!(names.contains(&"code-review"));
    }

    #[tokio::test]
    async fn test_dm() {
        let bus = MessageBus::new();
        bus.start().await.unwrap();

        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        let mut _rx1 = bus.subscribe(agent1).await;
        let mut rx2 = bus.subscribe(agent2).await;

        let from = AgentAddress::new("agent1", "test");
        let msg = AgentMessage::new(from, None, MessageType::StatusHeartbeat, Payload::Empty);

        bus.send_dm(&agent1, &agent2, msg).await.unwrap();

        let received = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            rx2.recv(),
        ).await;
        assert!(received.is_ok());
    }
}

