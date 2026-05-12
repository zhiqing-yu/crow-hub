# Four-Day Plan for Crow Hub (2026-05-12 → 2026-05-16)

> **Audience**: the next coding agent (Gemini CLI / Kimi / a fresh Claude Code
> session) continuing work on crow-hub while the original user is away.
> This document is self-contained — you do NOT need any prior conversation
> context to execute it.

---

## What is crow-hub?

A multi-agent orchestration hub built in Rust.  It runs CLI-based AI agents
(Claude, Gemini, Kimi, OpenClaw, etc.) through native / WSL / SSH subprocess
drivers, wires them through a message bus with shared channels and
correlation IDs, and presents a TUI for the user to chat with selected
agents.  Repo: https://github.com/zhiqing-yu/crow-hub.

## Where we are right now

- All infrastructure works: message bus, streaming subprocess driver,
  host-env probe + cache (per-user PATH discovery), TUI with live per-agent
  status indicators (●idle/◐thinking/✗errored)
- 10 of 11 configured agents respond end-to-end via `crow doctor`
- PR #2 (`chore/strip-user-manifests-for-fresh-clones`) is open — strips
  user-specific manifests from git so fresh clones get a clean baseline
- 85 unit tests passing

For the complete history, read
`docs/journals/2026-05-12_runtime_and_setup_milestones.md`.

---

## Pre-flight (~5 minutes, do BEFORE Day 1)

```bash
cd <repo>
git fetch origin
git checkout main
git pull origin main
git status                              # must be clean
cargo test --workspace --lib            # must show 85 passing
gh pr list --repo zhiqing-yu/crow-hub   # see open PRs
```

If `cargo test` fails or the repo isn't clean, **stop and report** —
don't try to fix unrelated breakage on your own.

Read these two files for context:
- `docs/journals/2026-05-12_runtime_and_setup_milestones.md` — what's been
  built and why
- `ROADMAP.md` — the long-term phased plan

---

## Day 1 (Monday) — Land PR #2 + validate fresh-clone flow

**Goal**: ensure any new user can clone the repo, run `crow setup`, and
reach a working TUI.  This is the "make it shareable" milestone.

### Tasks

**1.1** — Check PR #2 status:

```bash
gh pr view 2 --repo zhiqing-yu/crow-hub
```

If still open: review the description; if CI is green and the diff looks
reasonable (removes 11 manifests, adds 5 templates + README updates),
**squash-merge it** with the title `chore: strip user-specific manifests
from git; ship template-only repo`.

If already merged: skip to 1.2.

**1.2** — Fresh-clone smoke test (in a scratch directory):

```bash
cd /tmp && rm -rf crow-hub-test
git clone https://github.com/zhiqing-yu/crow-hub.git crow-hub-test
cd crow-hub-test
cargo build --release --bin crow
./target/release/crow status
```

Expect: `Loaded 0 agent plugin(s)`, `Status: 🟢 Installed`, no crash.

**1.3** — Setup wizard test:

```bash
./target/release/crow setup
ls plugins/agents/        # should show one dir per agent it found
git status                # MUST show no new tracked files (gitignored)
```

**1.4** — Launch the TUI and confirm discovered agents appear:

```bash
./target/release/crow
# Tab to Agents panel, navigate with arrows, send a test prompt to one
# Verify status indicator goes ◐yellow then ●green
```

**1.5** — If anything is broken, fix it.  Likely fix areas:
- README's command sequence has a typo or wrong path
- `crow setup` writes to wrong location
- The probe times out and reports cryptically
- A template file in `examples/agents/` has invalid TOML

Open a small follow-up PR (`chore: post-merge fresh-clone polish`) for
any tweaks.

### Acceptance

- A fresh clone reaches a working TUI in 4 commands (clone → build → setup
  → run)
- No tracked files appear in `plugins/agents/` after setup
- 85 tests still pass

### If you finish Day 1 early

Look at `gen_pitch.py` / `crow-hub-pitch.pptx` in `docs/` — there's an old
pitch deck.  Not urgent; ignore unless you want to update screenshots to
reflect the current TUI.

