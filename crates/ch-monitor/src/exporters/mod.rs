//! Metrics exporters

pub mod console;
pub mod prometheus;

use crate::{MetricsSnapshot, Result};

/// Exporter trait for exporting metrics
#[async_trait::async_trait]
pub trait Exporter: Send + Sync {
    /// Export metrics snapshot
    async fn export(&self, snapshot: &MetricsSnapshot) -> Result<()>;

    /// Get exporter name
    fn name(&self) -> &str;
}
