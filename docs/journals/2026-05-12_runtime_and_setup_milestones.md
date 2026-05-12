# Runtime, Setup, and Repo Milestones — 2026-04-26 → 2026-05-12

A short recap of how crow-hub went from a single laptop's local prototype to
a publicly cloneable, generically-configurable multi-agent hub.  Two
collaborators contributed across this window:

- **Gemini** — pre-2026-04-26 — built the foundational scaffolding
- **Claude Code** — 2026-04-26 → 2026-05-12 — drove iteration on top

---

## Phase 1 — Scaffolding (Gemini, → 2026-04-26)

Initial commit `83d47c4` (Apr 26) shipped a complete Rust workspace baseline:

- 9 crates: `ch-protocol`, `ch-core`, `ch-model`, `ch-agent`, `ch-adapter`,
  `ch-memory`, `ch-monitor`, `ch-tui`, `ch-gui`
- Three driver types: API, Subprocess (native / WSL / SSH), Tmux
- Environment scanner with `bash -lc` probing for nvm / fnm / Homebrew
- TUI with focus panels (Tab cycling), word wrap, mouse scroll, bracketed
  paste handling
- Message bus with channels and correlation IDs
- Manifest format + plugin loader
- Multi-platform GitHub Actions CI matrix (Linux / macOS / Windows)
- 26 unit tests

This was the largest single contribution to the project.  Everything below
builds on it.

---

## Phase 2 — Make WSL CLI agents actually respond (Claude Code, pre-upload)

The bus was wired but no WSL agent responded end-to-end.  Four root causes:

1. **Subprocess errors swallowed stderr** — every failure surfaced as
   `Driver error: exit 1` with no diagnostic
