//! Universal Environment Scanner
//!
//! Probes all execution environments (native, WSL2, SSH) for CLI coding
//! agents and API model servers. Designed to discover *any* agent, not
//! just a hardcoded list.

use crate::manifest::{
    AgentManifest, AgentSection, CapabilitiesSection, ChannelsSection,
    DriverType, ModelSection, ShellType, SubprocessSection,
    SubprocessInputMode, SubprocessOutputMode,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info, warn};

// ── Well-known CLI agents ────────────────────────────────────

/// A well-known CLI agent binary and its metadata.
#[derive(Debug, Clone)]
pub struct KnownAgent {
    /// The binary name to search for (e.g. "claude", "gemini")
    pub binary: &'static str,
    /// Human-readable display name
    pub display_name: &'static str,
    /// Short description
    pub description: &'static str,
    /// Does it support chat?
    pub chat: bool,
    /// Does it support code execution?
    pub code_exec: bool,
}

/// Comprehensive list of known coding CLI agents.
/// The scanner checks for all of these, but the system is NOT limited
/// to this list — users can add any custom binary.
pub fn known_agents() -> Vec<KnownAgent> {
    vec![
        KnownAgent { binary: "claude",    display_name: "Claude Code",    description: "Anthropic Claude Code CLI",       chat: true, code_exec: true },
        KnownAgent { binary: "gemini",    display_name: "Gemini CLI",     description: "Google Gemini CLI",               chat: true, code_exec: true },
        KnownAgent { binary: "codex",     display_name: "Codex CLI",      description: "OpenAI Codex CLI",                chat: true, code_exec: true },
        KnownAgent { binary: "aider",     display_name: "Aider",          description: "AI pair programming in terminal", chat: true, code_exec: true },
        KnownAgent { binary: "cody",      display_name: "Cody",           description: "Sourcegraph Cody CLI",            chat: true, code_exec: false },
        KnownAgent { binary: "copilot",   display_name: "Copilot CLI",    description: "GitHub Copilot CLI",              chat: true, code_exec: false },
        KnownAgent { binary: "openclaw",  display_name: "OpenClaw",       description: "OpenClaw agent",                  chat: true, code_exec: true },
        KnownAgent { binary: "openclaw-browser", display_name: "OpenClaw Browser", description: "OpenClaw browser agent", chat: true, code_exec: true },
        KnownAgent { binary: "hermes",    display_name: "Hermes",         description: "Hermes agent",                    chat: true, code_exec: true },
        KnownAgent { binary: "kimi",      display_name: "Kimi CLI",       description: "Moonshot Kimi CLI",               chat: true, code_exec: false },
        KnownAgent { binary: "tabby",     display_name: "Tabby",          description: "TabbyML code assistant",          chat: true, code_exec: false },
        KnownAgent { binary: "cline",     display_name: "Cline",          description: "Cline AI assistant",              chat: true, code_exec: true },
        KnownAgent { binary: "avante",    display_name: "Avante",         description: "Avante AI coding assistant",      chat: true, code_exec: true },
        KnownAgent { binary: "continue",  display_name: "Continue",       description: "Continue.dev CLI",                chat: true, code_exec: false },
        KnownAgent { binary: "amp",       display_name: "Amp",            description: "Amp AI terminal agent",           chat: true, code_exec: true },
        KnownAgent { binary: "goose",     display_name: "Goose",          description: "Block Goose agent",               chat: true, code_exec: true },
        KnownAgent { binary: "plandex",   display_name: "Plandex",        description: "Plandex AI coding engine",        chat: true, code_exec: true },
        KnownAgent { binary: "mentat",    display_name: "Mentat",         description: "Mentat AI coder",                 chat: true, code_exec: true },
        KnownAgent { binary: "sweep",     display_name: "Sweep",          description: "Sweep AI dev assistant",          chat: true, code_exec: true },
        KnownAgent { binary: "devika",    display_name: "Devika",         description: "Devika AI agent",                 chat: true, code_exec: true },
        KnownAgent { binary: "gpt-engineer", display_name: "GPT-Engineer", description: "GPT-Engineer CLI",              chat: true, code_exec: true },

        KnownAgent { binary: "ollama",    display_name: "Ollama CLI",     description: "Ollama Local CLI",               chat: true, code_exec: false },
    ]
}

