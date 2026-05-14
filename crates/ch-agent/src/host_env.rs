//! Host environment discovery + caching.
//!
//! The fundamental problem this module solves: `ssh user@host /path/to/binary`
//! and `wsl -e /path/to/binary` both spawn the binary in a non-interactive
//! shell that has NOT loaded the user's `.bashrc` (no nvm/fnm/asdf init,
//! no PATH augmentation).  When the binary's shebang is `#!/usr/bin/env node`
//! and `node` lives in `~/.nvm/versions/node/.../bin/`, this fails with
//! `exit 127: node: no such file or directory`.
//!
//! Rather than hardcoding a list of common version-manager paths (the
//! previous `SHELL_SETUP_PRELUDE` approach, which doesn't generalize across
//! installation methods — volta, asdf, mise, n, pkgx, etc.), we **probe
//! each host once** for the user's actual interactive `$PATH` (and a handful
//! of related vars), cache the result, and prefix every subsequent agent
//! invocation with `env PATH=<cached> NVM_DIR=<cached> ... <cmd> <args>`.
//!
//! This works for any user, any version manager, any installation method —
//! whatever their `~/.bashrc` sets up is what we use.
//!
//! ## Cache layers
//!
//! 1. **In-memory** (process lifetime): an `Arc<DashMap<HostKey, HostEnv>>`
//!    so concurrent agent loads against the same host share one probe.
//! 2. **On-disk** (`~/.crow-hub/env-cache/<host>.env`): persists across
//!    process restarts.  User can refresh by deleting the file (or via
//!    the `crow refresh-env` CLI command — see ch-tui).
//!
//! ## Probe command
//!
//! ```text
//! ssh user@host "PS1='$ ' bash -lc 'env'"
//! wsl -d <distro> -e bash -lc "PS1='$ '; env"
//! ```
//!
//! The `PS1='$ '` trick fakes interactive mode so `[ -z "$PS1" ] && return`
//! guards in `~/.bashrc` don't bail before the user's nvm/fnm/etc. init.

use crate::{AgentError, Result};
use dashmap::DashMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tracing::{debug, info, warn};

// ── HostKey ──────────────────────────────────────────────────

/// Identifies a host's execution context for env caching.
///
/// Two agents that share a `HostKey` (e.g. multiple Claude/Gemini/Kimi
/// agents on the same `Ubuntu` WSL distro) share one probe.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HostKey {
    /// Run on the local machine.
    Native,
    /// Run inside a WSL distro (e.g. `Ubuntu`).
    Wsl(String),
    /// Run over SSH.  Format: `user@host` (or `host` if no user given).
    Ssh(String),
}

impl HostKey {
    /// Filename-safe key for the on-disk cache file.
    pub fn cache_filename(&self) -> String {
        match self {
            HostKey::Native => "native.env".to_string(),
            HostKey::Wsl(distro) => format!("wsl-{}.env", sanitize(distro)),
            HostKey::Ssh(target) => format!("ssh-{}.env", sanitize(target)),
        }
    }

    /// Human-readable description for logs and error messages.
    pub fn label(&self) -> String {
        match self {
            HostKey::Native => "native".to_string(),
            HostKey::Wsl(d) => format!("wsl:{}", d),
            HostKey::Ssh(t) => format!("ssh:{}", t),
        }
    }
}

/// Replace any character that isn't `[A-Za-z0-9._-]` with `_`.
/// Keeps cache filenames safe across Windows / Linux / macOS.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// ── HostEnv ──────────────────────────────────────────────────

/// A captured snapshot of the env vars we forward to remote invocations.
///
/// We capture a small allow-list — primarily `PATH` (the only one that
/// matters for shebang resolution) plus a few common vars that
/// version-managed tools sometimes need (`NVM_DIR`, `VOLTA_HOME`, etc.).
/// We deliberately do NOT capture shell internals (`PS1`, `OLDPWD`, etc.)
/// because forwarding them is either useless or harmful.
#[derive(Debug, Clone, Default)]
pub struct HostEnv {
    pub vars: HashMap<String, String>,
}

impl HostEnv {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    /// Returns the captured `$PATH`, or an empty string if not present.
    pub fn path(&self) -> &str {
        self.vars.get("PATH").map(|s| s.as_str()).unwrap_or("")
    }

