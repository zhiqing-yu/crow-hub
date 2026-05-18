# Day Plan for DeepSeek — 2026-05-18

> **Audience**: DeepSeek (or any next coding agent picking up the project).
> Self-contained — you do NOT need any prior conversation context to
> execute this.

---

## Where we are

Crow Hub is a Rust multi-agent orchestration hub.  The TUI works,
11 agents wired, host-env cache shipped, SQLite memory persistence
working, token counts displayed.  Two previous plans landed:
`docs/plans/2026-05-12_next_four_days.md` is fully complete.

Most recent journal: `docs/journals/2026-05-17_agents_panel_layout_fix.md`.
Read it first — it's short and explains the current state.

GitHub: https://github.com/zhiqing-yu/crow-hub

## Today's scope — 3 small tasks + 1 stretch

Bias: **multiple small, independent ships**, not one big feature.
Each task ends with its own commit landed directly on `main` (no PR
ceremony — the team has been shipping small fixes via direct push for
the last week).  Pick them up in order; if Task 1 takes 20 min you
have plenty of time for 2–4.

| # | Task | Effort | Why |
|---|------|-------:|-----|
| 1 | Fix Memory panel scroll inversion | 15 min | Long-standing UX bug, two-line fix |
| 2 | `pricing.toml` for cost estimation | 2-3 hrs | First $-visible feature, leverages existing token tracking |
| 3 | Parallel agent loading | 2 hrs | Cold-start 10s → ~3s, perceived perf win |
| 4 | (Stretch) Theme struct + 2 built-in themes | 2 hrs | Sets up extensibility for OpenCode-inspired UX direction |

Total: **~6-7 hrs realistic**.  Drop Task 4 if time-boxed.

---

## Pre-flight (5 minutes — do BEFORE Task 1)

```bash
cd <repo>
git fetch origin
git checkout main
git pull origin main
git status                            # must be clean
cargo test --workspace --lib          # must show 94 passing
```

If anything fails — stop and report.  Don't fix unrelated breakage.

Read these two files first:
- `docs/journals/2026-05-17_agents_panel_layout_fix.md` — what just shipped
- `docs/journals/2026-05-16_memory_browser_cli_and_deepseek_pickup.md` — context

---

## Task 1 — Memory panel scroll inversion (15 minutes)

**Bug**: in the TUI's Memory panel, pressing **↑ scrolls toward
newer messages** (down through history) and **↓ scrolls toward
older messages** (up through history).  Backwards from every chat
and log viewer convention.

**File**: `crates/ch-tui/src/app.rs`

**Find**: in `run_loop`, the Memory arms of the Up/Down match.  As
of commit `7464ce7` they look like:

```rust
KeyCode::Up => match app.focused_panel {
    ...
    FocusedPanel::Memory => {
        app.memory_scroll_offset = app.memory_scroll_offset.saturating_add(1);
    }
},
KeyCode::Down => match app.focused_panel {
    ...
    FocusedPanel::Memory => {
        app.memory_scroll_offset = app.memory_scroll_offset.saturating_sub(1);
    }
},
```

**Fix**: swap `add` ↔ `sub` for the Memory arms:

```rust
KeyCode::Up => match app.focused_panel {
    ...
    FocusedPanel::Memory => {
        app.memory_scroll_offset = app.memory_scroll_offset.saturating_sub(1);
    }
},
KeyCode::Down => match app.focused_panel {
    ...
    FocusedPanel::Memory => {
        app.memory_scroll_offset = app.memory_scroll_offset.saturating_add(1);
    }
},
```

Reasoning: rows are sorted oldest→newest in the render (`.iter().rev()`).
`offset` is `.skip(offset)`.  Increasing offset *skips more oldest*
items → reveals newer.  So:
- ↑ (move viewport toward older) should DECREASE offset
- ↓ (move viewport toward newer) should INCREASE offset

**Acceptance**:
- TUI: focus Memory panel.  Have some messages persisted (run TUI,
  chat once with any agent first).  Press ↑ — older messages appear
  at top; press ↓ — newer messages appear at top.
