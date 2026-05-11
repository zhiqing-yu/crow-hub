# Example Agent Manifests

This directory contains **template agent manifests** for the most common deployment
shapes. They are intended to be **copied** into `plugins/agents/<your-agent-name>/agent.toml`,
then customized to your local environment.

> `plugins/agents/` is **gitignored** — manifests there are user-specific (per-machine
> binary paths, SSH hosts, WSL distros, etc.) and not meant to be shared via git.
> Run `crow setup` from the repo root to auto-detect and generate manifests for the
> agents already installed on your system. Use the templates here when you need to
> add an agent the auto-scanner missed or to customize an unusual setup.

## Templates

| File | When to use |
|---|---|
| [`_template-native.toml`](_template-native.toml) | An agent CLI installed on the same machine running crow-hub (e.g. macOS-native, Linux-native, or Windows-native install). |
| [`_template-wsl.toml`](_template-wsl.toml) | An agent CLI installed inside a WSL distro (Linux running inside Windows). |
| [`_template-ssh.toml`](_template-ssh.toml) | An agent CLI installed on a remote machine reachable via SSH. |
| [`openclaw-json-example.toml`](openclaw-json-example.toml) | Demonstrates `output_filter` for agents that emit structured JSON instead of plain text. |
| [`codex-setup-script-example.toml`](codex-setup-script-example.toml) | Demonstrates the per-agent `setup_script` escape hatch — needed when the host-env probe can't capture some PATH the agent's shebang depends on (e.g. Homebrew prefix on a host whose `.bashrc` bails before `brew shellenv` runs). |

## How to add an agent from a template

1. **Pick a template** based on where your agent CLI is installed:
   ```bash
   cp examples/agents/_template-native.toml plugins/agents/my-agent/agent.toml
   ```
   The directory name (`my-agent` above) must match the manifest's `name` field.

2. **Edit the fields marked `<...>`** — the manifest is just TOML; everything is
   commented in the template files. The key fields:

   | Field | What to set |
   |---|---|
   | `name` | Unique name across all agents (matches the directory name) |
   | `command` | Absolute path to the agent binary on the target host |
   | `args` | Argv that the agent expects before the prompt (e.g. `["chat", "-q"]`) |
   | `shell` | `"native"` / `"wsl"` / `"ssh"` |
   | `wsl_distro` | (WSL only) the distro name from `wsl -l -v` |
   | `ssh_host`, `ssh_user` | (SSH only) the remote target |
   | `input_mode` | `"argv"` (one-shot per message) / `"json"` / `"plain"` (persistent stdin) |
   | `output_mode` | `"raw"` (just print) / `"json"` (parse + filter) |
   | `output_filter` | (JSON only) dotted path to the text field, e.g. `"payloads.0.text"` |
   | `setup_script` | (Optional) Shell snippet run before the agent — escape hatch for env the host-env cache can't capture |
   | `auto_join` | List of bus channels the agent subscribes to (usually `["general"]`) |

3. **Restart `crow`** — the runtime picks up `plugins/agents/*` on startup. The agent
   appears in the TUI's left sidebar; send it a message to confirm it responds.

4. **Diagnose if it doesn't work**:
   ```bash
   crow doctor my-agent
   ```
   The doctor command sends a one-shot test prompt directly through the driver
   (bypassing the bus and TUI) and prints the raw response or the full error
   detail — fastest way to tune your manifest.

## How the auto-scanner relates to these templates

`crow setup` runs the environment scanner, which probes:

- Your local PATH (and common version-manager paths: nvm, fnm, volta, asdf, mise,
  Homebrew, npm-global, cargo bin, etc.)
- Any WSL distros (`wsl -l -v`)
- Any SSH hosts configured in `~/.ssh/config` (best-effort)

For every known CLI agent it finds (`claude`, `gemini`, `kimi`, `openclaw`,
`codex`, `opencode`, `hermes`, …), it writes a manifest to `plugins/agents/`
using the discovered binary path and a default invocation shape.

**Use these templates when:**
- The scanner missed your agent (custom or new CLI)
- You want a non-default invocation (custom flags, JSON output filter, etc.)
- You're using a host the scanner can't reach (e.g. via a non-SSH transport)