    /// Iterate captured vars in deterministic (sorted) order — useful for
    /// stable `env KEY=VALUE` invocations and stable cache files.
    pub fn iter_sorted(&self) -> Vec<(&String, &String)> {
        let mut entries: Vec<_> = self.vars.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries
    }
}

/// Env-var allow-list for the probe.  We capture these from the remote
/// shell's `env` output if present and discard everything else.
///
/// The list is conservative: `PATH` is the only must-have for shebang
/// resolution; the rest support edge cases (e.g. some asdf/mise hooks
/// need `*_HOME`/`*_DIR` to find their version files).
const ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "NVM_DIR",
    "NVM_BIN",
    "NVM_INC",
    "NODE_PATH",
    "NODE_VERSION",
    "VOLTA_HOME",
    "ASDF_DATA_DIR",
    "ASDF_CONFIG_FILE",
    "ASDF_DIR",
    "MISE_DATA_DIR",
    "RTX_DATA_DIR",
    "PYENV_ROOT",
    "PYENV_VERSION",
    "RBENV_ROOT",
    "RBENV_VERSION",
    "GOPATH",
    "GOROOT",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "HOMEBREW_PREFIX",
    "HOMEBREW_CELLAR",
    "HOMEBREW_REPOSITORY",
    "DENO_INSTALL",
    "BUN_INSTALL",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
];

fn is_allowed(name: &str) -> bool {
    ENV_ALLOWLIST.contains(&name)
}

// ── HostEnvCache ─────────────────────────────────────────────

/// Two-tier env cache: in-memory first, then on-disk, then probe.
pub struct HostEnvCache {
    in_memory: Arc<DashMap<HostKey, HostEnv>>,
    cache_dir: PathBuf,
}

impl HostEnvCache {
    /// Create a cache rooted at `~/.crow-hub/env-cache/`.
    pub fn new() -> Self {
        let cache_dir = home_cache_dir();
        let _ = std::fs::create_dir_all(&cache_dir);
        Self {
            in_memory: Arc::new(DashMap::new()),
            cache_dir,
        }
    }

    /// Create a cache rooted at a specific directory (for tests).
    #[cfg(test)]
    pub fn with_dir(cache_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&cache_dir);
        Self {
            in_memory: Arc::new(DashMap::new()),
            cache_dir,
        }
    }

    /// Get the env for a host: in-memory → on-disk → probe.
    ///
    /// Returns an empty `HostEnv` if probing fails — callers should treat
    /// the env as "best effort" and fall back to whatever PATH the remote
    /// non-interactive shell happens to have.
    pub fn get_or_probe(&self, key: &HostKey) -> HostEnv {
        // 1. In-memory hit
        if let Some(entry) = self.in_memory.get(key) {
            return entry.value().clone();
        }

        // 2. Disk hit
        let path = self.cache_dir.join(key.cache_filename());
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some(env) = parse_cache_file(&content) {
                debug!("[host_env] disk cache hit for {}", key.label());
                self.in_memory.insert(key.clone(), env.clone());
                return env;
            }
        }

        // 3. Probe
        info!("[host_env] probing {} (one-time)", key.label());
        let env = match probe(key) {
            Ok(env) => env,
            Err(e) => {
                warn!(
                    "[host_env] probe failed for {}: {} — falling back to empty env",
                    key.label(),
                    e
                );
                HostEnv::new()
            }
        };

        // Persist
        if let Err(e) = save_cache_file(&path, &env) {
            warn!(
                "[host_env] failed to persist cache for {}: {}",
                key.label(),
                e
            );
        }
        self.in_memory.insert(key.clone(), env.clone());

        env
    }

    /// Drop the cache entry for a host (in-memory + on-disk).
    /// Next `get_or_probe` will re-probe.
    pub fn invalidate(&self, key: &HostKey) -> Result<()> {
        self.in_memory.remove(key);
        let path = self.cache_dir.join(key.cache_filename());
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                AgentError::Driver(format!("failed to remove {}: {}", path.display(), e))
            })?;
        }
        info!("[host_env] invalidated cache for {}", key.label());
        Ok(())
    }

    /// Drop ALL cached host envs.
    pub fn invalidate_all(&self) -> Result<()> {
        self.in_memory.clear();
        if self.cache_dir.exists() {
            for entry in std::fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("env") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        info!("[host_env] invalidated all cached host envs");
        Ok(())
    }
}

