//! Subprocess Driver
//!
//! Spawns CLI agent tools as subprocesses and communicates
//! via stdin/stdout using JSON-line protocol.
//!
//! Supports three shell types:
//! - Native: direct process spawn on Windows
//! - WSL: runs via `wsl.exe -d <distro> -- <command>`
//! - SSH: runs via `ssh <user>@<host> <command>`

use crate::drivers::AgentDriver;
use crate::manifest::{ShellType, SubprocessSection, SubprocessInputMode, SubprocessOutputMode};
use crate::{AgentError, Result};
use ch_model::{ChatRequest, ChatResponse, ChatStreamChunk, ChatRole, FinishReason, TokenUsage};
use futures::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Driver that spawns CLI agents as subprocesses
pub struct SubprocessDriver {
    /// Agent name
    name: String,
    /// Subprocess configuration
    config: SubprocessSection,
    /// Running child process (if started)
    child: Mutex<Option<Child>>,
}

impl SubprocessDriver {
    /// Create a new subprocess driver
    pub fn new(name: impl Into<String>, config: SubprocessSection) -> Self {
        Self {
            name: name.into(),
            config,
            child: Mutex::new(None),
        }
    }

    /// Build the command based on shell type
    fn build_command(&self) -> Command {
        match self.config.shell {
            ShellType::Native => {
                let mut cmd = Command::new(&self.config.command);
                cmd.args(&self.config.args);
                if let Some(ref dir) = self.config.working_dir {
                    cmd.current_dir(dir);
                }
                for (k, v) in &self.config.env {
                    cmd.env(k, v);
                }
                cmd
            }
            ShellType::Wsl => {
                let mut cmd = Command::new("wsl.exe");
                // Avoid polluting WSL path with Windows paths by clearing WSLENV
                cmd.env_remove("WSLENV");

                if let Some(ref distro) = self.config.wsl_distro {
                    cmd.args(["-d", distro]);
                }
                
                // Wrap in a login shell so NVM/Node paths from ~/.bashrc are available
                cmd.args(["-e", "bash", "-lc", "exec \"$@\"", "bash"]);
                
                cmd.arg(&self.config.command);
                cmd.args(&self.config.args);
                cmd
            }
            ShellType::Ssh => {
                let mut cmd = Command::new("ssh");

                // Add key if specified
                if let Some(ref key) = self.config.ssh_key {
                    cmd.args(["-i", key]);
                }

                // Build user@host
                let host = self.config.ssh_host.as_deref().unwrap_or("localhost");
                let target = if let Some(ref user) = self.config.ssh_user {
                    format!("{}@{}", user, host)
                } else {
                    host.to_string()
                };
                cmd.arg(&target);

                // The remote command
                cmd.arg(&self.config.command);
                cmd.args(&self.config.args);
                cmd
            }
        }
    }

    /// Start the subprocess
    pub async fn start(&self) -> Result<()> {
        let mut cmd = self.build_command();
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        info!(
            "Starting subprocess agent '{}': {:?} (shell: {:?})",
            self.name, self.config.command, self.config.shell
        );

        let child = cmd.spawn()
            .map_err(|e| AgentError::Driver(format!(
                "Failed to spawn '{}': {}. Shell: {:?}",
                self.config.command, e, self.config.shell
            )))?;

        *self.child.lock().await = Some(child);
        info!("Subprocess agent '{}' started", self.name);
        Ok(())
    }

    /// Helper to resolve a JSON value from a path string like "a.b.c"
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

    fn process_output(&self, raw: &str) -> String {
        // Strip ANSI escape codes
        let ansi_escape = regex::Regex::new(r"\x1b\[[0-9;]*[mK]").unwrap();
        let clean_raw = ansi_escape.replace_all(raw, "").to_string();
        let trimmed = clean_raw.trim();
        
        // If OutputMode is Json, try to parse and filter
        if self.config.output_mode == SubprocessOutputMode::Json {
            // Find the outermost JSON block (first { to last })
            let json_str = if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
                if start <= end {
                    Some(&trimmed[start..=end])
                } else {
                    None
                }
            } else {
                Some(trimmed)
            };

            if let Some(jstr) = json_str {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(jstr) {
                    // 1. Try explicit filter
                    if let Some(filter) = &self.config.output_filter {
                        if let Some(extracted) = self.get_value_by_path(&val, filter) {
                            return match extracted {
                                serde_json::Value::String(s) => s.clone(),
                                other => other.to_string(),
                            };
                        }
                    } 
                    
                    // 2. Try common default fields if filter failed or was absent
                    if let Some(s) = val.get("content").and_then(|v| v.as_str()) {
                        return s.to_string();
                    }
                    if let Some(s) = val.get("text").and_then(|v| v.as_str()) {
                        return s.to_string();
                    }
                    if let Some(payloads) = val.get("payloads").and_then(|v| v.as_array()) {
                        if let Some(first) = payloads.first() {
                            if let Some(text) = first.get("text").and_then(|v| v.as_str()) {
                                return text.to_string();
                            }
                        }
                    }
                    
                    // 3. Fallback to SubprocessResponse format
                    if let Ok(resp) = serde_json::from_value::<SubprocessResponse>(val.clone()) {
                        return resp.content;
                    }
                }
            }
        }
        