// ── Discovered agent ─────────────────────────────────────────

/// An agent found during environment scanning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredAgent {
    /// Binary/command short name
    pub binary: String,
    /// Absolute executable path found
    pub executable_path: String,
    /// Human-readable name
    pub display_name: String,
    /// Description
    pub description: String,
    /// Where it was found
    pub environment: ScanEnvironment,
    /// Whether user selected it (for wizard state)
    pub selected: bool,
    /// Chat capability
    pub chat: bool,
    /// Code execution capability
    pub code_exec: bool,
}


/// Where the scan found the agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScanEnvironment {
    /// Native OS with its OS name (Windows, MacOS, Linux)
    Native(String),
    /// WSL2 distro with a name
    Wsl(String),
    /// Remote SSH host
    Ssh { host: String, user: String },
}

impl std::fmt::Display for ScanEnvironment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanEnvironment::Native(os) => write!(f, "Native ({})", os),
            ScanEnvironment::Wsl(distro) => write!(f, "WSL: {}", distro),
            ScanEnvironment::Ssh { host, user } => write!(f, "SSH: {}@{}", user, host),
        }
    }
}

/// Full scan results
#[derive(Debug, Clone, Default)]
pub struct ScanResults {
    pub agents: Vec<DiscoveredAgent>,
}

// ── Scanner ──────────────────────────────────────────────────

/// Scans specific chosen environments for agents and servers
pub struct EnvironmentScanner {
    targets: Vec<ScanEnvironment>,
}

impl EnvironmentScanner {
    pub fn new(targets: Vec<ScanEnvironment>) -> Self {
        Self { targets }
    }

    /// Helper to detect host OS
    pub fn detect_native_os() -> String {
        let os = std::env::consts::OS;
        match os {
            "windows" => "Windows".to_string(),
            "macos" => "macOS".to_string(),
            "linux" => "Linux".to_string(),
            _ => os.to_string(),
        }
    }

