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
use crate::host_env::HostEnv;
use crate::manifest::{ShellType, SubprocessInputMode, SubprocessOutputMode, SubprocessSection};
use crate::{AgentError, Result};
use ch_model::{ChatRequest, ChatResponse, ChatStreamChunk, FinishReason, TokenUsage};
use futures::stream::{self, BoxStream, StreamExt};
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tracing::{info, warn};

// ── Shell quoting + remote invocation helpers ─────────────────

/// POSIX-safe shell quoting: wraps `s` in single quotes and escapes any
/// embedded single quotes as `'\''` (close, escape, single, open).
/// Returns the input unchanged if it consists entirely of safe characters
/// (`A-Za-z0-9_/-.:,=@`).
fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    let safe = s.chars().all(|c| {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '-' | '.' | ':' | ',' | '=' | '@')
    });
    if safe {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Build a single shell command string for remote execution.
///
/// Strategy: prefix the agent binary with `env KEY=VAL ...` to override
/// `$PATH` (and a handful of other vars) with the host's cached interactive
/// env — captured once via `bash -lc env`.  This is generic across version
/// managers (nvm/fnm/volta/asdf/mise/etc.) because we don't *guess* paths,
/// we *use the user's actual shell PATH*.
///
/// If the manifest provides a `setup_script` (per-agent escape hatch — e.g.
/// `source venv/bin/activate`), wrap the invocation in `bash -c '<setup>;
/// exec <cmd> <args>'` so the script runs in a real shell before exec.
///
/// The result is intended to be passed:
///   * for SSH — as **one** argv slot to `ssh user@host` (since ssh joins
///     remote args with spaces, multi-slot would be split-on-space by the
///     remote shell);
///   * for WSL — as the trailing arg to `wsl -e <this>` (WSL preserves argv).
fn compose_remote_invocation(
    command: &str,
    args: &[String],
    prompt: Option<&str>,
    host_env: &HostEnv,
    setup_script: Option<&str>,
    working_dir: Option<&str>,
) -> String {
    // 1. Build the `env KEY=VAL ...` prefix from the cached host env.
    let mut env_parts: Vec<String> = host_env
        .iter_sorted()
        .into_iter()
        .map(|(k, v)| format!("{}={}", k, shell_quote(v)))
        .collect();

    // 2. Build the command + args + (optional) prompt, each shell-quoted.
    let mut cmd_parts: Vec<String> = std::iter::once(command.to_string())
        .chain(args.iter().cloned())
        .chain(prompt.map(|p| p.to_string()))
        .map(|s| shell_quote(&s))
        .collect();

    let mut prelude = Vec::new();
    if let Some(dir) = working_dir {
        prelude.push(format!("cd {}", shell_quote(dir)));
    }
    if let Some(script) = setup_script {
        prelude.push(script.trim().to_string());
    }

    if prelude.is_empty() {
        // No setup script or working dir — just `env KEY=VAL ... cmd args`.
        let mut all = Vec::with_capacity(1 + env_parts.len() + cmd_parts.len());
        if !env_parts.is_empty() {
            all.push("env".to_string());
            all.append(&mut env_parts);
        }
        all.append(&mut cmd_parts);
        all.join(" ")
    } else {
        // Wrap in bash -c so we can cd and run setup script.
        let inner = format!("{}; exec {}", prelude.join(" && "), cmd_parts.join(" "));
        let mut all = Vec::with_capacity(env_parts.len() + 4);
        if !env_parts.is_empty() {
            all.push("env".to_string());
            all.append(&mut env_parts);
        }
        all.push("bash".to_string());
        all.push("-c".to_string());
        all.push(shell_quote(&inner));
        all.join(" ")
    }
}

/// Driver that spawns CLI agents as subprocesses
pub struct SubprocessDriver {
    /// Agent name
    name: String,
    /// Subprocess configuration
    config: SubprocessSection,
    /// Captured host env (PATH etc. from the user's interactive shell).
    /// Defaults to empty, in which case the remote shell's default PATH
    /// is used — which is the old behavior.
    host_env: HostEnv,
    /// Running child process (if started)
    child: Mutex<Option<Child>>,
}

impl SubprocessDriver {
    /// Create a driver with no host-env knowledge (uses the remote shell's
    /// default PATH).  Mostly useful for tests; production code goes
    /// through [`with_env`] with a probed [`HostEnv`].
    pub fn new(name: impl Into<String>, config: SubprocessSection) -> Self {
        Self::with_env(name, config, HostEnv::default())
    }

    /// Create a driver with a pre-probed host env.  Used by the runtime
    /// at agent-load time.
    pub fn with_env(name: impl Into<String>, config: SubprocessSection, host_env: HostEnv) -> Self {
        Self {
            name: name.into(),
            config,
            host_env,
            child: Mutex::new(None),
        }
    }

    /// Build the command based on shell type.
    ///
    /// `prompt` is an optional final positional argument to append to the
    /// agent's invocation (used by Argv-mode `chat()` / `stream_chat()` —
    /// pass `None` for persistent-process modes that send prompts over
    /// stdin instead).
    ///
    /// For WSL and SSH, this wraps the invocation in `bash -c '<prelude>;
    /// exec <quoted-cmd-and-args>'` so non-interactive remote shells get
    /// nvm/fnm/Homebrew on `$PATH` (mirroring what the scanner does for
    /// discovery).  The prompt is shell-quoted into the inner string —
    /// callers must NOT append it after this returns.
    fn build_command(&self, prompt: Option<&str>) -> Command {
        match self.config.shell {
            ShellType::Native => {
                let mut cmd = Command::new(&self.config.command);
                cmd.args(&self.config.args);
                if let Some(p) = prompt {
                    cmd.arg(p);
                }
                if let Some(ref dir) = self.config.working_dir {
                    cmd.current_dir(dir);
                }
                for (k, v) in &self.config.env {
                    cmd.env(k, v);
                }
                cmd
            }
            ShellType::Wsl => {
                let inner = compose_remote_invocation(
                    &self.config.command,
                    &self.config.args,
                    prompt,
                    &self.host_env,
                    self.config.setup_script.as_deref(),
                    self.config.working_dir.as_deref(),
                );
                let mut cmd = Command::new("wsl.exe");
                // Avoid polluting WSL path with Windows paths by clearing WSLENV
                cmd.env_remove("WSLENV");

                if let Some(ref distro) = self.config.wsl_distro {
                    cmd.args(["-d", distro]);
                }
                // `wsl -e <argv...>` runs the trailing argv directly.  We
                // wrap in `sh -c '<inner>'` because `<inner>` is itself a
                // shell command (env KEY=VAL ... cmd args, possibly with
                // bash -c '...' inside if there's a setup_script).  Using
                // the lighter `sh -c` instead of `bash -c` here just to
                // run the env-prefix command — no shell features needed.
                cmd.args(["-e", "sh", "-c"]);
                cmd.arg(&inner);
                cmd
            }
            ShellType::Ssh => {
                let inner = compose_remote_invocation(
                    &self.config.command,
                    &self.config.args,
                    prompt,
                    &self.host_env,
                    self.config.setup_script.as_deref(),
                    self.config.working_dir.as_deref(),
                );
                let mut cmd = Command::new("ssh");

                // Match the scanner's SSH options so connections never prompt
                // and fail fast on first-connect host-key prompts.
                cmd.args([
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "ConnectTimeout=10",
                    "-o",
                    "StrictHostKeyChecking=accept-new",
                ]);
                if let Some(ref key) = self.config.ssh_key {
                    cmd.args(["-i", key]);
                }

                let host = self.config.ssh_host.as_deref().unwrap_or("localhost");
                let target = if let Some(ref user) = self.config.ssh_user {
                    format!("{}@{}", user, host)
                } else {
                    host.to_string()
                };
                cmd.arg(&target);

                // ssh joins all remote args with spaces and runs the result
                // via the remote `/bin/sh -c …`.  `<inner>` is already a
                // valid shell command (env KEY=VAL ... cmd args), so we can
                // pass it as one argv slot — no extra wrapping needed.
                cmd.arg(&inner);
                cmd
            }
        }
    }

    /// Start the subprocess
    pub async fn start(&self) -> Result<()> {
        // Persistent-process modes (Json/Plain) — prompt is sent over
        // stdin later, not as an argv, so pass `None` here.
        let mut cmd = self.build_command(None);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        info!(
            "Starting subprocess agent '{}': {:?} (shell: {:?})",
            self.name, self.config.command, self.config.shell
        );

        let child = cmd.spawn().map_err(|e| {
            AgentError::Driver(format!(
                "Failed to spawn '{}': {}. Shell: {:?}",
                self.config.command, e, self.config.shell
            ))
        })?;

        *self.child.lock().await = Some(child);
        info!("Subprocess agent '{}' started", self.name);
        Ok(())
    }

    /// Helper to resolve a JSON value from a path string like "a.b.c"
    fn get_value_by_path<'a>(
        &self,
        value: &'a serde_json::Value,
        path: &str,
    ) -> Option<&'a serde_json::Value> {
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
            let json_str = if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}'))
            {
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

    /// Try to extract token usage from the raw JSON output of a CLI agent.
    /// Probes common field paths used by different agents:
    ///   - OpenClaw: `meta.usage.input` / `meta.usage.output`
    ///   - Anthropic: `usage.input_tokens` / `usage.output_tokens`
    ///   - OpenAI: `usage.prompt_tokens` / `usage.completion_tokens`
    ///   - Gemini: `usageMetadata.promptTokenCount` / `usageMetadata.candidatesTokenCount`
    fn extract_usage_from_json(&self, raw_json: &str) -> Option<TokenUsage> {
        let val: serde_json::Value = serde_json::from_str(raw_json).ok()?;

        // OpenClaw shape: { meta: { usage: { input: N, output: M } } }
        if let Some(meta) = val.get("meta") {
            if let Some(usage) = meta.get("usage") {
                let input = usage.get("input").and_then(|v| v.as_u64());
                let output = usage.get("output").and_then(|v| v.as_u64());
                if input.is_some() || output.is_some() {
                    return Some(TokenUsage {
                        input_tokens: input.unwrap_or(0),
                        output_tokens: output.unwrap_or(0),
                        total_tokens: input.unwrap_or(0) + output.unwrap_or(0),
                    });
                }
            }
        }

        // Gemini shape: { usageMetadata: { promptTokenCount, candidatesTokenCount } }
        if let Some(meta) = val.get("usageMetadata") {
            let input = meta.get("promptTokenCount").and_then(|v| v.as_u64());
            let output = meta.get("candidatesTokenCount").and_then(|v| v.as_u64());
            if input.is_some() || output.is_some() {
                return Some(TokenUsage {
                    input_tokens: input.unwrap_or(0),
                    output_tokens: output.unwrap_or(0),
                    total_tokens: input.unwrap_or(0) + output.unwrap_or(0),
                });
            }
        }

        // Anthropic / OpenAI shape: { usage: { input_tokens, output_tokens } }
        // or { usage: { prompt_tokens, completion_tokens } }
        if let Some(usage) = val.get("usage") {
            let input = usage
                .get("input_tokens")
                .or_else(|| usage.get("prompt_tokens"))
                .and_then(|v| v.as_u64());
            let output = usage
                .get("output_tokens")
                .or_else(|| usage.get("completion_tokens"))
                .and_then(|v| v.as_u64());
            if input.is_some() || output.is_some() {
                return Some(TokenUsage {
                    input_tokens: input.unwrap_or(0),
                    output_tokens: output.unwrap_or(0),
                    total_tokens: input.unwrap_or(0) + output.unwrap_or(0),
                });
            }
        }

        None
    }

    /// Extract token usage from stderr when the CLI agent prints a
    /// human-readable summary.  Common formats:
    ///   - Claude: `⏺ Total tokens: 1234 (input: 100, output: 1134)`
    ///   - Generic:  `input: N, output: N`
    fn extract_usage_from_stderr(&self, stderr: &str) -> Option<TokenUsage> {
        // Claude Code format
        let re =
            regex::Regex::new(r"Total tokens:\s*(\d+)\s*\(input:\s*(\d+),\s*output:\s*(\d+)\)")
                .ok()?;
        if let Some(caps) = re.captures(stderr) {
            let input = caps.get(2)?.as_str().parse::<u64>().ok()?;
            let output = caps.get(3)?.as_str().parse::<u64>().ok()?;
            return Some(TokenUsage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: input + output,
            });
        }
        // Generic: "input: N ... output: N"
        let re2 = regex::Regex::new(r"(?:^|\s)input[^\d]*(\d+).*?output[^\d]*(\d+)").ok()?;
        if let Some(caps) = re2.captures(stderr) {
            let input = caps.get(1)?.as_str().parse::<u64>().ok()?;
            let output = caps.get(2)?.as_str().parse::<u64>().ok()?;
            if (input > 0 || output > 0) && input < 10_000_000 && output < 10_000_000 {
                return Some(TokenUsage {
                    input_tokens: input,
                    output_tokens: output,
                    total_tokens: input + output,
                });
            }
        }
        None
    }

    /// Estimate token count from text when the agent doesn't report usage.
    /// Uses a simple heuristic: ~4 chars/token for ASCII (English/code),
    /// ~1.5 chars/token for CJK.  Returns 0 for empty input.
    fn estimate_tokens(&self, text: &str) -> u64 {
        if text.is_empty() {
            return 0;
        }
        let ascii = text.chars().filter(|c| c.is_ascii()).count() as f64;
        let non_ascii = text.chars().count() as f64 - ascii;
        (ascii / 4.0 + non_ascii / 1.5).ceil() as u64
    }
}

