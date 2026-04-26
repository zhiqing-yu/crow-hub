//! Console exporter

use super::Exporter;
use crate::{MetricsSnapshot, Result};
use async_trait::async_trait;
use tracing::info;

/// Console metrics exporter
pub struct ConsoleExporter;

impl ConsoleExporter {
    /// Create a new console exporter
    pub fn new() -> Self {
        Self
    }
}

impl Default for ConsoleExporter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Exporter for ConsoleExporter {
    async fn export(&self, snapshot: &MetricsSnapshot) -> Result<()> {
        info!("=== Crow Hub Metrics ===");
        info!("Timestamp: {}", snapshot.timestamp);
        info!("Total Agents: {}", snapshot.system.total_agents);
        info!("Active Agents: {}", snapshot.system.active_agents);
        info!("Total Tokens: {}", snapshot.system.total_tokens);
        info!("Total Cost: ${:.4}", snapshot.system.total_cost_usd);
        
        for agent in &snapshot.agents {
            info!(
                "Agent {}: {} tokens, ${:.4} cost, {} requests",
                agent.agent_name,
                agent.tokens.total_tokens,
                agent.tokens.cost_usd,
                agent.requests_total
            );
        }
        
        Ok(())
    }
    
    fn name(&self) -> &str {
        "console"
    }
}
