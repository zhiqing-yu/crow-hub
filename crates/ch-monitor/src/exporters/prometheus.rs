//! Prometheus exporter

use super::Exporter;
use crate::{MetricsSnapshot, Result};
use async_trait::async_trait;

/// Prometheus metrics exporter
pub struct PrometheusExporter {
    port: u16,
}

impl PrometheusExporter {
    /// Create a new Prometheus exporter
    pub fn new(port: u16) -> Self {
        Self { port }
    }
    
    /// Start the HTTP server
    pub async fn start(&self) -> Result<()> {
        // In real implementation, start an HTTP server
        // that exposes /metrics endpoint in Prometheus format
        Ok(())
    }
}

#[async_trait]
impl Exporter for PrometheusExporter {
    async fn export(&self, snapshot: &MetricsSnapshot) -> Result<()> {
        // Format and expose metrics for Prometheus scraping
        let _output = format_metrics(snapshot);
        Ok(())
    }
    
    fn name(&self) -> &str {
        "prometheus"
    }
}

/// Format metrics in Prometheus exposition format
fn format_metrics(snapshot: &MetricsSnapshot) -> String {
    let mut output = String::new();
    
    // Add header
    output.push_str("# Crow Hub Metrics\n");
    output.push_str(&format!("# Timestamp: {}\n\n", snapshot.timestamp));
    
    // System metrics
    output.push_str("# HELP agenthub_agents_total Total number of agents\n");
    output.push_str("# TYPE agenthub_agents_total gauge\n");
    output.push_str(&format!("agenthub_agents_total {}\n\n", snapshot.system.total_agents));
    
    output.push_str("# HELP agenthub_tokens_total Total tokens used\n");
    output.push_str("# TYPE agenthub_tokens_total counter\n");
    output.push_str(&format!("agenthub_tokens_total {}\n\n", snapshot.system.total_tokens));
    
    output.push_str("# HELP agenthub_cost_usd_total Total cost in USD\n");
    output.push_str("# TYPE agenthub_cost_usd_total counter\n");
    output.push_str(&format!("agenthub_cost_usd_total {:.6}\n\n", snapshot.system.total_cost_usd));
    
    // Per-agent metrics
    for agent in &snapshot.agents {
        let labels = format!(r#"agent_id="{}",agent_name="{}",adapter="{}""#, 
            agent.agent_id, agent.agent_name, agent.adapter_type);
        
        output.push_str(&format!("agenthub_agent_tokens_total{{{}}} {}\n", 
            labels, agent.tokens.total_tokens));
        output.push_str(&format!("agenthub_agent_cost_usd{{{}}} {:.6}\n", 
            labels, agent.tokens.cost_usd));
        output.push_str(&format!("agenthub_agent_requests_total{{{}}} {}\n", 
            labels, agent.requests_total));
        output.push_str(&format!("agenthub_agent_errors_total{{{}}} {}\n", 
            labels, agent.errors_total));
    }
    
    output
}
