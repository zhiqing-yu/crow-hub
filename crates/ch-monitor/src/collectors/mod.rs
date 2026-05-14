//! Metrics collectors

pub mod gpu;
pub mod system;

use crate::Result;

/// Collector trait for gathering metrics
#[async_trait::async_trait]
pub trait Collector: Send + Sync {
    /// Collect metrics
    async fn collect(&self) -> Result<crate::AgentMetrics>;

    /// Get collector name
    fn name(&self) -> &str;
}