#[async_trait::async_trait]
impl AgentDriver for SubprocessDriver {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse> {
        let prompt = request
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // If we are in Argv mode, we spawn a fresh process for this message.
        // The prompt is baked into the inner shell command by build_command
        // (so it lands inside the SSH/WSL `bash -c '…'` wrapper) — do NOT
        // append `cmd.arg(&prompt)` after this returns.
        if self.config.input_mode == SubprocessInputMode::Argv {
            let mut cmd = self.build_command(Some(&prompt));
            cmd.stdin(Stdio::null());

            let output = cmd.output().await.map_err(|e| {
                AgentError::Driver(format!(
                    "Failed to execute '{}' (shell={:?}): {}",
                    self.config.command, self.config.shell, e
                ))
            })?;

            let raw_stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let raw_stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            // Log stderr for debugging (always, even on success) but filter WSL noise
            let filtered_stderr = raw_stderr
                .lines()
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
                    warn!(
                        "[{}] stderr (truncated): {}...",
                        self.name,
                        &filtered_stderr[..300]
                    );
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
                let exit_info = output
                    .status
                    .code()
                    .map(|c| format!("exit {}", c))
                    .unwrap_or_else(|| "killed by signal".to_string());
                let invocation = format!("{} {}", self.config.command, self.config.args.join(" "));
                let stdout_snip = if raw_stdout.is_empty() {
                    "(empty)".to_string()
                } else {
                    raw_stdout.clone()
                };
                let stderr_snip = if filtered_stderr.is_empty() {
                    "(empty)".to_string()
                } else {
                    filtered_stderr.clone()
                };
                return Err(AgentError::Driver(format!(
                    "Agent '{}' failed ({}) — invocation: `{}` (shell={:?})\n  stdout: {}\n  stderr: {}",
                    self.name, exit_info, invocation, self.config.shell,
                    stdout_snip, stderr_snip
                )));
            }

