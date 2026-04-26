//! Plugin Loader
//!
//! Scans `plugins/agents/` directory for agent manifests
//! and loads them into the runtime.

use crate::manifest::AgentManifest;
use crate::{AgentError, Result};
use std::path::{Path, PathBuf};
use tracing::{info, warn, debug};

/// Scans plugin directories and loads agent manifests
pub struct PluginLoader {
    /// Base directory containing plugin folders
    plugins_dir: PathBuf,
}

impl PluginLoader {
    /// Create a new loader pointing at the plugins directory
    pub fn new(plugins_dir: impl Into<PathBuf>) -> Self {
        Self {
            plugins_dir: plugins_dir.into(),
        }
    }

    /// Scan for all agent manifests in plugins/agents/
    pub fn scan(&self) -> Result<Vec<LoadedPlugin>> {
        let agents_dir = self.plugins_dir.join("agents");

        if !agents_dir.exists() {
            info!("No plugins/agents directory found at {:?}, creating it", agents_dir);
            std::fs::create_dir_all(&agents_dir)?;
            return Ok(Vec::new());
        }

        let mut plugins = Vec::new();

        let entries = std::fs::read_dir(&agents_dir)
            .map_err(|e| AgentError::Io(e))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let manifest_path = path.join("agent.toml");
            if !manifest_path.exists() {
                debug!("Skipping {:?} — no agent.toml found", path);
                continue;
            }

            match AgentManifest::from_file(&manifest_path) {
                Ok(manifest) => {
                    info!(
                        "Loaded agent plugin: {} (driver: {:?})",
                        manifest.agent.name, manifest.agent.driver
                    );
                    plugins.push(LoadedPlugin {
                        manifest,
                        plugin_dir: path,
                        manifest_path,
                    });
                }
                Err(e) => {
                    warn!("Failed to load manifest {:?}: {}", manifest_path, e);
                }
            }
        }

        info!("Loaded {} agent plugin(s)", plugins.len());
        Ok(plugins)
    }

    /// Load a single plugin from a directory
    pub fn load_single(&self, agent_name: &str) -> Result<LoadedPlugin> {
        let agent_dir = self.plugins_dir.join("agents").join(agent_name);
        let manifest_path = agent_dir.join("agent.toml");

        if !manifest_path.exists() {
            return Err(AgentError::NotFound(format!(
                "No agent.toml at {:?}", manifest_path
            )));
        }

        let manifest = AgentManifest::from_file(&manifest_path)?;
        Ok(LoadedPlugin {
            manifest,
            plugin_dir: agent_dir,
            manifest_path,
        })
    }
}

/// A successfully loaded plugin (manifest + location)
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    /// Parsed manifest
    pub manifest: AgentManifest,
    /// Directory containing the plugin
    pub plugin_dir: PathBuf,
    /// Path to the manifest file
    pub manifest_path: PathBuf,
}

impl LoadedPlugin {
    /// Get the agent name
    pub fn name(&self) -> &str {
        &self.manifest.agent.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scan_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let loader = PluginLoader::new(tmp.path());
        let plugins = loader.scan().unwrap();
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_scan_with_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");
        let claude_dir = agents_dir.join("claude-api");
        fs::create_dir_all(&claude_dir).unwrap();

        let manifest = r#"
[agent]
name = "claude-api"
driver = "api"
description = "Claude via API"

[model]
default = "claude-sonnet-4-6"

[capabilities]
chat = true
"#;
        fs::write(claude_dir.join("agent.toml"), manifest).unwrap();

        let loader = PluginLoader::new(tmp.path());
        let plugins = loader.scan().unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name(), "claude-api");
    }

    #[test]
    fn test_scan_skips_bad_manifests() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents");

        // Good plugin
        let good = agents_dir.join("good");
        fs::create_dir_all(&good).unwrap();
        fs::write(good.join("agent.toml"), r#"
[agent]
name = "good"
driver = "api"
[model]
default = "gpt-4"
"#).unwrap();

        // Bad plugin (missing required fields)
        let bad = agents_dir.join("bad");
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join("agent.toml"), "invalid toml {{{{").unwrap();

        let loader = PluginLoader::new(tmp.path());
        let plugins = loader.scan().unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name(), "good");
    }

    #[test]
    fn test_load_single() {
        let tmp = tempfile::tempdir().unwrap();
        let agents_dir = tmp.path().join("agents").join("test-agent");
        fs::create_dir_all(&agents_dir).unwrap();
        fs::write(agents_dir.join("agent.toml"), r#"
[agent]
name = "test-agent"
driver = "api"
[model]
default = "llama3"
"#).unwrap();

        let loader = PluginLoader::new(tmp.path());
        let plugin = loader.load_single("test-agent").unwrap();
        assert_eq!(plugin.name(), "test-agent");
    }
}
