//! System metrics collector

use super::Collector;
use crate::{AgentMetrics, Result};
use async_trait::async_trait;

/// System metrics collector
pub struct SystemCollector;

impl SystemCollector {
    /// Create a new system collector
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Collector for SystemCollector {
    async fn collect(&self) -> Result<AgentMetrics> {
        // In a real implementation, this would use sysinfo or similar
        // to collect actual system metrics
        
        Ok(AgentMetrics {
            agent_id: "system".to_string(),
            agent_name: "System".to_string(),
            adapter_type: "system".to_string(),
            timestamp: chrono::Utc::now(),
            tokens: ch_protocol::TokenMetrics {
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                tokens_per_second: 0.0,
                cost_usd: 0.0,
            },
            performance: ch_protocol::PerformanceMetrics {
                ttft_ms: 0,
                throughput_tps: 0.0,
                latency_p50_ms: 0,
                latency_p99_ms: 0,
            },
            resources: ch_protocol::ResourceMetrics {
                cpu_usage_percent: 0.0,
                memory_usage_mb: 0,
                gpu_usage_percent: None,
                gpu_memory_usage_mb: None,
                kv_cache_usage: None,
            },
            requests_total: 0,
            errors_total: 0,
            latency_avg_ms: 0.0,
        })
    }
    
    fn name(&self) -> &str {
        "system"
    }
}
