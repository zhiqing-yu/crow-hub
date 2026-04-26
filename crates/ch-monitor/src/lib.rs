//! Crow Hub Monitoring System
//!
//! Provides comprehensive monitoring and metrics collection
//! for agents, including token usage, performance, and resource metrics.

use ch_protocol::{TokenMetrics, PerformanceMetrics, ResourceMetrics};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info};

pub mod collectors;
pub mod exporters;

/// Monitor error types
#[derive(Error, Debug, Clone)]
pub enum MonitorError {
    #[error("Collection error: {0}")]
    Collection(String),
    
    #[error("Export error: {0}")]
    Export(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
}

/// Result type for monitor operations
pub type Result<T> = std::result::Result<T, MonitorError>;

/// Metrics snapshot for a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: DateTime<Utc>,
    pub agents: Vec<AgentMetrics>,
    pub system: SystemMetrics,
}

/// Agent-specific metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetrics {
    pub agent_id: String,
    pub agent_name: String,
    pub adapter_type: String,
    pub timestamp: DateTime<Utc>,
    pub tokens: TokenMetrics,
    pub performance: PerformanceMetrics,
    pub resources: ResourceMetrics,
    pub requests_total: u64,
    pub errors_total: u64,
    pub latency_avg_ms: f64,
}

/// System-wide metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub total_agents: usize,
    pub active_agents: usize,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub requests_per_second: f64,
    pub cpu_usage_percent: f32,
    pub memory_usage_mb: u64,
}

/// Cost configuration for token pricing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    pub input_price_per_1k: f64,
    pub output_price_per_1k: f64,
    pub currency: String,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            input_price_per_1k: 0.01,
            output_price_per_1k: 0.03,
            currency: "USD".to_string(),
        }
    }
}

/// Monitor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub enabled: bool,
    pub export_interval_seconds: u64,
    pub retention_hours: u64,
    pub prometheus_enabled: bool,
    pub prometheus_port: u16,
    pub cost_config: CostConfig,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            export_interval_seconds: 60,
            retention_hours: 24,
            prometheus_enabled: true,
            prometheus_port: 9090,
            cost_config: CostConfig::default(),
        }
    }
}

/// Historical metrics entry
#[derive(Debug, Clone)]
struct HistoricalEntry {
    timestamp: DateTime<Utc>,
    metrics: AgentMetrics,
}

/// Metrics history for an agent
struct AgentMetricsHistory {
    entries: RwLock<Vec<HistoricalEntry>>,
    max_size: usize,
}

impl AgentMetricsHistory {
    fn new(max_size: usize) -> Self {
        Self {
            entries: RwLock::new(Vec::with_capacity(max_size)),
            max_size,
        }
    }
    
    fn add(&self, metrics: AgentMetrics) {
        let mut entries = self.entries.write();
        entries.push(HistoricalEntry {
            timestamp: metrics.timestamp,
            metrics,
        });
        
        // Trim old entries
        if entries.len() > self.max_size {
            entries.remove(0);
        }
    }
    
    fn get_recent(&self, count: usize) -> Vec<AgentMetrics> {
        let entries = self.entries.read();
        entries.iter().rev().take(count).map(|e| e.metrics.clone()).collect()
    }
    
    fn get_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<AgentMetrics> {
        let entries = self.entries.read();
        entries.iter()
            .filter(|e| e.timestamp >= start && e.timestamp <= end)
            .map(|e| e.metrics.clone())
            .collect()
    }
}

/// Main monitor struct
pub struct Monitor {
    config: MonitorConfig,
    current_metrics: DashMap<String, AgentMetrics>,
    history: DashMap<String, AgentMetricsHistory>,
    running: RwLock<bool>,
}