---

## Day 2 (Tuesday) — Multi-agent broadcast in the TUI

**Goal**: select multiple agents in the sidebar (Space toggles), press
Enter, send the same prompt to all selected agents in parallel.  All
responses appear in the chat with their agent prefix.

**Why this is high-value + tractable**: the bus already supports
multi-agent fan-out (every agent subscribes to `general`), per-agent
runtime handlers already work independently, and the TUI's `on_tick`
already merges responses by agent prefix.  The only missing piece is
the multi-select UX.

### Tasks

**2.1** — Extend the `App` struct in `crates/ch-tui/src/app.rs` (around
line 33):

```rust
use std::collections::HashSet;

pub struct App {
    // ...existing fields...
    pub selected_agent: usize,             // primary cursor (already exists)
    pub multi_selected: HashSet<usize>,    // NEW
}
```

Initialize as `HashSet::new()` in `App::new`.

**2.2** — Add keyboard handling in `run_loop` in `app.rs` (around line
195, where arrow keys are handled for the Agents panel):

| Key | When | Action |
|---|---|---|
| `Space` | Agents panel focused | Toggle `selected_agent` in `multi_selected` |
| `Backspace` | Agents panel focused (no other meaning here) | Clear `multi_selected` |
| `Tab` / arrows | Agents panel | Existing behavior, unchanged |

**2.3** — Update the Enter handler (around line 231) to broadcast:

```rust
KeyCode::Enter => {
    if !app.input.is_empty() {
        let prompt = app.input.clone();
        app.messages.push(format!("You: {}", prompt));

        let targets: Vec<String> = if !app.multi_selected.is_empty() {
            // Broadcast: collect all multi-selected agent names
            app.multi_selected.iter()
                .filter_map(|i| app.agents.get(*i).map(|a| a.name.clone()))
                .collect()
        } else {
            // Single agent (existing behavior)
            vec![app.agents[app.selected_agent].name.clone()]
        };

        for agent_name in targets {
            // Existing single-agent send logic, parameterized
            // by agent_name (extract into a helper if it gets messy)
            // ...
        }

        app.input.clear();
        // Don't clear multi_selected — user may want to send another
        // prompt to the same set
    }
}
```

**2.4** — Visual cue in the agent list (`ui` function, around line 314):

For each agent at index `i`, if `app.multi_selected.contains(&i)`:
- Change the cursor prefix from `"  "` to `"[✓] "`
- Use `Color::Yellow` for the name (in addition to existing styling)

Keep the `> ` cursor for the primary `selected_agent` overlay so users
can navigate the agent list independently of their multi-selection.

**2.5** — Tests in `crates/ch-tui/src/app.rs::tests`:

```rust
#[test]
fn multi_select_starts_empty() { ... }

#[test]
fn multi_select_toggles_via_space() {
    // Simulate setting selected_agent then toggling — verify HashSet state
}

#[test]
fn multi_select_clears_via_backspace() { ... }
```

(The TUI's run_loop is hard to unit test directly; test the state
mutations as pure functions if possible, or extract them into helper
methods on `App`.)

**2.6** — Manual validation:

```bash
cargo build --release --bin crow
./target/release/crow
```

- Tab to Agents panel
- Navigate to 3 different agents, hit Space on each
- Verify the [✓] prefix appears in yellow
- Type a prompt, Enter
- All 3 agents should go ◐yellow simultaneously, then ●green as they respond
- Chat shows their responses prefixed `<agent-name>:`

### Acceptance

- Space toggles individual agents in/out of the multi-selection
- Backspace clears the multi-selection
- Enter with N selected agents fans out to all N in parallel
- Status glyphs animate independently per agent
- Default (no multi-selection) behavior is unchanged
- Tests pass; no regressions

**Branch**: `feat/tui-multi-agent-broadcast`
**PR title**: `feat(tui): multi-agent broadcast — Space to toggle, Enter to send to all`

---

## Day 3 (Wednesday) — Memory layer: SQLite persistence

**Goal**: every message that flows through the bus gets persisted to
SQLite so we have a foundation for semantic recall later.  **No embeddings
in this PR** — that's a follow-up.  Scope: schema + writer + minimal
query API.

