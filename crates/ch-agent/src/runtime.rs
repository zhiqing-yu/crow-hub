//! Agent Runtime
//!
//! Manages the lifecycle of all loaded agents, connecting them
//! to the MessageBus and ModelRouter.

use crate::drivers::{AgentDriver, APIDriver, SubprocessDriver, TmuxDriver};
use crate::loader::{LoadedPlugin, PluginLoader};
use crate::manifest::DriverType;
use crate::{AgentActivity, AgentError, AgentInfo, AgentState, Result};
use ch_core::{ChannelVisibility, MessageBus};
use ch_model::{ChatRequest, ChatStreamChunk, ModelRouter};
use ch_protocol::{AgentAddress, AgentId, AgentMessage, MessageType, Payload};
use chrono::Utc;
use dashmap::DashMap;
use futures::stream::BoxStream;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// Manages all loaded agents
pub struct AgentRuntime {
    /// Loaded agents: name → agent entry
    agents: DashMap<String, AgentEntry>,
    /// Agent name → AgentId mapping for bus addressing
    agent_ids: DashMap<String, AgentId>,
    /// Live per-agent activity (idle / thinking / errored).
    /// Wrapped in Arc so the per-agent message-handler tasks can update it
    /// without holding a reference to `self`.
    activities: Arc<DashMap<String, AgentActivity>>,
    /// Model router (shared)
    router: Arc<ModelRouter>,
    /// Message bus (shared)
    bus: Arc<MessageBus>,
    /// Plugins directory
    plugins_dir: PathBuf,
}

/// An agent entry in the runtime
struct AgentEntry {
    /// Stable bus identity for this agent
    agent_id: AgentId,
    /// The driver for this agent (Arc for sharing with message handler task)
    driver: Arc<dyn AgentDriver>,
    /// Agent info / state
    info: AgentInfo,
    /// Original manifest
    plugin: LoadedPlugin,
}