            let content = self.process_output(&combined_output);
            // Try real token counts first (JSON metadata → stderr summary → char estimate)
            let usage = if self.config.output_mode == SubprocessOutputMode::Json {
                self.extract_usage_from_json(&combined_output)
                    .or_else(|| self.extract_usage_from_stderr(&filtered_stderr))
                    .unwrap_or_else(|| TokenUsage {
                        input_tokens: self.estimate_tokens(&prompt),
                        output_tokens: self.estimate_tokens(&content),
                        total_tokens: self.estimate_tokens(&prompt)
                            + self.estimate_tokens(&content),
                    })
            } else {
                self.extract_usage_from_stderr(&filtered_stderr)
                    .unwrap_or_else(|| TokenUsage {
                        input_tokens: self.estimate_tokens(&prompt),
                        output_tokens: self.estimate_tokens(&content),
                        total_tokens: self.estimate_tokens(&prompt)
                            + self.estimate_tokens(&content),
                    })
            };

            // Even on clean exit, if content is empty and stderr had something,
            // fold that into the response so the user sees it rather than a
            // silent "(no response)".
            if content.trim().is_empty() && !filtered_stderr.is_empty() {
                return Ok(ChatResponse {
                    content: format!("(agent produced no stdout; stderr: {})", filtered_stderr),
                    model: request.model,
                    backend: format!("subprocess/{}", self.name),
                    usage,
                    finish_reason: FinishReason::Stop,
                });
            }