- Test count unchanged (94).

**Branch**: just commit on `main`.
**Commit**: `fix(tui): Memory panel scroll direction — ↑ for older, ↓ for newer`

---

## Task 2 — `pricing.toml` for cost estimation (2-3 hours)

**Goal**: alongside the existing `· 22k/284` token suffix per agent,
also show `$0.04` cumulative cost.  Per-model rates live in a
checked-in `pricing.toml` users can edit without recompiling.

**Files to touch**:
- New: `examples/pricing.toml` (committed; reference template)
- New: `crates/ch-core/src/pricing.rs` (pricing config + lookup)
- Modify: `crates/ch-agent/src/lib.rs` — `AgentActivity::Idle` gains
  `cumulative_cost_usd: f64`
- Modify: `crates/ch-agent/src/runtime.rs` — read pricing, multiply
  on Idle transition
- Modify: `crates/ch-tui/src/app.rs` — `format_tokens` / `render_activity`
  appends `· $0.04` when cost > 0

### `examples/pricing.toml`

```toml
# Per-million-token rates in USD.  Edit to match your provider's
# pricing.  Missing models cost $0.0 (silently — no panic).
#
# Match is by model substring (case-insensitive), most-specific first.
# Empty file = no costs displayed.

[[rates]]
model = "claude-opus"      # matches claude-opus, claude-opus-4
input_per_mtok = 15.00
output_per_mtok = 75.00

[[rates]]
model = "claude-sonnet"
input_per_mtok = 3.00
output_per_mtok = 15.00

[[rates]]
model = "claude-haiku"
input_per_mtok = 0.25
output_per_mtok = 1.25

[[rates]]
model = "gemini-2.0-pro"
input_per_mtok = 1.25
output_per_mtok = 5.00

[[rates]]
model = "kimi-code"
input_per_mtok = 0.15
output_per_mtok = 2.50

# Fallback for anything else — comment out for strict mode
[[rates]]
model = ""
input_per_mtok = 0.0
output_per_mtok = 0.0
```

### Pricing module shape

```rust
// crates/ch-core/src/pricing.rs

#[derive(Debug, Clone, Deserialize)]
pub struct Rate {
    pub model: String,
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PricingTable {
    #[serde(default)]
    pub rates: Vec<Rate>,
}

impl PricingTable {
    /// Load from `<home>/pricing.toml`, fall back to checked-in
    /// `examples/pricing.toml`, fall back to empty.
    pub fn load() -> Self { ... }

    /// Find first rate whose `model` substring is contained in `model_name`
    /// (case-insensitive).  Longest-match wins; empty `model` is the
    /// catch-all fallback.
    pub fn lookup(&self, model_name: &str) -> Option<&Rate> { ... }

    /// Compute cost in USD given a rate and a (tokens_in, tokens_out) pair.
    pub fn cost(&self, model_name: &str, tokens_in: u64, tokens_out: u64) -> f64 {
        self.lookup(model_name)
            .map(|r| {
                (tokens_in as f64 / 1_000_000.0) * r.input_per_mtok
                    + (tokens_out as f64 / 1_000_000.0) * r.output_per_mtok
            })
            .unwrap_or(0.0)
    }
}
```

### Wiring

1. `AgentRuntime::new` loads pricing once, stores `Arc<PricingTable>`.
2. The per-agent message handler that transitions to
   `AgentActivity::Idle` looks up the agent's `default_model`,
   computes cost from the new chunk's tokens, adds to running total,
   stores in `cumulative_cost_usd`.