**Why scope is intentionally narrow**: `ch-memory` already has scaffolding
(`backends/sqlite.rs`, `embedder/`).  A "data plane only" PR avoids
ML-dependency churn and ships value: even without embeddings, recent-N
recall by channel or correlation_id is useful.

### Tasks

**3.1** — Read the existing skeleton:

```
crates/ch-memory/src/lib.rs
crates/ch-memory/src/backends/sqlite.rs
crates/ch-memory/src/backends/mod.rs
```

Understand the trait shape and what's already there.  Don't rewrite —
build on it.

**3.2** — Confirm or add the SQLite schema (in `sqlite.rs::init`):

```sql
CREATE TABLE IF NOT EXISTS messages (
  message_id      TEXT PRIMARY KEY,
  correlation_id  TEXT,
  from_agent      TEXT NOT NULL,
  to_agent        TEXT,
  channel         TEXT,
  message_type    TEXT NOT NULL,
  content         TEXT NOT NULL,
  embedding       BLOB,                 -- reserved, NULL for now
  created_at      INTEGER NOT NULL      -- unix seconds
);
CREATE INDEX IF NOT EXISTS idx_messages_correlation ON messages(correlation_id);
CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);
CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel);
```

DB location: `~/.crow-hub/messages.db`.  Auto-create the parent dir.
Allow override via env var `CROW_HUB_MEMORY_PATH` for tests.

**3.3** — Wire a writer to the bus.  Two patterns possible:

**A** — Subscribe to all channels with a dedicated `memory-writer` agent
ID, write each received message.  Cleaner separation.

**B** — Hook directly in `MessageBus::send_to_channel` after a successful
fan-out.

Pick **A** — it composes better with future filtering (e.g. don't store
status messages, only TaskRequest/TaskResponse).

Add in `crates/ch-core/src/bus.rs` or as a separate module in `ch-memory`:

```rust
pub fn spawn_memory_writer(
    bus: Arc<MessageBus>,
    store: Arc<dyn MemoryStore>,
) -> tokio::task::JoinHandle<()> {
    let writer_id = AgentId::new();
    tokio::spawn(async move {
        let mut rx = bus.subscribe(writer_id).await;
        bus.join_channel("general", writer_id, ChannelVisibility::Read).await.ok();
        while let Some(msg) = rx.recv().await {
            // Filter: only persist Text-payload TaskRequest / TaskResponse
            if let Payload::Text(ref text) = msg.payload {
                if matches!(msg.message_type, MessageType::TaskRequest | MessageType::TaskResponse) {
                    let _ = store.write(Memory::from_message(&msg, text)).await;
                }
            }
        }
    })
}
```

Spawn this from `main.rs` after the runtime is set up.

**3.4** — Query API stubs (no embeddings):

```rust
impl MemoryStore for SqliteStore {
    async fn recent(&self, channel: &str, limit: usize) -> Result<Vec<Memory>>;
    async fn by_correlation(&self, id: Uuid) -> Result<Vec<Memory>>;
}
```

These are just SQL queries ordered by `created_at DESC`.

**3.5** — Tests in `crates/ch-memory/src/backends/sqlite.rs`:

- Write + read round-trip (single message)
- `by_correlation` returns chunks in timestamp order
- `recent(channel, N)` returns last N from that channel
- Empty channel returns empty Vec, no error

Use `tempfile::tempdir()` for the test DB path.

**3.6** — Don't surface in TUI yet.  Just persist.  Surfacing recall is
a follow-up that depends on having an embedder.

### Acceptance

- Run `crow` for 1 minute, send a few prompts.  Then:
  ```bash
  sqlite3 ~/.crow-hub/messages.db 'SELECT COUNT(*), MIN(created_at), MAX(created_at) FROM messages'
  ```
  Returns a non-zero count and reasonable timestamps.
- New tests pass (target: +6 tests, total 91+)
- No regressions; bus still works exactly as before for end users

**Branch**: `feat/memory-sqlite-persist`
**PR title**: `feat(memory): persist every bus message to SQLite`