impl Monitor {
    /// Create a new monitor
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            config,
            current_metrics: DashMap::new(),
            history: DashMap::new(),
            running: RwLock::new(false),
        }
    }
    
    /// Start the monitor
    pub async fn start(&self) -> Result<()> {
        let mut running = self.running.write();
        *running = true;
        
        info!("Monitor started with {}s export interval", self.config.export_interval_seconds);
        Ok(())
    }
    
    /// Stop the monitor
    pub async fn stop(&self) -> Result<()> {
        let mut running = self.running.write();
        *running = false;
        
        info!("Monitor stopped");
        Ok(())
    }
    
    /// Check if monitor is running
    pub fn is_running(&self) -> bool {
        *self.running.read()
    }
    
    /// Record token usage
    pub async fn record_tokens(
        &self,
        agent_id: &str,
        agent_name: &str,
        adapter_type: &str,
        input_tokens: u64,
        output_tokens: u64,
    ) -> Result<()> {
        let total_tokens = input_tokens + output_tokens;
        
        // Calculate cost
        let input_cost = (input_tokens as f64 / 1000.0) * self.config.cost_config.input_price_per_1k;
        let output_cost = (output_tokens as f64 / 1000.0) * self.config.cost_config.output_price_per_1k;
        let total_cost = input_cost + output_cost;
        
        let metrics = AgentMetrics {
            agent_id: agent_id.to_string(),
            agent_name: agent_name.to_string(),
            adapter_type: adapter_type.to_string(),
            timestamp: Utc::now(),
            tokens: TokenMetrics {
                input_tokens,
                output_tokens,
                total_tokens,
                tokens_per_second: 0.0,
                cost_usd: total_cost,
            },
            performance: PerformanceMetrics {
                ttft_ms: 0,
                throughput_tps: 0.0,
                latency_p50_ms: 0,
                latency_p99_ms: 0,
            },
            resources: ResourceMetrics {
                cpu_usage_percent: 0.0,
                memory_usage_mb: 0,
                gpu_usage_percent: None,
                gpu_memory_usage_mb: None,
                kv_cache_usage: None,
            },
            requests_total: 1,
            errors_total: 0,
            latency_avg_ms: 0.0,
        };
        
        self.update_metrics(agent_id.to_string(), metrics);
        Ok(())
    }
    
    /// Record performance metrics
    pub async fn record_performance(
        &self,
        agent_id: &str,
        ttft_ms: u32,
        throughput_tps: f64,
        latency_p50_ms: u32,
        latency_p99_ms: u32,
    ) -> Result<()> {
        if let Some(mut metrics) = self.current_metrics.get_mut(agent_id) {
            metrics.performance = PerformanceMetrics {
                ttft_ms,
                throughput_tps,
                latency_p50_ms,
                latency_p99_ms,
            };
        }
        Ok(())
    }
    
    /// Record resource metrics
    pub async fn record_resources(
        &self,
        agent_id: &str,
        cpu_usage: f32,
        memory_usage_mb: u64,
        gpu_usage: Option<f32>,
        gpu_memory: Option<u64>,
    ) -> Result<()> {
        if let Some(mut metrics) = self.current_metrics.get_mut(agent_id) {
            metrics.resources = ResourceMetrics {
                cpu_usage_percent: cpu_usage,
                memory_usage_mb,
                gpu_usage_percent: gpu_usage,
                gpu_memory_usage_mb: gpu_memory,
                kv_cache_usage: None,
            };
        }
        Ok(())
    }
    
    /// Record request completion
    pub async fn record_request(
        &self,
        agent_id: &str,
        latency_ms: f64,
        success: bool,
    ) -> Result<()> {
        if let Some(mut metrics) = self.current_metrics.get_mut(agent_id) {
            metrics.requests_total += 1;
            if !success {
                metrics.errors_total += 1;
            }
            
            // Update average latency using exponential moving average
            let alpha = 0.3;
            metrics.latency_avg_ms = alpha * latency_ms + (1.0 - alpha) * metrics.latency_avg_ms;
        }
        Ok(())
    }
    
    /// Get current metrics for an agent
    pub fn get_agent_metrics(&self, agent_id: &str) -> Option<AgentMetrics> {
        self.current_metrics.get(agent_id).map(|m| m.clone())
    }
    
    /// Get all current metrics
    pub fn get_all_metrics(&self) -> Vec<AgentMetrics> {
        self.current_metrics.iter().map(|m| m.clone()).collect()
    }
    
    /// Get metrics snapshot
    pub fn get_snapshot(&self) -> MetricsSnapshot {
        let agents = self.get_all_metrics();
        
        let total_tokens: u64 = agents.iter().map(|a| a.tokens.total_tokens).sum();
        let total_cost: f64 = agents.iter().map(|a| a.tokens.cost_usd).sum();
        
        MetricsSnapshot {
            timestamp: Utc::now(),
            agents,
            system: SystemMetrics {
                total_agents: self.current_metrics.len(),
                active_agents: self.current_metrics.iter().filter(|a| a.requests_total > 0).count(),
                total_tokens,
                total_cost_usd: total_cost,
                requests_per_second: 0.0, // Would need to calculate from history
                cpu_usage_percent: 0.0,
                memory_usage_mb: 0,
            },
        }
    }
    
    /// Get historical metrics for an agent
    pub fn get_agent_history(&self, agent_id: &str, count: usize) -> Vec<AgentMetrics> {
        self.history.get(agent_id)
            .map(|h| h.get_recent(count))
            .unwrap_or_default()
    }
    
    /// Remove an agent's metrics
    pub fn remove_agent(&self, agent_id: &str) {
        self.current_metrics.remove(agent_id);
        self.history.remove(agent_id);
    }
    
    /// Update metrics and history
    fn update_metrics(&self, agent_id: String, metrics: AgentMetrics) {
        // Ensure history exists
        self.history.entry(agent_id.clone())
            .or_insert_with(|| AgentMetricsHistory::new(1000));
        
        // Add to history
        if let Some(history) = self.history.get(&agent_id) {
            history.add(metrics.clone());
        }
        
        // Update current
        self.current_metrics.insert(agent_id, metrics);
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new(MonitorConfig::default())
    }
}

/// Metrics exporter trait
#[async_trait::async_trait]
pub trait MetricsExporter: Send + Sync {
    /// Export metrics
    async fn export(&self, snapshot: &MetricsSnapshot) -> Result<()>;
    
    /// Get exporter name
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_monitor_tokens() {
        let monitor = Monitor::new(MonitorConfig::default());
        
        monitor.record_tokens(
            "agent-1",
            "Test Agent",
            "claude",
            1000,
            500,
        ).await.unwrap();
        
        let metrics = monitor.get_agent_metrics("agent-1").unwrap();
        assert_eq!(metrics.tokens.input_tokens, 1000);
        assert_eq!(metrics.tokens.output_tokens, 500);
        assert!(metrics.tokens.cost_usd > 0.0);
    }

    #[tokio::test]
    async fn test_monitor_snapshot() {
        let monitor = Monitor::new(MonitorConfig::default());
        
        monitor.record_tokens("agent-1", "Agent 1", "claude", 1000, 500).await.unwrap();
        monitor.record_tokens("agent-2", "Agent 2", "kimi", 2000, 1000).await.unwrap();
        
        let snapshot = monitor.get_snapshot();
        assert_eq!(snapshot.agents.len(), 2);
        assert_eq!(snapshot.system.total_agents, 2);
    }
}
