//! Agent Manifest
//!
//! Defines the TOML manifest format for agent plugins.
//! Each agent lives in `plugins/agents/<name>/agent.toml`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use crate::{AgentError, Result};

/// Top-level agent manifest (parsed from agent.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Agent identity
    pub agent: AgentSection,
    /// Capabilities
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    /// Model configuration (for API driver)
    #[serde(default)]
    pub model: Option<ModelSection>,
    /// Authentication
    #[serde(default)]
    pub auth: Option<AuthSection>,
    /// Subprocess configuration (for subprocess driver)
    #[serde(default)]
    pub subprocess: Option<SubprocessSection>,
    /// Tmux backend configuration
    #[serde(default)]
    pub tmux: Option<TmuxSection>,
    /// Auto-join channels
    #[serde(default)]
    pub channels: Option<ChannelsSection>,
}

/// Agent identity section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSection {
    /// Unique name for this agent
    pub name: String,
    /// Semantic version
    #[serde(default = "default_version")]
    pub version: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// Driver type: "api", "subprocess", "mcp"
    pub driver: DriverType,
}

/// Driver type for connecting to the agent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriverType {
    /// HTTP API driver (uses ModelRouter)
    Api,
    /// Subprocess driver (spawns CLI tool)
    Subprocess,
    /// MCP protocol driver (future)
    Mcp,
    /// Tmux backend driver
    Tmux,
}

/// Tmux driver configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmuxSection {
    /// Tmux session name (e.g. "ch_openclaw")
    pub session_name: String,
    /// The command to execute in the tmux session initially
    pub command: String,
    /// Arguments for the command
    #[serde(default)]
    pub args: Vec<String>,
    /// Shell type to wrap tmux in
    #[serde(default = "default_shell")]
    pub shell: ShellType,
    /// WSL distro
    pub wsl_distro: Option<String>,
    /// How to read logs (if empty, defaults to capture-pane scraping)
    pub log_command: Option<String>,
    /// Optional field to extract from JSON log output (e.g. "content" or "text")
    pub output_filter: Option<String>,
}

/// Input communication strategy for subprocesses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SubprocessInputMode {
    /// Send full Crow Hub JSON object to stdin (Default)
    #[default]
    Json,
    /// Send only the raw text of the user's message to stdin
    Plain,
    /// Pass message text as a command-line argument to a fresh process
    Argv,
}

/// Output interpretation strategy for subprocesses
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SubprocessOutputMode {
    /// Treat all output as raw text (Default)
    #[default]
    Raw,
    /// Expect JSON output and allow filtering/extraction
    Json,
}

/// What the agent can do
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilitiesSection {
    #[serde(default)]
    pub chat: bool,
    #[serde(default)]
    pub streaming: bool,
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default)]
    pub code_execution: bool,
}

/// Model routing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSection {
    /// Backend name (must match a registered backend in ModelRouter)
    pub backend: Option<String>,
    /// Default model to use
    pub default: String,
    /// Allowed models (empty = any model on the backend)
    #[serde(default)]
    pub allowed: Vec<String>,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSection {
    /// Environment variable containing the API key
    pub api_key_env: Option<String>,
    /// Direct API key (not recommended, use env var)
    pub api_key: Option<String>,
}

impl AuthSection {
    /// Resolve the API key from env var or direct value
    pub fn resolve_api_key(&self) -> Option<String> {
        if let Some(ref env_var) = self.api_key_env {
            std::env::var(env_var).ok()
        } else {
            self.api_key.clone()
        }
    }
}

/// Subprocess driver configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubprocessSection {
    /// The command to run
    pub command: String,
    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory
    pub working_dir: Option<String>,
    /// Shell type: "native", "wsl", "ssh"
    #[serde(default = "default_shell")]
    pub shell: ShellType,
    /// WSL distro name (for shell = "wsl")
    pub wsl_distro: Option<String>,
    /// SSH host (for shell = "ssh")
    pub ssh_host: Option<String>,
    /// SSH user (for shell = "ssh")
    pub ssh_user: Option<String>,
    /// SSH key path (for shell = "ssh")
    pub ssh_key: Option<String>,
    /// Environment variables to set
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Strategy for sending data to the process
    #[serde(default)]
    pub input_mode: SubprocessInputMode,
    /// Strategy for interpreting process output
    #[serde(default)]
    pub output_mode: SubprocessOutputMode,
    /// Optional field to extract from JSON output (e.g. "candidates.0.text")
    pub output_filter: Option<String>,
}