### If you finish Day 3 early

- Add a `crow memory tail [channel]` CLI subcommand that prints the last
  N entries.  Read-only, no embedder needed.

---

## Day 4 (Thursday) — Token + cost surfacing in the TUI

**Goal**: extend the existing `AgentActivity` to track cumulative token
counts (input + output) per agent, surface them in the TUI agent list.
This delivers the first piece of Phase 4 (monitoring) without the full
metrics system.

**Why it works**: most CLI agents already emit token counts in their JSON
output (we saw OpenClaw's `meta.usage.input: 22279` in earlier doctor
runs).  We extract them from the existing response parser.

### Tasks

**4.1** — Extend `AgentActivity` in `crates/ch-agent/src/lib.rs`:

```rust
pub enum AgentActivity {
    Unknown,
    Idle {
        last_latency_ms: Option<u64>,
        cumulative_tokens_in:  u64,     // NEW
        cumulative_tokens_out: u64,     // NEW
    },
    Thinking { since: DateTime<Utc> },
    Errored { last_error: String },
}
```

Update the test in `ch-agent/src/lib.rs::tests::test_agent_state_serialization`
to cover the new variant shape if needed.

**4.2** — Parse tokens in the subprocess driver.

In `crates/ch-agent/src/drivers/subprocess.rs::process_output` (around
line 134), after JSON parsing, try to extract token usage from common
field paths:

- `meta.usage.input` + `meta.usage.output` (OpenClaw shape)
- `usage.input_tokens` + `usage.output_tokens` (Anthropic shape)
- `usage.prompt_tokens` + `usage.completion_tokens` (OpenAI shape)

Return them on `ChatResponse.usage: TokenUsage` (the field already exists
— check `crates/ch-model/src/lib.rs::TokenUsage`).

**4.3** — Accumulate in runtime.

In `crates/ch-agent/src/runtime.rs`, the per-agent message handler that
sets `AgentActivity::Idle`: instead of `cumulative_tokens_*: 0`, read
the previous value (from the existing `activities` DashMap) and add the
latest response's tokens.

This means `AgentActivity::Idle` is cumulative over the agent's session,
resetting only on restart.

**4.4** — Display in TUI (`crates/ch-tui/src/app.rs::render_activity`):

After the latency suffix, append `· <K>k/<O>k` where K/O are tokens in
thousands.  Skip if both are zero (first request, or agent doesn't emit
counts):

```
● openclaw-wsl-ubuntu  18.6s · 22k/0.3k
● claude-wsl-ubuntu     2.1s
```

Use `Color::DarkGray` like the latency suffix.  Don't make the row wider
than it already is — truncate gracefully if the agent name is long.

**4.5** — Tests in `subprocess.rs::tests` and `app.rs::tests`:

- `process_output` extracts tokens from a sample OpenClaw JSON
- `process_output` returns zero tokens when no usage field present (no
  panic)
- `format_tokens(22279, 284)` returns `"22k/0.3k"`
- `render_activity` includes the token suffix when present, omits when zero

### Acceptance

- After 3-4 prompts to OpenClaw, the agent list shows something like:
  `● openclaw-wsl-ubuntu  18.6s · 67k/0.9k`
- Agents that don't emit token counts (Claude raw stdout, Gemini, etc.)
  show only the latency, no token suffix
- All existing tests pass; ~5 new ones added

**Branch**: `feat/agent-token-counts`
**PR title**: `feat(monitor): cumulative token counts per agent in the TUI`

### If you finish Day 4 early

- Add per-message cost estimation if pricing tables are easy to add as a
  TOML config (`pricing.toml` with $/Mtoken per model).  Skip if it gets
  complicated.

---

## General conventions

