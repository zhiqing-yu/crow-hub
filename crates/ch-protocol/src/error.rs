//! Protocol error types

use thiserror::Error;

/// Result type alias for protocol operations
pub type Result<T> = std::result::Result<T, ProtocolError>;

/// Protocol-level errors
#[derive(Error, Debug, Clone)]
pub enum ProtocolError {
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Deserialization error: {0}")]
    Deserialization(String),
    
    #[error("Invalid message format: {0}")]
    InvalidFormat(String),
    
    #[error("Message expired: {message_id}")]
    MessageExpired { message_id: String },
    
    #[error("Unknown message type: {0}")]
    UnknownMessageType(String),
    
    #[error("Routing error: {0}")]
    RoutingError(String),
    
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    
    #[error("Timeout after {duration}s")]
    Timeout { duration: u64 },
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
    
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: String, actual: String },
}

impl From<serde_json::Error> for ProtocolError {
    fn from(err: serde_json::Error) -> Self {
        ProtocolError::Serialization(err.to_string())
    }
}

impl From<std::io::Error> for ProtocolError {
    fn from(err: std::io::Error) -> Self {
        ProtocolError::InvalidFormat(err.to_string())
    }
}