impl Default for HostEnvCache {
    fn default() -> Self {
        Self::new()
    }
}

fn home_cache_dir() -> PathBuf {
    ch_core::get_home_dir().join("env-cache")
}

// ── Probe ────────────────────────────────────────────────────

/// Init script run during host-env probe.  Goal: end up with the user's
/// "interactive" `$PATH` so any binary they can run from a terminal
/// (nvm, volta, asdf, mise, brew, custom) is also resolvable when the
/// driver execs the agent.
///
/// Strategy is best-effort, layered:
///
/// 1. **`PS1='$ '`** — bypass the common `[ -z "$PS1" ] && return` guard
///    in `~/.bashrc`, so the user's interactive init code runs.
/// 2. **`source ~/.bashrc`** — primary path: the user's actual shell config.
/// 3. **Explicit version-manager fallbacks** (nvm.sh, fnm env, asdf, mise) —
///    in case the user's init is more guarded than just the `$PS1` check.
///    These ARE specific to known managers, but they only run if the
///    manager's init script exists; users with no version manager
///    just see them no-op.
/// 4. **`env`** — print the resulting environment for capture.
///
/// We deliberately do NOT hardcode PATH entries (e.g. `:/home/.../bin`):
/// the prior approach of stuffing fixed paths into PATH was the brittle
/// thing this whole probe-and-cache mechanism was designed to replace.
/// We only invoke version-manager init scripts that the user already
/// installed; we don't invent paths.
const PROBE_INIT_SCRIPT: &str = "\
export PS1='$ '; \
[ -f ~/.bashrc ] && . ~/.bashrc 2>/dev/null; \
[ -s ~/.nvm/nvm.sh ] && . ~/.nvm/nvm.sh 2>/dev/null; \
if command -v fnm >/dev/null 2>&1; then \
  eval \"$(fnm env --shell bash 2>/dev/null)\" 2>/dev/null; \
elif [ -x ~/.local/share/fnm/fnm ]; then \
  eval \"$(~/.local/share/fnm/fnm env --shell bash 2>/dev/null)\" 2>/dev/null; \
fi; \
[ -s ~/.asdf/asdf.sh ] && . ~/.asdf/asdf.sh 2>/dev/null; \
[ -s ~/.config/asdf/asdf.sh ] && . ~/.config/asdf/asdf.sh 2>/dev/null; \
if command -v mise >/dev/null 2>&1; then \
  eval \"$(mise env -s bash 2>/dev/null)\" 2>/dev/null; \
fi; \
env";

/// Run the env probe for a host and return the captured allow-listed vars.
fn probe(key: &HostKey) -> Result<HostEnv> {
    match key {
        HostKey::Native => probe_native(),
        HostKey::Wsl(distro) => probe_wsl(distro),
        HostKey::Ssh(target) => probe_ssh(target),
    }
}

fn probe_native() -> Result<HostEnv> {
    let mut env = HostEnv::new();
    for (k, v) in std::env::vars() {
        if is_allowed(&k) {
            env.vars.insert(k, v);
        }
    }
    Ok(env)
}