3. TUI's `render_activity` appends `· $0.04` when cost > 0
   (skip when 0 — don't pollute display for free / free-tier models).

### Tests

In `ch-core/src/pricing.rs`:
- `lookup` finds claude-sonnet for "claude-sonnet-4"
- `lookup` returns longest-match (claude-opus-4 should NOT match
  "claude-sonnet")
- `cost` calculates correctly for sample (100k in + 5k out @ claude-sonnet)
- Empty table returns 0 for any model
- TOML round-trip

### Acceptance

- `crow doctor claude-wsl-ubuntu` runs, then `crow` TUI shows
  `● claude-wsl-ubuntu  8.5s · 23k/847 · $0.08` after 3-4 prompts.
- Edit `~/.crow-hub/pricing.toml` to set claude-sonnet input to 100.0
  → restart TUI → costs reflect the new rate.

**Commit**: `feat(monitor): per-agent cost estimation via pricing.toml`

---

## Task 3 — Parallel agent loading (2 hours)

**Bug**: `AgentRuntime::load_all` loads agents sequentially.  Each
subprocess agent triggers a host-env probe on first load (one per
unique `HostKey`).  With 11 agents across 2 hosts (WSL Ubuntu + SSH
192.168.50.1), cold-start takes ~10s.  Probes are independent and
should run in parallel.

**File**: `crates/ch-agent/src/runtime.rs`

**Current shape** (around `load_all`):

```rust
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
    Ok(loaded_names)
}
```

**Fix sketch** — pre-probe unique HostKeys in parallel, then `load_plugin`
each agent sequentially (each agent's load is fast once its host env is
cached; the slow part was the cold probe):

```rust
pub async fn load_all(&self) -> Result<Vec<String>> {
    let loader = PluginLoader::new(&self.plugins_dir);
    let plugins = loader.scan()?;

    // Pre-warm host_env_cache: probe each unique HostKey in parallel.
    let unique_keys: std::collections::HashSet<HostKey> = plugins
        .iter()
        .filter_map(|p| p.manifest.subprocess.as_ref().map(derive_host_key))
        .collect();

    let mut probe_handles = Vec::new();
    for key in unique_keys {
        let cache = self.host_env_cache.clone();
        probe_handles.push(tokio::task::spawn_blocking(move || {
            let _ = cache.get_or_probe(&key);
        }));
    }
    for h in probe_handles {
        let _ = h.await;  // probes are best-effort; failures are warned inside
    }

    // Now load each plugin — env probes will be in-memory hits.
    let mut loaded_names = Vec::new();
    for plugin in plugins {
        match self.load_plugin(plugin).await {
            Ok(name) => loaded_names.push(name),
            Err(e) => warn!("Failed to load plugin: {}", e),
        }
    }
    Ok(loaded_names)
}
```

Why pre-probe + sequential load (not parallel `load_plugin`):
- Bus subscription order matters less than not racing each other
- DashMap inserts are safe but spawning N tasks each holding `&self`
  needs Arc gymnastics; pre-probing is cleaner.

### Tests

- `unique_keys` correctly dedupes when 4 agents share `Wsl(Ubuntu)`
  (refactor to a free `unique_host_keys(&[LoadedPlugin]) -> HashSet<HostKey>`
  for testability)
- Existing `test_list_agents` etc. still pass

### Acceptance

- Cold start (`crow refresh-env && crow status`): time the second command.
  Should drop from ~10s to ~3s on a setup with 2 unique hosts.  Measure
  with PowerShell `Measure-Command`.
- 94 tests still pass + ~2 new for `unique_host_keys`.

**Commit**: `perf(runtime): probe host env caches in parallel during load_all`

---

## Task 4 — (Stretch) Theme struct + 2 built-in themes (2 hours)

**Goal**: extract the hardcoded `Color::Cyan` / `Color::Yellow` /
`Color::Green` / etc. used throughout `crates/ch-tui/src/app.rs` into
a single `Theme` struct.  Ship two built-in themes ("default" and
one alternate — pick "monokai" or "high-contrast").  Theme switching
is offline-only for now (env var or config), no `/theme` slash command
yet — that's a separate task.

**Files**:
- New: `crates/ch-tui/src/theme.rs`
- Modify: `crates/ch-tui/src/app.rs` — replace literal `Color::*` calls
  with `theme.<field>`

### Theme struct

```rust
// crates/ch-tui/src/theme.rs

use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub border_focused: Color,
    pub agent_cursor: Color,
    pub agent_multi: Color,
    pub status_idle: Color,
    pub status_thinking: Color,
    pub status_errored: Color,
    pub status_unknown: Color,
    pub suffix: Color,
    pub summary: Color,
    pub footer: Color,
}

pub const DEFAULT_THEME: Theme = Theme {
    name: "default",
    border_focused: Color::LightBlue,
    agent_cursor: Color::Cyan,
    agent_multi: Color::Yellow,
    status_idle: Color::Green,
    status_thinking: Color::Yellow,
    status_errored: Color::Red,
    status_unknown: Color::DarkGray,
    suffix: Color::DarkGray,
    summary: Color::Gray,
    footer: Color::DarkGray,
};

pub const HIGH_CONTRAST_THEME: Theme = Theme {
    name: "high-contrast",
    border_focused: Color::White,
    agent_cursor: Color::White,    // bold + white = clear pick
    agent_multi: Color::LightYellow,
    status_idle: Color::LightGreen,
    status_thinking: Color::LightYellow,
    status_errored: Color::LightRed,
    status_unknown: Color::Gray,
    suffix: Color::Gray,
    summary: Color::White,
    footer: Color::Gray,
};

pub fn from_env() -> Theme {
    match std::env::var("CROW_THEME").ok().as_deref() {
        Some("high-contrast") | Some("hc") => HIGH_CONTRAST_THEME,
        _ => DEFAULT_THEME,
    }
}
```

### Wiring

1. `App` gets `theme: Theme` field.
2. `App::new` calls `theme::from_env()`.
3. `ui()` / `render_activity` take `&App` (which they already do) and
   substitute `app.theme.<field>` for each literal `Color::*`.

### Tests

- `from_env` returns default when CROW_THEME unset
- `from_env` returns high-contrast on `CROW_THEME=hc`
- Both themes have distinct values for `border_focused` (sanity check)

### Acceptance

- `CROW_THEME=hc cargo run --release --bin crow` → TUI in high-contrast.
- Default `cargo run` → unchanged appearance.

**Commit**: `feat(tui): theme struct + high-contrast built-in (CROW_THEME=hc)`

---

## General conventions (same as before)

| Topic | Convention |
|---|---|
| Commits | Imperative + scoped: `fix(tui): …`, `feat(monitor): …` |
| Tests | Every behavior change adds ≥1 test |
| CI gate | `cargo test --workspace --lib` must pass before push |
| Style | `cargo fmt --all` before commit |
| Branches | Direct to `main` for small commits today; create a feat branch only if a task explodes in scope |

## What to AVOID (CRITICAL)

- ❌ **No spawned-task worktrees.**  Do NOT use the `Agent` tool to
  spawn sub-tasks if you're Claude Code — it creates git worktrees
  under `.claude/worktrees/` that flip `[extensions] worktreeConfig`
  in `.git/config`, which silently breaks Antigravity's chat panel
  for this workspace.  Work in-session.  See journal
  `2026-05-13_*` Section 1 for cleanup procedure if it happens.
- ❌ No force-pushes to `main`.
- ❌ No new top-level dependencies without checking workspace `Cargo.toml`.
- ❌ Don't touch `~/.crow-hub/` (user data dir) or
  `examples/agents/` template files.

## If you get stuck

- `crow doctor <agent>` for fast feedback on driver issues.
- `RUST_LOG=trace cargo run …` shows every bus message.
- The full history is in `docs/journals/` — read backwards from
  the most recent date.

## Reporting back

End-of-day journal at `docs/journals/2026-05-18_<short-topic>.md`
following the chain.  Cover:
- What shipped (commits / scope)
- What got blocked + why
- One surprise / learning
- Test count delta
- Cold-start time delta (if Task 3 shipped) — `Measure-Command { crow status }`

If you only shipped Tasks 1 and 2, that's fine — flag Tasks 3 and 4
as carry-over and leave them for the next agent.  Better to merge
incremental progress than block on a big bang.
