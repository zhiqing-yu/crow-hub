//! Agent Drivers
//!
//! Different strategies for connecting to agents based on
//! where and how they run.

pub mod api;
pub mod subprocess;
pub mod tmux;

pub use api::APIDriver;
pub use subprocess::SubprocessDriver;
pub use tmux::TmuxDriver;

use crate::Result;
use ch_model::{ChatRequest, ChatResponse, ChatStreamChunk};
use futures::stream::BoxStream;

/// Trait that all agent drivers must implement
#[async_trait::async_trait]
pub trait AgentDriver: Send + Sync {
    /// Send a chat request through this agent
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;

    /// Send a streaming chat request
    async fn stream_chat(&self, request: ChatRequest) -> Result<BoxStream<'static, Result<ChatStreamChunk>>>;

    /// Check if the agent is reachable
    async fn health_check(&self) -> Result<bool>;

    /// Get the driver type name
    fn driver_type(&self) -> &str;

    /// Stop the driver (cleanup)
    async fn stop(&self) -> Result<()>;
}