        // Fallback or Raw mode
        trimmed.to_string()
    }
}

#[async_trait::async_trait]
impl AgentDriver for SubprocessDriver {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let prompt = request.messages.last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // If we are in Argv mode, we spawn a fresh process for this message
        if self.config.input_mode == SubprocessInputMode::Argv {
            let mut cmd = self.build_command();
            cmd.arg(&prompt);
            cmd.stdin(Stdio::null());

            let output = cmd.output().await
                .map_err(|e| AgentError::Driver(format!(
                    "Failed to execute '{}' (shell={:?}): {}",
                    self.config.command, self.config.shell, e
                )))?;

            let raw_stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let raw_stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            // Log stderr for debugging (always, even on success) but filter WSL noise
            let filtered_stderr = raw_stderr.lines()
                .filter(|line| {
                    let trimmed = line.trim();
                    !trimmed.starts_with("wsl: ") && !trimmed.starts_with("wsl:")
                })
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string();

            if !filtered_stderr.is_empty() {
                // Truncate stderr warning to avoid corrupting the TUI with massive JSON payloads
                if filtered_stderr.len() > 300 {
                    warn!("[{}] stderr (truncated): {}...", self.name, &filtered_stderr[..300]);
                } else {
                    warn!("[{}] stderr: {}", self.name, filtered_stderr);
                }
            }

            // Combine stdout and filtered stderr for JSON (as JSON agents might write payload to stderr)
            let combined_output = if self.config.output_mode == SubprocessOutputMode::Json {
                format!("{}\n{}", raw_stdout, filtered_stderr)
            } else {
                raw_stdout.clone()
            };

            // On any non-zero exit, surface full diagnostic info even if some
            // stdout exists.  Before, we only errored when combined_output was
            // empty, which made every CLI misconfiguration look identical as
            // "Agent process failed (exit code 1)" with no stderr detail.
            if !output.status.success() {
                let exit_info = output.status.code()
                    .map(|c| format!("exit {}", c))
                    .unwrap_or_else(|| "killed by signal".to_string());
                let invocation = format!(
                    "{} {}",
                    self.config.command,
                    self.config.args.join(" ")
                );
                let stdout_snip = if raw_stdout.is_empty() { "(empty)".to_string() } else { raw_stdout.clone() };
                let stderr_snip = if filtered_stderr.is_empty() { "(empty)".to_string() } else { filtered_stderr.clone() };
                return Err(AgentError::Driver(format!(
                    "Agent '{}' failed ({}) — invocation: `{}` (shell={:?})\n  stdout: {}\n  stderr: {}",
                    self.name, exit_info, invocation, self.config.shell,
                    stdout_snip, stderr_snip
                )));
            }

            let content = self.process_output(&combined_output);

            // Even on clean exit, if content is empty and stderr had something,
            // fold that into the response so the user sees it rather than a
            // silent "(no response)".
            if content.trim().is_empty() && !filtered_stderr.is_empty() {
                return Ok(ChatResponse {
                    content: format!("(agent produced no stdout; stderr: {})", filtered_stderr),
                    model: request.model,
                    backend: format!("subprocess/{}", self.name),
                    usage: TokenUsage::default(),
                    finish_reason: FinishReason::Stop,
                });
            }

            return Ok(ChatResponse {
                content,
                model: request.model,
                backend: format!("subprocess/{}", self.name),
                usage: TokenUsage::default(),
                finish_reason: FinishReason::Stop,
            });
        }

        // Persistent process modes (Json/Plain)
        let mut child_lock = self.child.lock().await;
        if child_lock.is_none() {
            drop(child_lock);
            self.start().await?;
            child_lock = self.child.lock().await;
        }

        let child = child_lock.as_mut()
            .ok_or_else(|| AgentError::Driver("Subprocess not running".to_string()))?;

        // Write based on input mode
        if let Some(ref mut stdin) = child.stdin {
            let payload = match self.config.input_mode {
                SubprocessInputMode::Json => {
                    let req = SubprocessRequest {
                        model: request.model.clone(),
                        messages: request.messages.iter().map(|m| SubprocessMessage {
                            role: format!("{:?}", m.role).to_lowercase(),
                            content: m.content.clone(),
                        }).collect(),
                    };
                    serde_json::to_string(&req).map_err(|e| AgentError::Driver(e.to_string()))?
                }
                SubprocessInputMode::Plain => format!("{}\n", prompt),
                SubprocessInputMode::Argv => unreachable!(),
            };

            stdin.write_all(payload.as_bytes()).await
                .map_err(|e| AgentError::Driver(format!("stdin write failed: {}", e)))?;
            stdin.flush().await
                .map_err(|e| AgentError::Driver(format!("stdin flush failed: {}", e)))?;
        }

