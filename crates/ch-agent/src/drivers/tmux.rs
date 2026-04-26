//! Tmux Headless Driver
//!
//! Spawns CLI agents inside a headless tmux session so that TUIs
//! can operate visually while Crow Hub controls them via virtual
//! keystrokes and retrieves outputs over RPC logs.

use crate::drivers::AgentDriver;
use crate::manifest::{ShellType, TmuxSection};
use crate::{AgentError, Result};
use ch_model::{ChatRequest, ChatResponse, ChatStreamChunk, FinishReason, TokenUsage};
use futures::stream::{self, BoxStream, StreamExt};
use serde::Deserialize;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tracing::{info, debug, warn};

pub struct TmuxDriver {
    name: String,
    config: TmuxSection,
}

impl TmuxDriver {
    pub fn new(name: impl Into<String>, config: TmuxSection) -> Self {
        Self {
            name: name.into(),
            config,
        }
    }

    fn wrap_command(&self, base_cmd: &str, args: &[&str]) -> Command {
        match self.config.shell {
            ShellType::Native => {
                let mut cmd = Command::new(base_cmd);
                cmd.args(args);
                cmd
            }
            ShellType::Wsl => {
                let mut cmd = Command::new("wsl.exe");
                // Suppress Windows PATH translation noise (prevents "Failed to translate" spam)
                cmd.env("WSLENV", "PATH/l");
                
                if let Some(ref distro) = self.config.wsl_distro {
                    cmd.args(["-d", distro]);
                }
                cmd.arg("--");
                cmd.arg(base_cmd);
                cmd.args(args);
                cmd
            }
            ShellType::Ssh => {
                let mut cmd = Command::new("ssh");
                cmd.arg(base_cmd);
                cmd.args(args);
                cmd
            }
        }
    }

    /// Check if the tmux session is running
    async fn is_running(&self) -> bool {
        let mut cmd = self.wrap_command("tmux", &["has-session", "-t", &self.config.session_name]);
        let output = cmd.output().await;
        match output {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    fn get_value_by_path<'a>(&self, value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
        let mut current = value;
        for part in path.split('.') {
            if let Ok(idx) = part.parse::<usize>() {
                current = current.get(idx)?;
            } else {
                current = current.get(part)?;
            }
        }
        Some(current)
    }

    fn process_line(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();
        if trimmed.is_empty() { return None; }

        if let Some(filter) = &self.config.output_filter {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if let Some(extracted) = self.get_value_by_path(&val, filter) {
                    return match extracted {
                        serde_json::Value::String(s) => Some(s.clone()),
                        other => Some(other.to_string()),
                    };
                }
            }
        }

        // Fallback: try to see if it's standard OpenClaw RPC JSON
        #[derive(Deserialize)]
        struct PossibleJsonRPC {
            content: Option<String>,
            text: Option<String>,
        }
        if let Ok(parsed) = serde_json::from_str::<PossibleJsonRPC>(trimmed) {
            return parsed.content.or(parsed.text);
        }

        Some(trimmed.to_string())
    }
}