| Topic | Convention |
|---|---|
| **Commits** | Imperative, scoped: `feat(tui): …`, `fix(runtime): …`, `chore: …`, `docs: …`.  Wrap message body at 72 chars. |
| **Branches** | `feat/<short-name>`, `fix/<short-name>`, `chore/<short-name>`, `docs/<short-name>` |
| **PRs** | One feature per PR.  Body has **Summary** + **Why** + **Test plan** sections.  AI co-authorship attribution is welcome (the user is fine with `🤖 Generated with [Claude Code]` or equivalent for your agent). |
| **Tests** | Every behavioral change adds at least 1 test.  `cargo test --workspace --lib` must pass before pushing. |
| **CI** | Must stay green on every push.  If you break CI, fix it before adding more work. |
| **Style** | Run `cargo fmt --all` before commit.  Don't bother with `cargo clippy --fix` — it's flaky.  `cargo check` and `cargo test` are the gates. |

---

## What to AVOID

- ❌ Force-pushing to `main` — ever.  Branch + PR.
- ❌ Touching `target/`, `.claude/`, `~/.crow-hub/` (user data dir),
  `.claude/worktrees/` (ephemeral scratch dirs).
- ❌ Adding new top-level dependencies without checking workspace
  `Cargo.toml`.  Use what's there (tokio, serde, dashmap, anyhow, tracing,
  futures).
- ❌ Big architectural pivots in one PR.  Day 3 (memory) is risky enough
  — keep its scope tight.
- ❌ Committing user-specific data: agent manifests, API keys, host
  addresses, `~/.crow-hub/*` contents.  The `.gitignore` covers most;
  stay paranoid.
- ❌ Editing files under `.claude/worktrees/` or `examples/agents/`
  templates without good reason.
- ❌ Renaming public crates, traits, or driver types — too many call sites.

---

## If you get stuck

1. **Driver issues**: `cargo run --release --bin crow -- doctor <agent>` —
   prints the full invocation, host env, and raw stdout/stderr.  Fastest
   feedback loop for anything subprocess-related.
2. **Bus issues**: trace logs via `RUST_LOG=trace cargo run …` show every
   message flow.
3. **Architectural why**: read
   `docs/journals/2026-05-12_runtime_and_setup_milestones.md`.
4. **Manifest format**: see `examples/agents/_template-*.toml` and
   `crates/ch-agent/src/manifest.rs`.
5. **A day's task is too big**: ship what works as a PR (use a `wip:`
   prefix in the title) and carry the rest to the next day or as a
   follow-up issue.  Better to merge incremental progress than block
   on a big-bang change.

---

## Reporting back

At the end of each day, write a short note in
`docs/journals/YYYY-MM-DD_<topic>.md` following the existing convention
(see other files in that dir).  Cover:

- What shipped (commits / PRs)
- What got blocked + why
- One thing that surprised you
- Tests passing count

The user reads these to catch up on what happened while away.

---

## Stretch — only if everything else is done

- **Parallel agent loading**: today the runtime loads agents sequentially,
  each waiting for its host env probe.  ~10s with 11 agents across 2
  hosts.  Probing all unique hosts in parallel first, then loading agents,
  could cut this to ~3s.
- **Embeddings for memory**: hook `crates/ch-memory/src/embedder/local.rs`
  into the writer, populate the `embedding BLOB` column on write,
  implement `MemoryStore::search(query, top_k)` with cosine similarity.
  Use `ort` (ONNX Runtime) + a sentence-transformer model.
- **GUI work**: `crates/ch-gui/src/main.rs` is a stub.  Tauri integration
  per Phase 6 of `ROADMAP.md`.  Probably 1-2 weeks of work, not 4 days.

---

## Reading list before starting

In order of importance:

1. `docs/journals/2026-05-12_runtime_and_setup_milestones.md` — what's been built
2. `ROADMAP.md` — the long-term phased plan
3. `examples/agents/README.md` — the manifest format users see
4. `crates/ch-agent/src/host_env.rs` — the cache machinery you'll
   reference but probably not modify
5. `crates/ch-tui/src/app.rs` — the TUI structure (will modify on Day 2 + 4)
6. `crates/ch-agent/src/runtime.rs` — the agent runtime (will modify on
   Day 4)
7. `crates/ch-memory/src/lib.rs` + `backends/sqlite.rs` — the memory
   scaffolding (will modify on Day 3)

---

Good luck.  Ship steadily, prefer working software over perfect.