impl AgentRuntime {
    /// Create a new runtime
    pub fn new(
        router: Arc<ModelRouter>,
        bus: Arc<MessageBus>,
        plugins_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            agents: DashMap::new(),
            agent_ids: DashMap::new(),
            activities: Arc::new(DashMap::new()),
            router,
            bus,
            plugins_dir: plugins_dir.into(),
        }
    }

    /// Load all plugins from the plugins directory
    pub async fn load_all(&self) -> Result<Vec<String>> {
        let loader = PluginLoader::new(&self.plugins_dir);
        let plugins = loader.scan()?;
        let mut loaded_names = Vec::new();

        for plugin in plugins {
            match self.load_plugin(plugin).await {
                Ok(name) => loaded_names.push(name),
                Err(e) => warn!("Failed to load plugin: {}", e),
            }
        }

        info!("Loaded {} agent(s) total", loaded_names.len());
        Ok(loaded_names)
    }

    /// Load a single plugin into the runtime
    pub async fn load_plugin(&self, plugin: LoadedPlugin) -> Result<String> {
        let name = plugin.name().to_string();
        let manifest = &plugin.manifest;

        // Create the appropriate driver (Arc for sharing with message handler)
        let driver: Arc<dyn AgentDriver> = match manifest.agent.driver {
            DriverType::Api => {
                Arc::new(APIDriver::from_manifest(manifest, self.router.clone())?)
            }
            DriverType::Subprocess => {
                let sub_config = manifest.subprocess.as_ref()
                    .ok_or_else(|| AgentError::Manifest(
                        "Subprocess driver requires [subprocess] section".to_string()
                    ))?;
                Arc::new(SubprocessDriver::new(&name, sub_config.clone()))
            }
            DriverType::Tmux => {
                let tmux_config = manifest.tmux.as_ref()
                    .ok_or_else(|| AgentError::Manifest(
                        "Tmux driver requires [tmux] section".to_string()
                    ))?;
                Arc::new(TmuxDriver::new(&name, tmux_config.clone()))
            }
            DriverType::Mcp => {
                return Err(AgentError::Driver("MCP driver not yet implemented".to_string()));
            }
        };

        // Determine model info
        let (model_backend, default_model) = if let Some(ref model) = manifest.model {
            (model.backend.clone(), Some(model.default.clone()))
        } else {
            (None, None)
        };

        // Assign a stable AgentId and subscribe to the message bus
        let agent_id = AgentId::new();
        let agent_rx = self.bus.subscribe(agent_id).await;

        // Auto-join channels (actually join, not just create)
        let mut channels = Vec::new();
        if let Some(ref ch_config) = manifest.channels {
            for channel_name in &ch_config.auto_join {
                let _ = self.bus.create_channel(channel_name);
                if let Err(e) = self.bus.join_channel(channel_name, agent_id, ChannelVisibility::Full) {
                    warn!("Agent '{}' failed to join #{}: {}", name, channel_name, e);
                }
                channels.push(channel_name.clone());
            }
        }

        let driver_type_str = driver.driver_type().to_string();
        let info = AgentInfo {
            name: name.clone(),
            state: AgentState::Ready,
            driver_type: driver_type_str,
            model_backend,
            default_model: default_model.clone(),
            loaded_at: Utc::now(),
            channels: channels.clone(),
            description: manifest.agent.description.clone(),
        };

        info!(
            "✓ Loaded agent '{}' (driver: {}, model: {}, bus: {})",
            name,
            info.driver_type,
            info.default_model.as_deref().unwrap_or("none"),
            agent_id,
        );

        self.agent_ids.insert(name.clone(), agent_id);
        self.agents.insert(name.clone(), AgentEntry {
            agent_id,
            driver: driver.clone(),
            info,
            plugin,
        });
        self.activities.insert(name.clone(), AgentActivity::Unknown);

        // Spawn per-agent message handler: listens on bus, processes
        // addressed messages through the driver, publishes responses back.
        let bus = self.bus.clone();
        let agent_name = name.clone();
        let default_model_for_task = default_model.unwrap_or_else(|| "default".to_string());
        let handler_channels = channels;
        let activities = self.activities.clone();

        tokio::spawn(async move {
            use futures::stream::StreamExt;
            let mut rx = agent_rx;
            while let Some(msg) = rx.recv().await {
                // Only process messages addressed to this agent
                let addressed_to_me = match &msg.to {
                    Some(addr) => addr.agent_id == agent_id,
                    None => false, // Don't auto-respond to broadcasts
                };
                if !addressed_to_me {
                    continue;
                }

                // Extract text prompt
                let prompt = match &msg.payload {
                    Payload::Text(text) => text.clone(),
                    _ => continue,
                };

                // Clone the driver Arc (don't hold any DashMap guard across await)
                let driver = driver.clone();
                let model = default_model_for_task.clone();
                let correlation_id = msg.message_id;

                let from_addr = AgentAddress {
                    agent_id,
                    agent_name: agent_name.clone(),
                    adapter_type: "agent".to_string(),
                };
                let channel = handler_channels
                    .first()
                    .map(|c| c.as_str())
                    .unwrap_or("general");

                // Mark this agent as Thinking and start the latency timer.
                // The latency we track is "time-to-first-chunk", not total
                // response time, since for chunked streams the user gets
                // visible feedback at the first chunk.
                let send_started = std::time::Instant::now();
                activities.insert(
                    agent_name.clone(),
                    AgentActivity::Thinking { since: Utc::now() },
                );

                // Stream the response through the bus so the TUI gets
                // incremental feedback.  Each non-empty chunk is published
                // as its own AgentMessage with the same correlation_id —
                // TUI's on_tick merges consecutive chunks from the same
                // agent by appending to the last message.
                match driver
                    .stream_chat(ChatRequest::simple(&model, &prompt))
                    .await
                {
                    Ok(mut stream) => {
                        let mut any_chunk_sent = false;
                        let mut first_chunk_latency_ms: Option<u64> = None;
                        let mut errored = false;
                        while let Some(chunk_res) = stream.next().await {
                            match chunk_res {
                                Ok(chunk) => {
                                    if chunk.content.is_empty() {
                                        continue;
                                    }
                                    if first_chunk_latency_ms.is_none() {
                                        first_chunk_latency_ms =
                                            Some(send_started.elapsed().as_millis() as u64);
                                    }
                                    any_chunk_sent = true;
                                    let chunk_msg = AgentMessage::new(
                                        from_addr.clone(),
                                        None,
                                        MessageType::TaskResponse,
                                        Payload::Text(chunk.content),
                                    )
                                    .with_correlation(correlation_id);
                                    if let Err(e) = bus
                                        .send_to_channel(channel, &agent_id, chunk_msg)
                                        .await
                                    {
                                        warn!(
                                            "[{}] Failed to publish chunk to bus: {}",
                                            agent_name, e
                                        );
                                        break;
                                    }
                                }
                                Err(e) => {
                                    warn!("[{}] Stream chunk error: {}", agent_name, e);
                                    let err_str = e.to_string();
                                    let err_msg = AgentMessage::new(
                                        from_addr.clone(),
                                        None,
                                        MessageType::TaskResponse,
                                        Payload::Text(format!("Error: {}", err_str)),
                                    )
                                    .with_correlation(correlation_id);
                                    let _ = bus
                                        .send_to_channel(channel, &agent_id, err_msg)
                                        .await;
                                    activities.insert(
                                        agent_name.clone(),
                                        AgentActivity::Errored { last_error: err_str },
                                    );
                                    any_chunk_sent = true;
                                    errored = true;
                                    break;
                                }
                            }
                        }

                        // If the stream completed without ever producing a
                        // chunk, send a placeholder so the user isn't left
                        // staring at silence.
                        if !any_chunk_sent {
                            warn!("[{}] Stream produced no chunks", agent_name);
                            let empty_msg = AgentMessage::new(
                                from_addr.clone(),
                                None,
                                MessageType::TaskResponse,
                                Payload::Text("(no response)".to_string()),
                            )
                            .with_correlation(correlation_id);
                            let _ = bus
                                .send_to_channel(channel, &agent_id, empty_msg)
                                .await;
                            activities.insert(
                                agent_name.clone(),
                                AgentActivity::Errored {
                                    last_error: "stream produced no chunks".to_string(),
                                },
                            );
                        } else if !errored {
                            activities.insert(
                                agent_name.clone(),
                                AgentActivity::Idle {
                                    last_latency_ms: first_chunk_latency_ms,
                                },
                            );
                        }
                    }
                    Err(e) => {
                        warn!("[{}] stream_chat failed to start: {}", agent_name, e);
                        let err_str = e.to_string();
                        let err_msg = AgentMessage::new(
                            from_addr.clone(),
                            None,
                            MessageType::TaskResponse,
                            Payload::Text(format!("Error: {}", err_str)),
                        )
                        .with_correlation(correlation_id);
                        let _ = bus.send_to_channel(channel, &agent_id, err_msg).await;
                        activities.insert(
                            agent_name.clone(),
                            AgentActivity::Errored { last_error: err_str },
                        );
                    }
                }
            }
            info!("Agent '{}' message handler stopped", agent_name);
        });

        Ok(name)
    }

    /// Send a chat message through a specific agent
    pub async fn chat(
        &self,
        agent_name: &str,
        request: ch_model::ChatRequest,
    ) -> Result<ch_model::ChatResponse> {
        let entry = self.agents.get(agent_name)
            .ok_or_else(|| AgentError::NotFound(format!("Agent '{}' not loaded", agent_name)))?;

        entry.driver.chat(request).await
    }

    /// Send a streaming chat message through a specific agent
    pub async fn stream_chat(
        &self,
        agent_name: &str,
        request: ch_model::ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatStreamChunk>>> {
        let entry = self.agents.get(agent_name)
            .ok_or_else(|| AgentError::NotFound(format!("Agent '{}' not loaded", agent_name)))?;

        entry.driver.stream_chat(request).await
    }

    /// Get info about all loaded agents
    pub fn list_agents(&self) -> Vec<AgentInfo> {
        self.agents.iter().map(|e| e.value().info.clone()).collect()
    }

    /// Get info about a specific agent
    pub fn get_agent_info(&self, name: &str) -> Option<AgentInfo> {
        self.agents.get(name).map(|e| e.value().info.clone())
    }

    /// Get the live per-request activity status for an agent.
    /// Returns `AgentActivity::Unknown` for agents that haven't been
    /// loaded or that have never received a request.
    pub fn activity_of(&self, name: &str) -> AgentActivity {
        self.activities
            .get(name)
            .map(|v| v.clone())
            .unwrap_or_default()
    }

    /// Get the AgentId for a loaded agent (for bus addressing)
    pub fn get_agent_id(&self, name: &str) -> Option<AgentId> {
        self.agent_ids.get(name).map(|id| *id)
    }

    /// Check if an agent is loaded
    pub fn has_agent(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }

    /// Get the number of loaded agents
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Stop a specific agent
    pub async fn stop_agent(&self, name: &str) -> Result<()> {
        if let Some(mut entry) = self.agents.get_mut(name) {
            entry.driver.stop().await?;
            entry.info.state = AgentState::Stopped;
            info!("Stopped agent '{}'", name);
            Ok(())
        } else {
            Err(AgentError::NotFound(format!("Agent '{}' not loaded", name)))
        }
    }

    /// Stop all agents
    pub async fn stop_all(&self) {
        for entry in self.agents.iter() {
            let name = entry.key().clone();
            if let Err(e) = entry.value().driver.stop().await {
                warn!("Error stopping agent '{}': {}", name, e);
            }
        }
        self.agents.clear();
        info!("All agents stopped");
    }

    /// Print a summary of loaded agents
    pub fn summary(&self) -> String {
        let mut lines = vec![format!("Agents: {} loaded", self.agent_count())];
        for entry in self.agents.iter() {
            let info = &entry.value().info;
            lines.push(format!(
                "  • {} ({}) model={} channels={:?}",
                info.name,
                info.driver_type,
                info.default_model.as_deref().unwrap_or("-"),
                info.channels,
            ));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ch_model::{ModelRegistry, backends::MockBackend, ChatRequest};
    use std::fs;

    async fn setup_test_env() -> (tempfile::TempDir, Arc<ModelRouter>, Arc<MessageBus>) {
        let tmp = tempfile::tempdir().unwrap();

        // Set up a mock model router
        let registry = Arc::new(ModelRegistry::new());
        let router = Arc::new(ModelRouter::new(registry));
        let bus = Arc::new(MessageBus::new());

        // Start the bus so subscribe/join work in load_plugin
        bus.start().await.unwrap();

        (tmp, router, bus)
    }

    #[tokio::test]
    async fn test_load_api_agent() {
        let (tmp, router, bus) = setup_test_env().await;

        // Register a mock backend
        let mock = MockBackend::new("test-backend")
            .with_models(vec!["test-model".to_string()])
            .with_default_response("Hello from test!");
        router.register_backend(Arc::new(mock)).await.unwrap();

        // Create plugin directory
        let agent_dir = tmp.path().join("agents").join("test-api");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(agent_dir.join("agent.toml"), r#"
[agent]
name = "test-api"
driver = "api"
description = "Test API agent"

[model]
backend = "test-backend"
default = "test-model"

[capabilities]
chat = true

[channels]
auto_join = ["general"]
"#).unwrap();

        let runtime = AgentRuntime::new(router, bus, tmp.path());
        let loaded = runtime.load_all().await.unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], "test-api");
        assert_eq!(runtime.agent_count(), 1);

        // Chat through the agent
        let req = ChatRequest::simple("test-model", "Hi");
        let resp = runtime.chat("test-api", req).await.unwrap();
        assert_eq!(resp.content, "Hello from test!");
    }

    #[tokio::test]
    async fn test_list_agents() {
        let (tmp, router, bus) = setup_test_env().await;

        let mock = MockBackend::new("b1")
            .with_models(vec!["m1".to_string()]);
        router.register_backend(Arc::new(mock)).await.unwrap();

        // Create two agents
        for name in &["agent-a", "agent-b"] {
            let dir = tmp.path().join("agents").join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("agent.toml"), format!(r#"
[agent]
name = "{}"
driver = "api"
description = "Test"
[model]
default = "m1"
"#, name)).unwrap();
        }

        let runtime = AgentRuntime::new(router, bus, tmp.path());
        runtime.load_all().await.unwrap();

        let agents = runtime.list_agents();
        assert_eq!(agents.len(), 2);
    }

    #[tokio::test]
    async fn test_agent_not_found() {
        let (_tmp, router, bus) = setup_test_env().await;
        let runtime = AgentRuntime::new(router, bus, PathBuf::from("."));
        let result = runtime.chat("nonexistent", ChatRequest::simple("m", "hi")).await;
        assert!(result.is_err());
    }
}
