//! Session Management
//!
//! Manages multi-agent collaboration sessions

use ch_protocol::{AgentId, SessionConfig};
use dashmap::DashMap;
use std::collections::HashSet;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::Result;
use crate::CoreError;

/// Session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Created,
    Active,
    Paused,
    Completed,
    Failed,
}

/// Collaboration session
#[derive(Debug, Clone)]
pub struct Session {
    pub session_id: String,
    pub config: SessionConfig,
    pub state: SessionState,
    pub participants: HashSet<AgentId>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub round: u32,
    pub max_rounds: Option<u32>,
    pub shared_memory_enabled: bool,
}

impl Session {
    /// Create a new session
    pub fn new(config: SessionConfig) -> Self {
        let session_id = config.session_id.clone();
        let participants = config.agent_ids.iter()
            .map(|id| AgentId(Uuid::parse_str(id).unwrap_or_else(|_| Uuid::new_v4())))
            .collect();
        
        Self {
            session_id,
            config: config.clone(),
            state: SessionState::Created,
            participants,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            round: 0,
            max_rounds: config.max_rounds,
            shared_memory_enabled: config.shared_memory,
        }
    }
    
    /// Start the session
    pub fn start(&mut self) {
        self.state = SessionState::Active;
        self.started_at = Some(chrono::Utc::now());
        info!("Session {} started", self.session_id);
    }
    
    /// Increment round
    pub fn next_round(&mut self) -> bool {
        self.round += 1;
        
        if let Some(max) = self.max_rounds {
            if self.round >= max {
                self.complete();
                return false;
            }
        }
        true
    }
    
    /// Complete the session
    pub fn complete(&mut self) {
        self.state = SessionState::Completed;
        self.completed_at = Some(chrono::Utc::now());
        info!("Session {} completed", self.session_id);
    }
    
    /// Fail the session
    pub fn fail(&mut self) {
        self.state = SessionState::Failed;
        self.completed_at = Some(chrono::Utc::now());
        warn!("Session {} failed", self.session_id);
    }
    
    /// Check if session is active
    pub fn is_active(&self) -> bool {
        self.state == SessionState::Active
    }
    
    /// Add participant
    pub fn add_participant(&mut self, agent_id: AgentId) {
        self.participants.insert(agent_id);
    }
    
    /// Remove participant
    pub fn remove_participant(&mut self, agent_id: &AgentId) {
        self.participants.remove(agent_id);
    }
}

/// Session manager
pub struct SessionManager {
    /// Active sessions
    sessions: DashMap<String, RwLock<Session>>,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }
    
    /// Create a new session
    pub async fn create(&self, config: SessionConfig) -> Result<Session> {
        if self.sessions.contains_key(&config.session_id) {
            return Err(CoreError::Session(
                format!("Session {} already exists", config.session_id)
            ));
        }
        
        let session = Session::new(config);
        self.sessions.insert(
            session.session_id.clone(),
            RwLock::new(session.clone())
        );
        
        info!("Session {} created", session.session_id);
        Ok(session)
    }
    
    /// Get a session
    pub async fn get(&self, session_id: &str) -> Result<Session> {
        if let Some(session) = self.sessions.get(session_id) {
            Ok(session.read().await.clone())
        } else {
            Err(CoreError::Session(format!("Session {} not found", session_id)))
        }
    }
    
    /// Start a session
    pub async fn start(&self, session_id: &str) -> Result<()> {
        if let Some(session) = self.sessions.get(session_id) {
            let mut s = session.write().await;
            s.start();
            Ok(())
        } else {
            Err(CoreError::Session(format!("Session {} not found", session_id)))
        }
    }
    
    /// End a session
    pub async fn end(&self, session_id: &str) -> Result<()> {
        if let Some(session) = self.sessions.get(session_id) {
            let mut s = session.write().await;
            s.complete();
            Ok(())
        } else {
            Err(CoreError::Session(format!("Session {} not found", session_id)))
        }
    }
    
    /// List all sessions
    pub async fn list(&self) -> Vec<Session> {
        let mut sessions = Vec::new();
        for entry in self.sessions.iter() {
            sessions.push(entry.value().read().await.clone());
        }
        sessions
    }
    
    /// List active sessions
    pub async fn list_active(&self) -> Vec<Session> {
        self.list().await.into_iter()
            .filter(|s| s.is_active())
            .collect()
    }
    
    /// Delete a session
    pub fn delete(&self, session_id: &str) -> Result<()> {
        self.sessions.remove(session_id);
        info!("Session {} deleted", session_id);
        Ok(())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