            return Ok(ChatResponse {
                content,
                model: request.model,
                backend: format!("subprocess/{}", self.name),
                usage,
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

        let child = child_lock
            .as_mut()
            .ok_or_else(|| AgentError::Driver("Subprocess not running".to_string()))?;

        // Write based on input mode
        if let Some(ref mut stdin) = child.stdin {
            let payload = match self.config.input_mode {
                SubprocessInputMode::Json => {
                    let req = SubprocessRequest {
                        model: request.model.clone(),
                        messages: request
                            .messages
                            .iter()
                            .map(|m| SubprocessMessage {
                                role: format!("{:?}", m.role).to_lowercase(),
                                content: m.content.clone(),
                            })
                            .collect(),
                    };
                    serde_json::to_string(&req).map_err(|e| AgentError::Driver(e.to_string()))?
                }
                SubprocessInputMode::Plain => format!("{}\n", prompt),
                SubprocessInputMode::Argv => unreachable!(),
            };

            stdin
                .write_all(payload.as_bytes())
                .await
                .map_err(|e| AgentError::Driver(format!("stdin write failed: {}", e)))?;
            stdin
                .flush()
                .await
                .map_err(|e| AgentError::Driver(format!("stdin flush failed: {}", e)))?;
        }

        // Close stdin so the child knows input is complete and flushes its
        // response buffer.  This is critical for agents like Kimi that wait
        // for EOF before emitting output.
        drop(child.stdin.take());

        // Read response — chunk-based accumulation instead of a single
        // read_line, because many agents emit multi-line output or flush
        // in arbitrary chunks.
        let stdout = child
            .stdout
            .take()
            .ok_or(AgentError::Driver("stdout unavailable".into()))?;
        let mut reader = BufReader::new(stdout);
        let mut buf = Vec::with_capacity(4096);

        let read_result = tokio::time::timeout(std::time::Duration::from_secs(120), async {
            // Read until EOF (child closes its stdout)
            tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut buf).await
        })
        .await;

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
            Err(_) => Err(AgentError::Driver(
                "Subprocess response timed out after 120s".into(),
            )),
        }
    }