/// Shell type for subprocess execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellType {
    /// Direct process spawn on the host OS
    Native,
    /// Run in WSL2 via wsl.exe
    Wsl,
    /// Run on remote host via SSH
    Ssh,
}

/// Channels configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelsSection {
    /// Channels to auto-join on startup
    #[serde(default)]
    pub auto_join: Vec<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

fn default_shell() -> ShellType {
    ShellType::Native
}

impl AgentManifest {
    /// Load a manifest from a TOML file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| AgentError::Manifest(format!("Cannot read {:?}: {}", path, e)))?;
        Self::from_str(&content)
    }

    /// Parse a manifest from a TOML string
    pub fn from_str(content: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the manifest
    fn validate(&self) -> Result<()> {
        if self.agent.name.is_empty() {
            return Err(AgentError::Manifest("Agent name is required".to_string()));
        }

        match self.agent.driver {
            DriverType::Api => {
                if self.model.is_none() {
                    return Err(AgentError::Manifest(
                        "API driver requires [model] section".to_string(),
                    ));
                }
            }
            DriverType::Subprocess => {
                if self.subprocess.is_none() {
                    return Err(AgentError::Manifest(
                        "Subprocess driver requires [subprocess] section".to_string(),
                    ));
                }
            }
            DriverType::Mcp => {
                // MCP validation will come later
            }
            DriverType::Tmux => {
                if self.tmux.is_none() {
                    return Err(AgentError::Manifest(
                        "Tmux driver requires [tmux] section".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_api_manifest() {
        let toml = r#"
[agent]
name = "claude-api"
driver = "api"
description = "Claude via Anthropic API"

[capabilities]
chat = true
streaming = true
tools = true

[model]
backend = "anthropic"
default = "claude-sonnet-4-6"
allowed = ["claude-sonnet-4-6", "claude-opus-4-6"]

[auth]
api_key_env = "ANTHROPIC_API_KEY"

[channels]
auto_join = ["general"]
"#;

        let manifest = AgentManifest::from_str(toml).unwrap();
        assert_eq!(manifest.agent.name, "claude-api");
        assert_eq!(manifest.agent.driver, DriverType::Api);
        assert!(manifest.capabilities.chat);
        assert!(manifest.capabilities.streaming);
        assert_eq!(manifest.model.as_ref().unwrap().default, "claude-sonnet-4-6");
        assert_eq!(manifest.channels.as_ref().unwrap().auto_join, vec!["general"]);
    }

    #[test]
    fn test_parse_subprocess_manifest_wsl() {
        let toml = r#"
[agent]
name = "claude-code"
driver = "subprocess"
description = "Claude Code CLI in WSL"

[capabilities]
chat = true
code_execution = true

[subprocess]
command = "claude"
args = ["--output-format", "json"]
shell = "wsl"
wsl_distro = "Ubuntu"
"#;

        let manifest = AgentManifest::from_str(toml).unwrap();
        assert_eq!(manifest.agent.driver, DriverType::Subprocess);
        let sub = manifest.subprocess.unwrap();
        assert_eq!(sub.command, "claude");
        assert_eq!(sub.shell, ShellType::Wsl);
        assert_eq!(sub.wsl_distro.unwrap(), "Ubuntu");
    }

    #[test]
    fn test_parse_subprocess_manifest_ssh() {
        let toml = r#"
[agent]
name = "hermes-spark"
driver = "subprocess"
description = "Hermes on DGX Spark"

[subprocess]
command = "hermes"
shell = "ssh"
ssh_host = "192.168.50.1"
ssh_user = "dgx-user"
"#;

        let manifest = AgentManifest::from_str(toml).unwrap();
        let sub = manifest.subprocess.unwrap();
        assert_eq!(sub.shell, ShellType::Ssh);
        assert_eq!(sub.ssh_host.unwrap(), "192.168.50.1");
    }

    #[test]
    fn test_api_manifest_requires_model() {
        let toml = r#"
[agent]
name = "bad-agent"
driver = "api"
"#;
        assert!(AgentManifest::from_str(toml).is_err());
    }

    #[test]
    fn test_subprocess_manifest_requires_subprocess() {
        let toml = r#"
[agent]
name = "bad-agent"
driver = "subprocess"
"#;
        assert!(AgentManifest::from_str(toml).is_err());
    }
}
