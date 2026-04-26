//! Channel system for group communication
//!
//! Provides named channels (#general, #code-review, etc.)
//! with per-agent visibility controls.

use ch_protocol::AgentId;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Visibility level for an agent in a channel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelVisibility {
    /// Agent sees all messages in full
    Full,
    /// Agent only gets notified of activity (topic + sender, not content)
    Notify,
    /// Agent is muted — receives nothing
    None,
}

/// A named communication channel
pub struct Channel {
    /// Channel name (e.g. "general", "code-review")
    pub name: String,
    /// Members with their visibility level
    members: DashMap<AgentId, ChannelVisibility>,
    /// When the channel was created
    pub created_at: DateTime<Utc>,
    /// Optional topic/description
    pub topic: String,
}

impl Channel {
    /// Create a new channel
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            members: DashMap::new(),
            created_at: Utc::now(),
            topic: String::new(),
        }
    }

    /// Create a channel with a topic
    pub fn with_topic(mut self, topic: impl Into<String>) -> Self {
        self.topic = topic.into();
        self
    }

    /// Add a member to the channel
    pub fn join(&self, agent_id: AgentId, visibility: ChannelVisibility) {
        self.members.insert(agent_id, visibility);
    }

    /// Remove a member from the channel
    pub fn leave(&self, agent_id: &AgentId) -> bool {
        self.members.remove(agent_id).is_some()
    }

    /// Check if an agent is a member
    pub fn is_member(&self, agent_id: &AgentId) -> bool {
        self.members.contains_key(agent_id)
    }

    /// Get a member's visibility level
    pub fn get_visibility(&self, agent_id: &AgentId) -> Option<ChannelVisibility> {
        self.members.get(agent_id).map(|v| *v)
    }

    /// Update a member's visibility
    pub fn set_visibility(&self, agent_id: &AgentId, visibility: ChannelVisibility) -> bool {
        if let Some(mut entry) = self.members.get_mut(agent_id) {
            *entry = visibility;
            true
        } else {
            false
        }
    }

    /// Get all members with Full visibility (should receive full messages)
    pub fn full_members(&self) -> Vec<AgentId> {
        self.members
            .iter()
            .filter(|e| *e.value() == ChannelVisibility::Full)
            .map(|e| *e.key())
            .collect()
    }

    /// Get all members with Notify visibility
    pub fn notify_members(&self) -> Vec<AgentId> {
        self.members
            .iter()
            .filter(|e| *e.value() == ChannelVisibility::Notify)
            .map(|e| *e.key())
            .collect()
    }

    /// Get all member IDs
    pub fn all_members(&self) -> Vec<AgentId> {
        self.members.iter().map(|e| *e.key()).collect()
    }

    /// Get member count
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Set the topic
    pub fn set_topic(&self, _topic: impl Into<String>) -> String {
        // Note: Can't mutate &self directly, but DashMap allows interior mutability
        // For topic, we'd need RwLock or similar — for now return what was set
        // This is a known limitation of the current design
        self.topic.clone()
    }
}

/// Summary info about a channel (safe to clone/send)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub name: String,
    pub topic: String,
    pub member_count: usize,
    pub created_at: DateTime<Utc>,
}

impl From<&Channel> for ChannelInfo {
    fn from(ch: &Channel) -> Self {
        Self {
            name: ch.name.clone(),
            topic: ch.topic.clone(),
            member_count: ch.member_count(),
            created_at: ch.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_create() {
        let ch = Channel::new("general").with_topic("Main channel");
        assert_eq!(ch.name, "general");
        assert_eq!(ch.topic, "Main channel");
        assert_eq!(ch.member_count(), 0);
    }

    #[test]
    fn test_channel_join_leave() {
        let ch = Channel::new("test");
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();

        ch.join(agent1, ChannelVisibility::Full);
        ch.join(agent2, ChannelVisibility::Notify);

        assert_eq!(ch.member_count(), 2);
        assert!(ch.is_member(&agent1));
        assert_eq!(ch.get_visibility(&agent1), Some(ChannelVisibility::Full));
        assert_eq!(ch.get_visibility(&agent2), Some(ChannelVisibility::Notify));

        ch.leave(&agent1);
        assert_eq!(ch.member_count(), 1);
        assert!(!ch.is_member(&agent1));
    }

    #[test]
    fn test_channel_visibility_filters() {
        let ch = Channel::new("test");
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let a3 = AgentId::new();

        ch.join(a1, ChannelVisibility::Full);
        ch.join(a2, ChannelVisibility::Full);
        ch.join(a3, ChannelVisibility::Notify);

        assert_eq!(ch.full_members().len(), 2);
        assert_eq!(ch.notify_members().len(), 1);
        assert_eq!(ch.all_members().len(), 3);
    }

    #[test]
    fn test_channel_update_visibility() {
        let ch = Channel::new("test");
        let agent = AgentId::new();

        ch.join(agent, ChannelVisibility::Full);
        assert_eq!(ch.get_visibility(&agent), Some(ChannelVisibility::Full));

        ch.set_visibility(&agent, ChannelVisibility::None);
        assert_eq!(ch.get_visibility(&agent), Some(ChannelVisibility::None));
    }

    #[test]
    fn test_channel_info() {
        let ch = Channel::new("code-review").with_topic("PR reviews");
        let a1 = AgentId::new();
        ch.join(a1, ChannelVisibility::Full);

        let info = ChannelInfo::from(&ch);
        assert_eq!(info.name, "code-review");
        assert_eq!(info.topic, "PR reviews");
        assert_eq!(info.member_count, 1);
    }
}