2. **TUI mouse capture leaked escape sequences** as `[[[[` floods on
   movement (Antigravity terminal couldn't parse the tracking sequences)
3. **Bracketed paste not enabled** — Ctrl+V pasted char-by-char
4. **Bus handler used blocking `chat()`** — no streaming feedback during
   long responses

Fixes:

- Subprocess driver: surface full diagnostic (invocation + stdout + stderr)
  on any non-zero exit
- TUI: enable bracketed paste; mouse capture restored carefully
- New `crow doctor <agent>` CLI subcommand for tight iteration without TUI
- Runtime: replace blocking `chat()` with `stream_chat()` and forward each
  non-empty chunk to the bus with shared `correlation_id`

After these changes all four WSL agents (Claude, Gemini, Kimi, OpenClaw)
responded.

---

## Phase 3 — Public on GitHub (2026-04-26 → 2026-05-09)

- Published `83d47c4` to **`github.com/zhiqing-yu/crow-hub`** as a public
  repo.  Original Apr 26 timestamp on the commit is preserved server-side
  by GitHub, anchoring authorship.
- Fixed broken CI: `dtolnay/rust-action` (non-existent) → `dtolnay/rust-toolchain`,
  deprecated `actions/upload-artifact@v3` → v4, wrong binary names
  (`ah` → `crow`, `crow-gui`)
- All 7 CI jobs green across the matrix

Commits: `7dc8b27`, `556fa88`, `8cf348b`

---

## Phase 4 — Live per-agent status in the TUI (2026-05-10)

Agents had no visible status — you'd send a message and stare at silence.

Added `AgentActivity` enum (Unknown / Idle / Thinking / Errored) tracked
per agent in the runtime.  The TUI agent list now shows:

| Glyph | Color | Meaning |
|:-:|---|---|
| `○` | gray | never spoken to |
| `●` | green | idle, last latency shown (e.g. `2.1s`) |
| `◐` | yellow | thinking, live elapsed counter (`12s…`) |
| `✗` | red | last request errored |

Latency is **time-to-first-chunk**, which matches what users perceive as
"the agent started responding."

Commit: `b35c14f` — 211 insertions, 6 new unit tests.

---

## Phase 5 — Generic PATH handling for any user's install (2026-05-10)

The core fix that made the project shareable.  Two iterations:

### First attempt — hardcoded prelude (`c1fb9bb`)

Built a `SHELL_SETUP_PRELUDE` constant that explicitly sourced `nvm.sh`,
`fnm env`, and stuffed `~/.cargo/bin`, `~/.npm-global/bin`,
`/home/linuxbrew/.linuxbrew/bin`, etc. into `$PATH`.  Solved the
"nvm-installed binary fails over SSH because `node` not in non-interactive
PATH" bug for our own setup.

**Problem**: it baked specific paths into the driver.  Users with volta /
asdf / mise / n / pkgx / custom installs would be invisible.

### Better — probe + cache (`36f2468`)

New `host_env.rs` module:

- **Probe**: on first agent load per host, run `bash -lc env` (with
  `PS1='$ '` trick to bypass `[ -z "$PS1" ] && return` guards), capture
  the user's actual interactive `$PATH` plus a small allow-list of
  related vars (HOME, NVM_DIR, VOLTA_HOME, ASDF_DATA_DIR, …)
- **Two-tier cache**: in-memory `DashMap` per process, persistent file
  at `~/.crow-hub/env-cache/<host>.env`
- **Driver**: prefix every invocation with `env KEY=VAL ... cmd args`
  (no shell wrapper needed, faster than `bash -c`)
- **`setup_script` manifest field**: per-agent escape hatch for the rare
  case the cache can't cover (e.g. activating a Python venv before exec)
- **`crow refresh-env [host]` CLI subcommand**: invalidate the cache

Generic across any version manager.  Zero-config for ~95% of users
(anyone whose `.bashrc` reflects their interactive setup), with the
`setup_script` field for the long tail.

End state: 10 of 11 configured agents responding through doctor.  The
one failure (`codex-ssh-1`) is a remote-host auth issue, not driver.

---

## Phase 6 — Repo cleanup for fresh clones (2026-05-11 → 2026-05-12)

A friend cloned on macOS and saw 11 agents in his TUI — all pointing
at `zhiqing@192.168.50.1`, `wsl_distro = Ubuntu`, etc.  Useless to him.
His own installed `openclaw` + `kimi` had no manifests.

Realization: **agent manifests are per-machine state, like `~/.ssh/config`.
They don't belong in git.**

PR #2 (`chore/strip-user-manifests-for-fresh-clones`):

- `git rm` all 11 committed manifests
- `.gitignore` rule: `plugins/agents/*` with `!plugins/agents/.gitkeep`
- `examples/agents/` — 5 tracked templates:
  - `_template-native.toml`, `_template-wsl.toml`, `_template-ssh.toml`
  - `openclaw-json-example.toml` (output_filter demo)
  - `codex-setup-script-example.toml` (setup_script demo)
  - `README.md` explaining when to use each
- README rewritten: clone → `cargo build` → `crow setup` → `crow`

After this PR, anyone clones the repo, runs `crow setup`, and gets a TUI
populated with **their** installed agents, on **their** hosts, using
**their** PATH.

---

## Test count over time

| Milestone | Total tests |
|---|---:|
| Initial commit | 26 |
| TUI status indicators | 32 |
| host-env probe + cache | 85 |
| PR #2 (no new code, just file moves) | 85 |

---

## What's next

- PR #2 review and merge
- A clean Mac run with friend's native openclaw + kimi as the validation
- **Phase 3** of the original roadmap: shared memory layer (semantic recall
  across sessions, SQLite + local embeddings)
- **Phase 4**: monitoring (token usage / cost / GPU metrics surfaced in TUI)
- Multi-agent broadcast — send the same prompt to a selected group of
  agents simultaneously; the bus + status indicators already support it,
  needs only a TUI shortcut to multi-select agents

---

## Acknowledgements

- **Gemini** — built the bedrock the rest of this stands on.  The TUI
  ergonomics, scanner robustness, and driver/manifest abstractions are
  all his.
- **Claude Code** — iteration on top, plus the discovery-cache architecture
  that made the project shareable.