fn probe_wsl(distro: &str) -> Result<HostEnv> {
    let output = Command::new("wsl.exe")
        .args(["-d", distro, "--", "bash", "-lc", PROBE_INIT_SCRIPT])
        .output()
        .map_err(|e| AgentError::Driver(format!("wsl probe spawn failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AgentError::Driver(format!(
            "wsl probe for {} exited {}: {}",
            distro,
            output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".to_string()),
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_env_output(&stdout))
}

fn probe_ssh(target: &str) -> Result<HostEnv> {
    let output = Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "StrictHostKeyChecking=accept-new",
            target,
            PROBE_INIT_SCRIPT,
        ])
        .output()
        .map_err(|e| AgentError::Driver(format!("ssh probe spawn failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AgentError::Driver(format!(
            "ssh probe for {} exited {}: {}",
            target,
            output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".to_string()),
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_env_output(&stdout))
}

/// Parse the `env`-style output (`KEY=VALUE` per line) into a HostEnv,
/// keeping only allow-listed names and skipping malformed lines.
pub(crate) fn parse_env_output(output: &str) -> HostEnv {
    let mut env = HostEnv::new();
    for line in output.lines() {
        let line = line.trim_end_matches('\r');
        // Skip lines without `=` (rare, e.g. shell prompt echoes from .bashrc).
        let Some(eq_pos) = line.find('=') else {
            continue;
        };
        let key = &line[..eq_pos];
        let value = &line[eq_pos + 1..];
        // Skip lines where the key isn't a valid env-var name (e.g. function definitions).
        if key.is_empty()
            || !key
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            continue;
        }
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        if is_allowed(key) {
            env.vars.insert(key.to_string(), value.to_string());
        }
    }
    env
}

// ── Disk cache I/O ───────────────────────────────────────────

/// Cache file format:
///   `# crow-hub host env cache`
///   `# Generated: <ISO 8601 timestamp>`
///   `KEY=VALUE`
///   ...
///
/// Comments (lines starting with `#`) and blank lines are ignored on read.
fn save_cache_file(path: &std::path::Path, env: &HostEnv) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(path)?;
    writeln!(f, "# crow-hub host env cache")?;
    writeln!(f, "# Generated: {}", chrono::Utc::now().to_rfc3339())?;
    writeln!(
        f,
        "# To refresh, delete this file (or run `crow refresh-env`)."
    )?;
    writeln!(f)?;
    for (k, v) in env.iter_sorted() {
        writeln!(f, "{}={}", k, v)?;
    }
    Ok(())
}