        // Close stdin so the child knows input is complete and flushes its
        // response buffer.  This is critical for agents like Kimi that wait
        // for EOF before emitting output.
        drop(child.stdin.take());

        // Read response — chunk-based accumulation instead of a single
        // read_line, because many agents emit multi-line output or flush
        // in arbitrary chunks.
        let stdout = child.stdout.take()
            .ok_or(AgentError::Driver("stdout unavailable".into()))?;
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::with_capacity(4096);

        let read_result = tokio::time::timeout(
            std::time::Duration::from_secs(120),
            async {
                // Read until EOF (child closes its stdout)
                tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut buf).await
            },
        ).await;

        match read_result {
            Ok(Ok(_)) => {
                let raw = String::from_utf8_lossy(&buf).to_string();
                let content = self.process_output(&raw);
                Ok(ChatResponse {
                    content,
                    model: request.model,
                    backend: format!("subprocess/{}", self.name),
                    usage: TokenUsage::default(),
                    finish_reason: FinishReason::Stop,
                })
            }
            Ok(Err(e)) => Err(AgentError::Driver(format!("stdout read error: {}", e))),
            Err(_) => Err(AgentError::Driver("Subprocess response timed out after 120s".into())),
        }
    }

    async fn stream_chat(&self, request: ChatRequest) -> Result<BoxStream<'static, Result<ChatStreamChunk>>> {
        let prompt = request.messages.last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Argv mode: spawn a fresh process and stream stdout line-by-line
        // Note: Structured JSON outputs are often multi-line, which breaks line-by-line parsing.
        // We fall through to `chat()` for Json output mode to accumulate and parse correctly.
        if self.config.input_mode == SubprocessInputMode::Argv && self.config.output_mode != SubprocessOutputMode::Json {
            let mut cmd = self.build_command();
            cmd.arg(&prompt);
            cmd.stdin(Stdio::null());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());

            let mut child = cmd.spawn()
                .map_err(|e| AgentError::Driver(format!("Failed to spawn '{}': {}", self.config.command, e)))?;

            // Drain stderr in background so the pipe buffer never fills
            if let Some(stderr) = child.stderr.take() {
                let agent_name = self.name.clone();
                tokio::spawn(async move {
                    let reader = BufReader::new(stderr);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        let t = line.trim().to_string();
                        if !t.is_empty() && !t.starts_with("wsl: ") && !t.starts_with("wsl:") {
                            warn!("[{}] stderr: {}", agent_name, t);
                        }
                    }
                });
            }

            let stdout = child.stdout.take()
                .ok_or(AgentError::Driver("stdout unavailable".into()))?;
            let lines = BufReader::new(stdout).lines();
            let config = self.config.clone();

            let stream = stream::unfold(
                (lines, child),
                move |(mut lines, mut child)| {
                    let cfg = config.clone();
                    async move {
                        match lines.next_line().await {
                            Ok(Some(line)) => {
                                let trimmed = line.trim();

                                // Skip noise lines
                                let is_noise = trimmed.is_empty()
                                    || trimmed.contains("Failed to translate")
                                    || trimmed.starts_with("\x1b[")
                                    || trimmed.starts_with("warning:")
                                    || trimmed.starts_with("wsl:");
                                if is_noise {
                                    return Some((Ok(ChatStreamChunk {
                                        content: String::new(),
                                        is_final: false,
                                        finish_reason: None,
                                        usage: None,
                                    }), (lines, child)));
                                }

                                // Extract content based on output mode
                                let content = if cfg.output_mode == SubprocessOutputMode::Json {
                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                                        if let Some(ref filter) = cfg.output_filter {
                                            let mut cur = &val;
                                            for part in filter.split('.') {
                                                cur = if let Ok(i) = part.parse::<usize>() {
                                                    cur.get(i).unwrap_or(&serde_json::Value::Null)
                                                } else {
                                                    cur.get(part).unwrap_or(&serde_json::Value::Null)
                                                };
                                            }
                                            cur.as_str().map(|s| s.to_string())
                                                .unwrap_or_else(|| trimmed.to_string())
                                        } else {
                                            val.get("content").and_then(|v| v.as_str()).map(|s| s.to_string())
                                                .or_else(|| val.get("text").and_then(|v| v.as_str()).map(|s| s.to_string()))
                                                .unwrap_or_else(|| trimmed.to_string())
                                        }
                                    } else {
                                        trimmed.to_string()
                                    }
                                } else {
                                    trimmed.to_string()
                                };

                                Some((Ok(ChatStreamChunk {
                                    content: format!("{}\n", content),
                                    is_final: false,
                                    finish_reason: None,
                                    usage: None,
                                }), (lines, child)))
                            }
                            Ok(None) => {
                                // EOF — process finished
                                let _ = child.wait().await;
                                None
                            }
                            Err(_) => {
                                let _ = child.kill().await;
                                None
                            }
                        }
                    }
                },
            );

            return Ok(stream.boxed());
        }

        // Persistent-process modes (Json/Plain): single-chunk for now
        let resp = self.chat(request).await?;
        let chunk = ChatStreamChunk {
            content: resp.content,
            is_final: true,
            finish_reason: Some(resp.finish_reason),
            usage: Some(resp.usage),
        };
        Ok(stream::once(async move { Ok(chunk) }).boxed())
    }

    async fn health_check(&self) -> Result<bool> {
        let child = self.child.lock().await;
        Ok(child.is_some())
    }

    fn driver_type(&self) -> &str {
        match self.config.shell {
            ShellType::Native => "subprocess/native",
            ShellType::Wsl => "subprocess/wsl",
            ShellType::Ssh => "subprocess/ssh",
        }
    }

    async fn stop(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        if let Some(ref mut c) = *child {
            let _ = c.kill().await;
            info!("Stopped subprocess agent '{}'", self.name);
        }
        *child = None;
        Ok(())
    }
}