    /// Helper to detect installed WSL2 distros (runs only on Windows)
    pub fn detect_wsl_distros() -> Vec<String> {
        if std::env::consts::OS != "windows" {
            return Vec::new();
        }

        let output = Command::new("wsl")
            .args(["-l", "-q"])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                let raw = out.stdout;
                let text = String::from_utf16_lossy(
                    &raw.chunks(2)
                        .filter_map(|c| {
                            if c.len() == 2 {
                                Some(u16::from_le_bytes([c[0], c[1]]))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<u16>>(),
                );
                text.lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            _ => Vec::new()
        }
    }

    /// Run full scan across the explicit targets
    pub fn scan(&self) -> ScanResults {
        let mut results = ScanResults::default();

        for target in &self.targets {
            match target {
                ScanEnvironment::Native(os) => {
                    info!("Scanning Native ({})...", os);
                    self.scan_native(os.clone(), &mut results);
                }
                ScanEnvironment::Wsl(distro) => {
                    info!("Scanning WSL distro: {}", distro);
                    self.scan_wsl(distro, &mut results);
                }
                ScanEnvironment::Ssh { host, user } => {
                    info!("Scanning SSH: {}@{}", user, host);
                    self.scan_ssh(host, user, &mut results);
                }
            }
        }

        info!(
            "Scan complete: {} CLI agents",
            results.agents.len()
        );
        results
    }

    /// Scan native OS PATH for known binaries
    fn scan_native(&self, os_name: String, results: &mut ScanResults) {
        for agent in known_agents() {
            if self.binary_exists_native(agent.binary) {
                info!("  ✓ Found {} (native)", agent.display_name);
                results.agents.push(DiscoveredAgent {
                    binary: agent.binary.to_string(),
                    executable_path: agent.binary.to_string(), // Use raw binary for native search
                    display_name: agent.display_name.to_string(),
                    description: agent.description.to_string(),
                    environment: ScanEnvironment::Native(os_name.clone()),
                    selected: false,
                    chat: agent.chat,
                    code_exec: agent.code_exec,
                });
            }
        }

    }

    /// Scan a WSL2 distro for known binaries
    fn scan_wsl(&self, distro: &str, results: &mut ScanResults) {
        for agent in known_agents() {
            if let Some(abs_path) = self.find_binary_wsl(distro, agent.binary) {
                info!("  ✓ Found {} in WSL:{}", agent.display_name, distro);
                results.agents.push(DiscoveredAgent {
                    binary: agent.binary.to_string(),
                    executable_path: abs_path,
                    display_name: agent.display_name.to_string(),
                    description: agent.description.to_string(),
                    environment: ScanEnvironment::Wsl(distro.to_string()),
                    selected: false,
                    chat: agent.chat,
                    code_exec: agent.code_exec,
                });
            }
        }
    }

    /// Scan an SSH host for known binaries
    fn scan_ssh(&self, host: &str, user: &str, results: &mut ScanResults) {
        for agent in known_agents() {
            if let Some(path) = self.find_binary_ssh(host, user, agent.binary) {
                info!("  ✓ Found {} on {}@{}", agent.display_name, user, host);
                results.agents.push(DiscoveredAgent {
                    binary: agent.binary.to_string(),
                    executable_path: path,
                    display_name: agent.display_name.to_string(),
                    description: agent.description.to_string(),
                    environment: ScanEnvironment::Ssh {
                        host: host.to_string(),
                        user: user.to_string(),
                    },
                    selected: false,
                    chat: agent.chat,
                    code_exec: agent.code_exec,
                });
            }
        }
    }



    /// Check if a binary exists on native Windows PATH
    fn binary_exists_native(&self, binary: &str) -> bool {
        Command::new("where")
            .arg(binary)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Check if a binary exists in a WSL distro and return its path
    fn find_binary_wsl(&self, distro: &str, binary: &str) -> Option<String> {
        // Use bash -ic to source .bashrc where node/npm paths are typically stored.
        // Fallbacks included for .npm-global and nvm paths explicitly.
        let script = format!(
            "which {0} 2>/dev/null || ( [ -f ~/.local/bin/{0} ] && echo ~/.local/bin/{0} ) || ( [ -f ~/.cargo/bin/{0} ] && echo ~/.cargo/bin/{0} ) || ( [ -f /usr/local/bin/{0} ] && echo /usr/local/bin/{0} ) || ( [ -f ~/.npm-global/bin/{0} ] && echo ~/.npm-global/bin/{0} )",
            binary
        );
        
        let output = Command::new("wsl")
            .args(["-d", distro, "--", "bash", "-ic", &script])
            .output()
            .ok()?;

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
        None
    }

    /// Check if a binary exists on an SSH host and return its path
    fn find_binary_ssh(&self, host: &str, user: &str, binary: &str) -> Option<String> {
        let script = format!(
            "which {0} 2>/dev/null || ( [ -f ~/.local/bin/{0} ] && echo ~/.local/bin/{0} ) || ( [ -f ~/.cargo/bin/{0} ] && echo ~/.cargo/bin/{0} ) || ( [ -f /usr/local/bin/{0} ] && echo /usr/local/bin/{0} ) || ( [ -f ~/.npm-global/bin/{0} ] && echo ~/.npm-global/bin/{0} )",
            binary
        );
        
        let output = Command::new("ssh")
            .args([
                "-o", "ConnectTimeout=3",
                "-o", "BatchMode=yes",
                &format!("{}@{}", user, host),
                "bash", "-lc", &script,
            ])
            .output()
            .ok()?;

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
        None
    }


}

// ── Manifest generation from discovered agents ───────────────

impl DiscoveredAgent {
    /// Generate a unique agent name for the manifest (e.g. "claude-wsl-ubuntu")
    pub fn manifest_name(&self) -> String {
        let env_suffix = match &self.environment {
            ScanEnvironment::Native(_) => "native".to_string(),
            ScanEnvironment::Wsl(distro) => format!("wsl-{}", distro.to_lowercase()),
            ScanEnvironment::Ssh { host, .. } => {
                // Use last octet or hostname
                let short = host.rsplit('.').next().unwrap_or(host);
                format!("ssh-{}", short)
            }
        };
        format!("{}-{}", self.binary, env_suffix)
    }

    /// Generate a TOML manifest for this agent
    pub fn to_manifest(&self) -> AgentManifest {
        let (shell, wsl_distro, ssh_host, ssh_user) = match &self.environment {
            ScanEnvironment::Native(_) => (ShellType::Native, None, None, None),
            ScanEnvironment::Wsl(distro) => (ShellType::Wsl, Some(distro.clone()), None, None),
            ScanEnvironment::Ssh { host, user } => {
                (ShellType::Ssh, None, Some(host.clone()), Some(user.clone()))
            }
        };

        // Determine communication modes and default args based on binary name
        let (input_mode, output_mode, output_filter, extra_args) = match self.binary.as_str() {
            "openclaw" => (
                // OpenClaw headless CLI: `openclaw agent --agent main --local --json "prompt"`
                // Returns JSON with response at payloads.0.text
                SubprocessInputMode::Argv,
                SubprocessOutputMode::Json,
                Some("finalAssistantVisibleText".to_string()),
                vec!["agent".to_string(), "--agent".to_string(), "main".to_string(),
                     "--local".to_string(), "--json".to_string(), "-m".to_string()],
            ),
            "kimi" => (
                SubprocessInputMode::Argv,
                SubprocessOutputMode::Raw,
                None,
                vec!["--quiet".to_string(), "--prompt".to_string()],
            ),
            "gemini" => (
                // Gemini CLI requires -p/--prompt for non-interactive headless mode.
                // Without -p, it launches interactive mode which hangs as a subprocess.
                // -p must be LAST in args because it takes the next arg (the prompt)
                // as its value: `gemini --yolo -p "prompt text"`
                SubprocessInputMode::Argv,
                SubprocessOutputMode::Raw,
                None,
                vec!["-p".to_string()],
            ),
            "claude" | "aider" => (
                SubprocessInputMode::Argv,
                SubprocessOutputMode::Raw,
                None,
                vec![],
            ),
            "hermes" => (
                SubprocessInputMode::Argv,
                SubprocessOutputMode::Raw,
                None,
                vec!["chat".to_string(), "-Q".to_string(), "-q".to_string()],
            ),
            _ => (SubprocessInputMode::Json, SubprocessOutputMode::Raw, None, vec![]),
        };

        // Translate executable path to WSL absolute mount point if needed.
        // Paths from `which` inside WSL are already absolute Linux paths.
        // Only Windows-style paths (C:\...) need conversion.
        let mut exec_path = self.executable_path.clone();
        if let ScanEnvironment::Wsl(_) = &self.environment {
            if let Some(pos) = exec_path.find(":\\") {
                let drive = exec_path[..pos].to_lowercase();
                let path = exec_path[pos + 2..].replace('\\', "/");
                exec_path = format!("/mnt/{}/{}", drive, path);
            }
            // If it starts with ~ expand it to ensure it's absolute
            if exec_path.starts_with("~/") {
                exec_path = exec_path.replacen("~/", "/home/$USER/", 1);
            }
        }

        // All agents use subprocess driver (including OpenClaw in headless CLI mode)
        let (driver, subprocess, tmux) = (
            DriverType::Subprocess,
            Some(SubprocessSection {
                command: exec_path,
                args: extra_args,
                working_dir: None,
                shell,
                wsl_distro,
                ssh_host,
                ssh_user,
                ssh_key: None,
                env: HashMap::new(),
                input_mode,
                output_mode,
                output_filter,
            }),
            None,
        );

        AgentManifest {
            agent: AgentSection {
                name: self.manifest_name(),
                version: "1.0.0".to_string(),
                description: self.description.clone(),
                driver,
            },
            capabilities: CapabilitiesSection {
                chat: self.chat,
                code_execution: self.code_exec,
                ..Default::default()
            },
            model: None,
            auth: None,
            subprocess,
            tmux,
            channels: Some(ChannelsSection {
                auto_join: vec!["general".to_string()],
            }),
        }
    }

    /// Write the manifest to disk
    pub fn write_manifest(&self, plugins_dir: &std::path::Path) -> crate::Result<PathBuf> {
        let name = self.manifest_name();
        let dir = plugins_dir.join("agents").join(&name);
        std::fs::create_dir_all(&dir)?;

        let manifest = self.to_manifest();
        let toml_str = toml::to_string_pretty(&manifest)
            .map_err(|e| crate::AgentError::Manifest(format!("Serialize error: {}", e)))?;

        let path = dir.join("agent.toml");
        std::fs::write(&path, toml_str)?;
        info!("  ✓ Created {}", path.display());
        Ok(path)
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_agents_not_empty() {
        assert!(!known_agents().is_empty());
    }

    #[test]
    fn test_manifest_name_native() {
        let agent = DiscoveredAgent {
            binary: "claude".to_string(),
            executable_path: "claude".to_string(),
            display_name: "Claude Code".to_string(),
            description: "test".to_string(),
            environment: ScanEnvironment::Native("Windows".to_string()),
            selected: true,
            chat: true,
            code_exec: true,
        };
        assert_eq!(agent.manifest_name(), "claude-native");
    }

    #[test]
    fn test_manifest_name_wsl() {
        let agent = DiscoveredAgent {
            binary: "gemini".to_string(),
            executable_path: "/home/user/.local/bin/gemini".to_string(),
            display_name: "Gemini CLI".to_string(),
            description: "test".to_string(),
            environment: ScanEnvironment::Wsl("Ubuntu".to_string()),
            selected: true,
            chat: true,
            code_exec: false,
        };
        assert_eq!(agent.manifest_name(), "gemini-wsl-ubuntu");
    }

    #[test]
    fn test_manifest_name_ssh() {
        let agent = DiscoveredAgent {
            binary: "hermes".to_string(),
            executable_path: "/bin/hermes".to_string(),
            display_name: "Hermes".to_string(),
            description: "test".to_string(),
            environment: ScanEnvironment::Ssh {
                host: "192.168.50.1".to_string(),
                user: "dgx".to_string(),
            },
            selected: true,
            chat: true,
            code_exec: true,
        };
        assert_eq!(agent.manifest_name(), "hermes-ssh-1");
    }

    #[test]
    fn test_to_manifest_subprocess() {
        let agent = DiscoveredAgent {
            binary: "claude".to_string(),
            executable_path: "/home/user/.local/bin/claude".to_string(),
            display_name: "Claude Code".to_string(),
            description: "Claude Code CLI".to_string(),
            environment: ScanEnvironment::Wsl("Ubuntu".to_string()),
            selected: true,
            chat: true,
            code_exec: true,
        };
        let m = agent.to_manifest();
        assert_eq!(m.agent.driver, DriverType::Subprocess);
        assert_eq!(m.subprocess.as_ref().unwrap().shell, ShellType::Wsl);
        assert_eq!(
            m.subprocess.as_ref().unwrap().wsl_distro.as_deref(),
            Some("Ubuntu")
        );
    }

    #[test]
    fn test_write_manifest_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let agent = DiscoveredAgent {
            binary: "claude".to_string(),
            executable_path: "claude".to_string(),
            display_name: "Claude Code".to_string(),
            description: "test".to_string(),
            environment: ScanEnvironment::Native("Linux".to_string()),
            selected: true,
            chat: true,
            code_exec: true,
        };
        let path = agent.write_manifest(tmp.path()).unwrap();
        assert!(path.exists());

        // Verify it's valid TOML that can be re-parsed
        let content = std::fs::read_to_string(&path).unwrap();
        let _: AgentManifest = toml::from_str(&content).unwrap();
    }
}