fn parse_cache_file(content: &str) -> Option<HostEnv> {
    let mut env = HostEnv::new();
    let mut found_any = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(eq_pos) = line.find('=') else {
            continue;
        };
        let key = &line[..eq_pos];
        let value = &line[eq_pos + 1..];
        if !is_allowed(key) {
            continue;
        }
        env.vars.insert(key.to_string(), value.to_string());
        found_any = true;
    }
    if found_any {
        Some(env)
    } else {
        None
    }
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_key_cache_filenames_are_safe() {
        assert_eq!(HostKey::Native.cache_filename(), "native.env");
        assert_eq!(
            HostKey::Wsl("Ubuntu".into()).cache_filename(),
            "wsl-Ubuntu.env"
        );
        assert_eq!(
            HostKey::Ssh("zhiqing@192.168.50.1".into()).cache_filename(),
            "ssh-zhiqing_192.168.50.1.env"
        );
        // Pathological characters get sanitized
        assert_eq!(
            HostKey::Ssh("user/with:slash".into()).cache_filename(),
            "ssh-user_with_slash.env"
        );
    }

    #[test]
    fn parse_env_output_filters_to_allowlist() {
        let stdout = "PATH=/usr/local/bin:/usr/bin\n\
                      PS1=$ \n\
                      OLDPWD=/tmp\n\
                      HOME=/home/user\n\
                      NVM_DIR=/home/user/.nvm\n\
                      RANDOM=12345\n\
                      LANG=en_US.UTF-8\n";
        let env = parse_env_output(stdout);
        assert_eq!(env.vars.get("PATH").unwrap(), "/usr/local/bin:/usr/bin");
        assert_eq!(env.vars.get("HOME").unwrap(), "/home/user");
        assert_eq!(env.vars.get("NVM_DIR").unwrap(), "/home/user/.nvm");
        assert_eq!(env.vars.get("LANG").unwrap(), "en_US.UTF-8");
        // Disallowed vars filtered out
        assert!(env.vars.get("PS1").is_none());
        assert!(env.vars.get("OLDPWD").is_none());
        assert!(env.vars.get("RANDOM").is_none());
    }

    #[test]
    fn parse_env_output_skips_malformed_lines() {
        let stdout = "PATH=/usr/bin\n\
                      garbage line with no equals\n\
                      =empty-key\n\
                      9STARTING_WITH_DIGIT=foo\n\
                      KEY-WITH-DASH=bar\n\
                      _UNDER=ok\n\
                      HOME=/h\n";
        // Only PATH and HOME are allow-listed AND well-formed; _UNDER is well-formed
        // but not allow-listed.
        let env = parse_env_output(stdout);
        assert_eq!(env.vars.len(), 2);
        assert!(env.vars.contains_key("PATH"));
        assert!(env.vars.contains_key("HOME"));
    }

    #[test]
    fn parse_env_output_handles_values_with_equals() {
        // PATH segments don't have `=`, but some env values do (e.g. JAVA_TOOL_OPTIONS).
        // We only split on the FIRST `=` so the value preserves embedded ones.
        let stdout = "LANG=foo=bar=baz\n";
        let env = parse_env_output(stdout);
        assert_eq!(env.vars.get("LANG").unwrap(), "foo=bar=baz");
    }

    #[test]
    fn parse_cache_file_skips_comments_and_blanks() {
        let content = "# crow-hub host env cache\n\
                       # Generated: 2026-05-10T...\n\
                       \n\
                       PATH=/usr/bin\n\
                       \n\
                       HOME=/home/u\n\
                       # trailing comment\n";
        let env = parse_cache_file(content).expect("should parse");
        assert_eq!(env.vars.get("PATH").unwrap(), "/usr/bin");
        assert_eq!(env.vars.get("HOME").unwrap(), "/home/u");
    }

    #[test]
    fn parse_cache_file_returns_none_when_empty() {
        // An entirely-comment file is treated as no cache (re-probe).
        let content = "# only comments\n# nothing useful\n";
        assert!(parse_cache_file(content).is_none());
    }

    #[test]
    fn cache_round_trip_through_disk_and_memory() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = HostEnvCache::with_dir(tmp.path().to_path_buf());

        // Pre-seed by writing a cache file directly — no probe is triggered.
        let key = HostKey::Ssh("alice@example.invalid".into());
        let content = "PATH=/opt/foo/bin:/usr/bin\nHOME=/home/alice\n";
        std::fs::write(tmp.path().join(key.cache_filename()), content).unwrap();

        // First call: disk hit.
        let env = cache.get_or_probe(&key);
        assert_eq!(env.vars.get("PATH").unwrap(), "/opt/foo/bin:/usr/bin");
        assert_eq!(env.vars.get("HOME").unwrap(), "/home/alice");

        // Second call: in-memory hit (we delete the disk file to prove this).
        std::fs::remove_file(tmp.path().join(key.cache_filename())).unwrap();
        let env2 = cache.get_or_probe(&key);
        assert_eq!(env2.vars.get("PATH").unwrap(), "/opt/foo/bin:/usr/bin");
    }

    #[test]
    fn invalidate_clears_in_memory_and_disk_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = HostEnvCache::with_dir(tmp.path().to_path_buf());

        let key = HostKey::Wsl("Ubuntu".into());
        let cache_file = tmp.path().join(key.cache_filename());

        // Pre-seed disk + in-memory.
        std::fs::write(&cache_file, "PATH=/usr/bin\n").unwrap();
        let _ = cache.get_or_probe(&key);
        assert!(cache_file.exists());

        cache.invalidate(&key).unwrap();
        assert!(!cache_file.exists(), "disk cache file should be removed");

        // After invalidate, in-memory is gone — re-seed disk and verify the
        // next call disk-hits again (proves in-memory was cleared).
        std::fs::write(&cache_file, "PATH=/different/path\n").unwrap();
        let env = cache.get_or_probe(&key);
        assert_eq!(env.vars.get("PATH").unwrap(), "/different/path");
    }

    #[test]
    fn invalidate_all_clears_every_cache_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = HostEnvCache::with_dir(tmp.path().to_path_buf());

        let k1 = HostKey::Wsl("Ubuntu".into());
        let k2 = HostKey::Ssh("user@host.invalid".into());
        std::fs::write(tmp.path().join(k1.cache_filename()), "PATH=/a\n").unwrap();
        std::fs::write(tmp.path().join(k2.cache_filename()), "PATH=/b\n").unwrap();
        let _ = cache.get_or_probe(&k1);
        let _ = cache.get_or_probe(&k2);

        cache.invalidate_all().unwrap();

        // Both files are gone.
        assert!(!tmp.path().join(k1.cache_filename()).exists());
        assert!(!tmp.path().join(k2.cache_filename()).exists());
    }
}