// ── Subprocess protocol types ────────────────────────────────

#[derive(Serialize)]
struct SubprocessRequest {
    model: String,
    messages: Vec<SubprocessMessage>,
}

#[derive(Serialize, Deserialize)]
struct SubprocessMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct SubprocessResponse {
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_native_command() {
        let config = SubprocessSection {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            working_dir: None,
            shell: ShellType::Native,
            wsl_distro: None,
            ssh_host: None,
            ssh_user: None,
            ssh_key: None,
            env: Default::default(),
            input_mode: SubprocessInputMode::Argv,
            output_mode: SubprocessOutputMode::Raw,
            output_filter: None,
        };
        let driver = SubprocessDriver::new("test", config);
        let _cmd = driver.build_command();
        // Verify it builds without panic
        assert_eq!(driver.driver_type(), "subprocess/native");
    }

    #[test]
    fn test_build_wsl_command() {
        let config = SubprocessSection {
            command: "claude".to_string(),
            args: vec!["--json".to_string()],
            working_dir: None,
            shell: ShellType::Wsl,
            wsl_distro: Some("Ubuntu".to_string()),
            ssh_host: None,
            ssh_user: None,
            ssh_key: None,
            env: Default::default(),
            input_mode: SubprocessInputMode::Argv,
            output_mode: SubprocessOutputMode::Raw,
            output_filter: None,
        };
        let driver = SubprocessDriver::new("claude-wsl", config);
        assert_eq!(driver.driver_type(), "subprocess/wsl");
    }

    #[test]
    fn test_build_ssh_command() {
        let config = SubprocessSection {
            command: "hermes".to_string(),
            args: vec![],
            working_dir: None,
            shell: ShellType::Ssh,
            wsl_distro: None,
            ssh_host: Some("192.168.50.1".to_string()),
            ssh_user: Some("dgx-user".to_string()),
            ssh_key: None,
            env: Default::default(),
            input_mode: SubprocessInputMode::Argv,
            output_mode: SubprocessOutputMode::Raw,
            output_filter: None,
        };
        let driver = SubprocessDriver::new("hermes-spark", config);
        assert_eq!(driver.driver_type(), "subprocess/ssh");
    }

    #[test]
    fn test_process_output_openclaw() {
        let config = SubprocessSection {
            command: "openclaw".to_string(),
            args: vec![],
            working_dir: None,
            shell: ShellType::Native,
            wsl_distro: None,
            ssh_host: None,
            ssh_user: None,
            ssh_key: None,
            env: Default::default(),
            input_mode: SubprocessInputMode::Argv,
            output_mode: SubprocessOutputMode::Json,
            output_filter: Some("finalAssistantVisibleText".to_string()),
        };
        let driver = SubprocessDriver::new("openclaw", config);
        let raw_json = r#"{
  "payloads": [
    {
      "text": "Hey Z!",
      "mediaUrl": null
    }
  ],
  "meta": {
    "durationMs": 6555
  },
  "finalAssistantVisibleText": "Hey Z!",
  "replayInvalid": false,
  "livenessState": "working",
  "stopReason": "stop"
}"#;
        // Also simulate what happens if there is stderr before it
        let combined = format!("some stderr\n{}", raw_json);
        let processed = driver.process_output(&combined);
        assert_eq!(processed, "Hey Z!");
    }
}