#[async_trait::async_trait]
impl AgentDriver for TmuxDriver {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let mut stream = self.stream_chat(request).await?;
        let mut full_content = String::new();
        let mut last_usage = TokenUsage::default();
        let mut last_reason = FinishReason::Stop;

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res?;
            full_content.push_str(&chunk.content);
            if chunk.is_final {
                if let Some(usage) = chunk.usage { last_usage = usage; }
                if let Some(reason) = chunk.finish_reason { last_reason = reason; }
            }
        }

        Ok(ChatResponse {
            content: full_content,
            model: "tmux".into(), // Will be updated by runtime
            backend: format!("tmux/{}", self.name),
            usage: last_usage,
            finish_reason: last_reason,
        })
    }

    async fn stream_chat(&self, request: ChatRequest) -> Result<BoxStream<'static, Result<ChatStreamChunk>>> {
        // 1. Ensure session is running
        if !self.is_running().await {
            info!("[{}] Starting tmux session '{}'", self.name, self.config.session_name);
            let mut start_cmd = self.wrap_command("tmux", &[
                "new-session", "-d", "-s", &self.config.session_name, &self.config.command
            ]);
            for arg in &self.config.args { start_cmd.arg(arg); }
            start_cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            let output = start_cmd.output().await
                .map_err(|e| AgentError::Driver(format!("[{}] Failed to start tmux: {}", self.name, e)))?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("[{}] tmux start stderr: {}", self.name, stderr);
                return Err(AgentError::Driver(format!(
                    "tmux session '{}' failed to start: {}",
                    self.config.session_name,
                    stderr.trim()
                )));
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }

        // 2. Extract prompt and send via tmux keystrokes
        let prompt = request.messages.last().map(|m| m.content.clone()).unwrap_or_default();
        if let Err(e) = self.wrap_command("tmux", &["send-keys", "-t", &self.config.session_name, "-l", &prompt]).status().await {
            warn!("[{}] send-keys failed: {}", self.name, e);
        }
        if let Err(e) = self.wrap_command("tmux", &["send-keys", "-t", &self.config.session_name, "Enter"]).status().await {
            warn!("[{}] send Enter failed: {}", self.name, e);
        }

        // 3. Setup log streaming
        if let Some(log_cmd_str) = &self.config.log_command {
            let parts: Vec<&str> = log_cmd_str.split_whitespace().collect();
            if parts.is_empty() { return Err(AgentError::Driver("Empty log_command".into())); }
            
            let mut log_cmd = self.wrap_command(parts[0], &parts[1..]);
            log_cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            
            let mut child = log_cmd.spawn()
                .map_err(|e| AgentError::Driver(format!("Failed to start log stream: {}", e)))?;
            
            let stdout = child.stdout.take().unwrap();
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            
            let driver_name = self.name.clone();
            let config = self.config.clone();
            
            // Create a custom stream from the lines
            let stream = stream::unfold(
                (lines, child),
                move |(mut lines, mut child)| {
                    let config_clone = config.clone();
                    async move {
                        match lines.next_line().await {
                            Ok(Some(line)) => {
                                // Extract content from line
                                if line.trim().starts_with("wsl:") {
                                    return Some((Ok(ChatStreamChunk { content: String::new(), is_final: false, finish_reason: None, usage: None }), (lines, child)));
                                }

                                let content = if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                    if let Some(filter) = &config_clone.output_filter {
                                        let mut path_val = &val;
                                        for part in filter.split('.') {
                                            path_val = path_val.get(part).unwrap_or(&serde_json::Value::Null);
                                        }
                                        path_val.as_str().map(|s| s.to_string()).unwrap_or(line)
                                    } else {
                                        // Default RPC extraction: try "content", then "text",
                                        // then fall through to raw line so we never silently
                                        // drop a valid JSON message that uses an unknown schema.
                                        val.get("content").and_then(|v| v.as_str()).map(|s| s.to_string())
                                            .or_else(|| val.get("text").and_then(|v| v.as_str()).map(|s| s.to_string()))
                                            .unwrap_or(line)
                                    }
                                } else {
                                    // Non-JSON line — treat as raw text after filtering
                                    // known terminal/WSL noise patterns.
                                    let trimmed = line.trim();
                                    let is_noise =
                                        trimmed.is_empty()
                                        || trimmed.contains("Failed to translate")
                                        || trimmed.starts_with("\x1b[")  // ANSI escape sequences
                                        || trimmed.starts_with("warning:")
                                        || trimmed.starts_with("wsl:");
                                    if is_noise { String::new() } else { trimmed.to_string() }
                                };

                                if content.is_empty() {
                                    return Some((Ok(ChatStreamChunk { content: String::new(), is_final: false, finish_reason: None, usage: None }), (lines, child)));
                                }

                                let chunk = ChatStreamChunk {
                                    content,
                                    is_final: false,
                                    finish_reason: None,
                                    usage: None,
                                };
                                Some((Ok(chunk), (lines, child)))
                            }
                            _ => {
                                // Stream closed or error
                                let _ = child.kill().await;
                                None
                            }
                        }
                    }
                }
            );
            
            return Ok(stream.boxed());
        }

        // Scraper fallback (simple single-chunk stream)
        let resp = self.chat(request).await?;
        Ok(stream::once(async move {
            Ok(ChatStreamChunk {
                content: resp.content,
                is_final: true,
                finish_reason: Some(resp.finish_reason),
                usage: Some(resp.usage),
            })
        }).boxed())
    }

    async fn health_check(&self) -> Result<bool> {
        Ok(self.is_running().await)
    }

    fn driver_type(&self) -> &str {
        "tmux"
    }

    async fn stop(&self) -> Result<()> {
        if self.is_running().await {
            let mut cmd = self.wrap_command("tmux", &["kill-session", "-t", &self.config.session_name]);
            let _ = cmd.status().await;
            info!("Stopped tmux session '{}'", self.config.session_name);
        }
        Ok(())
    }
}