    async fn stream_chat(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<ChatStreamChunk>>> {
        let prompt = request
            .messages
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Argv mode: spawn a fresh process and stream stdout line-by-line.
        // The prompt is baked into build_command's inner shell wrapper for
        // SSH/WSL — do NOT append `cmd.arg(&prompt)` after this returns.
        // Structured JSON outputs are often multi-line, which breaks
        // line-by-line parsing — fall through to `chat()` for Json output
        // mode to accumulate and parse correctly.
        if self.config.input_mode == SubprocessInputMode::Argv
            && self.config.output_mode != SubprocessOutputMode::Json
        {
            let mut cmd = self.build_command(Some(&prompt));
            cmd.stdin(Stdio::null());
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());

            let mut child = cmd.spawn().map_err(|e| {
                AgentError::Driver(format!("Failed to spawn '{}': {}", self.config.command, e))
            })?;

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

            let stdout = child
                .stdout
                .take()
                .ok_or(AgentError::Driver("stdout unavailable".into()))?;
            let lines = BufReader::new(stdout).lines();
            let config = self.config.clone();
            let prompt_len = prompt.len() as u64;

            // Track total output chars to estimate tokens at stream end
            let total_output = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
            let total_for_final = total_output.clone();

            let stream = stream::unfold((lines, child), move |(mut lines, mut child)| {
                let total = total_output.clone();
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
                                return Some((
                                    Ok(ChatStreamChunk {
                                        content: String::new(),
                                        is_final: false,
                                        finish_reason: None,
                                        usage: None,
                                    }),
                                    (lines, child),
                                ));
                            }

                            // Extract content based on output mode
                            let content = if cfg.output_mode == SubprocessOutputMode::Json {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed)
                                {
                                    if let Some(ref filter) = cfg.output_filter {
                                        let mut cur = &val;
                                        for part in filter.split('.') {
                                            cur = if let Ok(i) = part.parse::<usize>() {
                                                cur.get(i).unwrap_or(&serde_json::Value::Null)
                                            } else {
                                                cur.get(part).unwrap_or(&serde_json::Value::Null)
                                            };
                                        }
                                        cur.as_str()
                                            .map(|s| s.to_string())
                                            .unwrap_or_else(|| trimmed.to_string())
                                    } else {
                                        val.get("content")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string())
                                            .or_else(|| {
                                                val.get("text")
                                                    .and_then(|v| v.as_str())
                                                    .map(|s| s.to_string())
                                            })
                                            .unwrap_or_else(|| trimmed.to_string())
                                    }
                                } else {
                                    trimmed.to_string()
                                }
                            } else {
                                trimmed.to_string()
                            };

                            // Track output chars for token estimation at end of stream
                            total.fetch_add(
                                content.len() as u64,
                                std::sync::atomic::Ordering::Relaxed,
                            );

                            Some((
                                Ok(ChatStreamChunk {
                                    content: format!("{}\n", content),
                                    is_final: false,
                                    finish_reason: None,
                                    usage: None,
                                }),
                                (lines, child),
                            ))
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
            });

            // Chain a final chunk with estimated token counts so the runtime
            // handler picks them up for the cumulative TUI display.
            let out_chars = total_for_final.load(std::sync::atomic::Ordering::Relaxed);
            let est_in = (prompt_len as f64 / 4.0).ceil() as u64;
            let est_out = (out_chars as f64 / 4.0).ceil() as u64;
            let final_chunk = ChatStreamChunk {
                content: String::new(),
                is_final: true,
                finish_reason: Some(FinishReason::Stop),
                usage: Some(TokenUsage {
                    input_tokens: est_in,
                    output_tokens: est_out,
                    total_tokens: est_in + est_out,
                }),
            };
            let stream = stream.chain(stream::once(async move { Ok(final_chunk) }));

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

    // ── shell_quote ────────────────────────────────────────────

    #[test]
    fn shell_quote_safe_passthrough() {
        assert_eq!(shell_quote("hello"), "hello");
        assert_eq!(shell_quote("/usr/local/bin/foo"), "/usr/local/bin/foo");
        assert_eq!(shell_quote("--flag=value"), "--flag=value");
        assert_eq!(shell_quote("v24.14.0"), "v24.14.0");
        assert_eq!(shell_quote("user@host"), "user@host");
    }

    #[test]
    fn shell_quote_wraps_spaces() {
        assert_eq!(shell_quote("hello world"), "'hello world'");
        assert_eq!(shell_quote("say  hi"), "'say  hi'");
    }

    #[test]
    fn shell_quote_escapes_single_quote() {
        // 'it'\''s' — close, escape, single, reopen.
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote("'leading"), r"''\''leading'");
    }

    #[test]
    fn shell_quote_double_quotes_left_alone() {
        // Double quotes are safe inside single-quoted strings — no escaping.
        assert_eq!(shell_quote(r#"say "hi""#), r#"'say "hi"'"#);
    }

    #[test]
    fn shell_quote_empty() {
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn shell_quote_special_shell_chars_get_quoted() {
        // Globs, $vars, semicolons, backticks, pipes, redirections — all must
        // be inside single quotes so the remote shell doesn't interpret them.
        for s in [
            "*.rs", "$HOME", "a;b", "`cmd`", "a|b", "a>b", "a&b", "a(b)c", "a[b]c", "a{b}c",
        ] {
            let q = shell_quote(s);
            assert!(
                q.starts_with('\''),
                "expected {:?} to be quoted, got {:?}",
                s,
                q
            );
        }
    }

    // ── compose_remote_invocation ──────────────────────────────

    fn fake_env() -> HostEnv {
        let mut env = HostEnv::new();
        env.vars.insert(
            "PATH".into(),
            "/home/u/.nvm/versions/node/v24/bin:/usr/bin".into(),
        );
        env.vars.insert("NVM_DIR".into(), "/home/u/.nvm".into());
        env
    }

    #[test]
    fn compose_remote_invocation_prefixes_env_and_quotes_args() {
        let s = compose_remote_invocation(
            "/home/u/.nvm/versions/node/v24/bin/openclaw",
            &[
                "agent".into(),
                "--agent".into(),
                "main".into(),
                "--json".into(),
                "-m".into(),
            ],
            Some("hello world"),
            &fake_env(),
            None,
            None,
        );

        // env prefix is present and assigns each captured var
        assert!(s.starts_with("env "), "must start with `env ...`");
        assert!(s.contains("NVM_DIR=/home/u/.nvm"));
        assert!(s.contains("PATH=/home/u/.nvm/versions/node/v24/bin:/usr/bin"));

        // Args preserved (paths and flags pass through as-is)
        assert!(s.contains("/home/u/.nvm/versions/node/v24/bin/openclaw"));
        assert!(s.contains("agent"));
        assert!(s.contains("--agent"));
        assert!(s.contains("--json"));

        // Prompt with space is shell-quoted and is the final token
        assert!(s.ends_with("'hello world'"));
    }

    #[test]
    fn compose_remote_invocation_with_empty_env_skips_prefix() {
        // No probe data → no `env` prefix; bare command.
        let s = compose_remote_invocation(
            "/usr/local/bin/claude",
            &[],
            None,
            &HostEnv::new(),
            None,
            None,
        );
        assert_eq!(s, "/usr/local/bin/claude");
    }

    #[test]
    fn compose_remote_invocation_without_prompt() {
        let s =
            compose_remote_invocation("/usr/local/bin/claude", &[], None, &fake_env(), None, None);
        assert!(s.ends_with("/usr/local/bin/claude"));
        assert!(s.starts_with("env "));
    }

    #[test]
    fn compose_remote_invocation_quotes_prompt_with_single_quote() {
        let s =
            compose_remote_invocation("/bin/echo", &[], Some("it's me"), &fake_env(), None, None);
        // Single quote inside the prompt is properly POSIX-escaped
        assert!(s.contains(r"'it'\''s me'"), "got: {}", s);
    }

    #[test]
    fn compose_remote_invocation_with_setup_script_wraps_in_bash_c() {
        let s = compose_remote_invocation(
            "/usr/local/bin/myagent",
            &["chat".into()],
            Some("hi"),
            &fake_env(),
            Some("source ~/venv/bin/activate"),
            None,
        );
        // Must include the env prefix, then `bash -c '<setup>; exec <cmd>'`
        assert!(s.starts_with("env "));
        assert!(s.contains("bash -c"));
        assert!(s.contains("source ~/venv/bin/activate"));
        assert!(s.contains("exec /usr/local/bin/myagent chat hi"));
    }

    #[test]
    fn compose_remote_invocation_setup_script_is_quoted_safely() {
        // A pathological setup_script with single quotes and special chars
        // must not break out of the bash -c wrapper.
        let s = compose_remote_invocation(
            "/bin/true",
            &[],
            None,
            &fake_env(),
            Some("export FOO='it'\\''s'"),
            None,
        );
        // The full bash -c argument is single-quoted, so the inner single
        // quotes get the POSIX `'\''` escape.  Just verify it parses without
        // panic and that the wrapper structure is intact.
        assert!(s.contains("bash -c"));
        assert!(s.contains("/bin/true"));
    }

    #[test]
    fn compose_remote_invocation_with_working_dir_includes_cd() {
        let s = compose_remote_invocation(
            "/bin/ls",
            &[],
            None,
            &fake_env(),
            None,
            Some("/tmp/project"),
        );
        // Must include `cd /tmp/project && exec /bin/ls`
        assert!(s.contains("cd /tmp/project"));
        assert!(s.contains("exec /bin/ls"));
    }

    // ── build_command ──────────────────────────────────────────

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
            setup_script: None,
        };
        let driver = SubprocessDriver::new("test", config);
        let _cmd = driver.build_command(None);
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
            setup_script: None,
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
            setup_script: None,
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
            setup_script: None,
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

    // ── estimate_tokens ────────────────────────────────────────

    fn dummy_driver() -> SubprocessDriver {
        SubprocessDriver::new(
            "test",
            SubprocessSection {
                command: "echo".into(),
                args: vec![],
                working_dir: None,
                shell: ShellType::Native,
                wsl_distro: None,
                ssh_host: None,
                ssh_user: None,
                ssh_key: None,
                env: std::collections::HashMap::new(),
                input_mode: SubprocessInputMode::Argv,
                output_mode: SubprocessOutputMode::Json,
                output_filter: None,
                setup_script: None,
            },
        )
    }

    #[test]
    fn estimate_tokens_empty() {
        let d = dummy_driver();
        assert_eq!(d.estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_ascii() {
        let d = dummy_driver();
        // "hello world" = 11 chars → ~3 tokens
        assert_eq!(d.estimate_tokens("hello world"), 3);
    }

    #[test]
    fn estimate_tokens_chinese() {
        let d = dummy_driver();
        // "你好世界" = 4 Chinese chars → ~3 tokens (4/1.5=2.67→3)
        assert_eq!(d.estimate_tokens("你好世界"), 3);
    }

    #[test]
    fn estimate_tokens_long_english() {
        let d = dummy_driver();
        // "hello" * 8 = 40 chars → 10 tokens
        let text = "hellohellohellohellohellohellohellohello";
        assert_eq!(d.estimate_tokens(text), 10);
    }

    // ── extract_usage_from_json ────────────────────────────────

    #[test]
    fn extract_usage_gemini_shape() {
        let d = dummy_driver();
        let json = r#"{"usageMetadata":{"promptTokenCount":100,"candidatesTokenCount":50,"totalTokenCount":150}}"#;
        let usage = d.extract_usage_from_json(json).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    // ── extract_usage_from_stderr ──────────────────────────────

    #[test]
    fn extract_usage_from_stderr_claude_format() {
        let d = dummy_driver();
        let stderr = "⏺ Total tokens: 1234 (input: 100, output: 1134)";
        let usage = d.extract_usage_from_stderr(stderr).unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 1134);
    }

    #[test]
    fn extract_usage_from_stderr_generic() {
        let d = dummy_driver();
        let stderr = "some noise\n  input tokens:  500  output tokens: 3200\nmore noise";
        let usage = d.extract_usage_from_stderr(stderr).unwrap();
        assert_eq!(usage.input_tokens, 500);
        assert_eq!(usage.output_tokens, 3200);
    }

    #[test]
    fn extract_usage_from_stderr_no_match() {
        let d = dummy_driver();
        assert!(d.extract_usage_from_stderr("no token info here").is_none());
    }
}
