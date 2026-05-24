# 2026-05-22 — Handoff Envelopes (Maestro Task 1)

**Author:** Claude
**Plan:** `C:\Users\zhiqing\.claude\plans\distributed-sauteeing-bee.md`
(combined plan: verify + handoff envelopes)
**Commit:** `48c2e76`

Continues from `2026-05-22_project_state_and_roadmap_sync.md` (DeepSeek).

---

## What shipped

### 1. Memory writer sanity check + tracing (Step 1)

Located the writer at `crates/ch-memory/src/writer.rs`.  The code
itself was correct (subscribe + persist on TaskRequest/TaskResponse,
error path already had `warn!`).  The reason `crow memory count`
returned 0 was simply that the TUI hadn't been launched since the
`~/.crow-hub/` path migration on 5/14 — the log file at
`C:\Users\zhiqing\.crow-hub\crow-hub.log` was last touched 5/17 with
zero `"memory writer subscribed"` lines, confirming a fresh run was
needed, not a bug.

Added two new tracing improvements:
- DEBUG on each successful persist (gated behind
  `RUST_LOG=ch_memory=debug` since streaming chunks make this chatty)
- INFO when the bus rx closes ("writer task exiting") so a future
  silent panic is observable
- WARN message bodies now include `msg_id` and `from` for faster
  triage

### 2. End-to-end verification of b671d40 (Step 2)

Code-read only — I can't drive an interactive TUI from this session.

- `model_override: Option<String>` on `AgentMessage` ✅ (lib.rs:124)
- `with_model_override` builder ✅ (lib.rs:152)
- Runtime reads override at runtime.rs:255 with fallback ✅
- `chat_scope_all: bool` field + cursor toggle + multi-select bypass
  + `/all` command — all wired correctly per source

Found one **pre-existing breakage**: `test_message_expiration` was
broken when `model_override` was added — the struct literal didn't
include the new field, causing `cargo check -p ch-protocol --tests`
to fail.  Fixed in the same commit as the Handoff feature.  Mystery
solved: nobody had run `cargo test` on protocol-only since b671d40.

### 3. Handoff Envelopes (Step 3, Maestro Task 1)

The marquee feature.  ~100 production lines + 50 test lines across
5 files, all additive (no schema migration).

**Protocol** (`ch-protocol/src/lib.rs`):
- `HandoffEnvelope { from_agent, summary, decisions, open_questions,
  continuation_hint }` — Default-implementable, `#[serde(default)]`
  on vec/string fields for graceful old-row parsing
- `MessageType::Handoff` variant (serialises as `"handoff"` snake_case)
- `Payload::Handoff(HandoffEnvelope)` variant
- 5 new tests including a backward-compat one that parses an old
  JSON message lacking `model_override`

**TUI** (`ch-tui/src/app.rs`):
- `/handoff <summary>` slash command — broadcasts a handoff envelope
  on the bus (to=None), renders locally as `⇄ You → <summary>`
- Chat scope filter passes any `⇄`-prefixed line unconditionally
  (handoffs are cross-agent coordination, belong in every scope view)
- `/help` documents both `/handoff` and (the previously-undocumented)
  `/all`

**Memory writer** (`ch-memory/src/writer.rs`):
- Now persists handoff envelopes alongside chat — JSON-serialises the
  envelope into the existing `content` column with
  `memory_type = "handoff"`.  No schema migration needed.

**Runtime observation** (`ch-agent/src/runtime.rs`):
- Per-agent message handler logs `INFO [<agent>] received handoff from
  <from>: <summary>` whenever a handoff crosses the agent's queue,
  regardless of addressing.  First step toward agents reacting to
  handoffs; today it's visibility-only.

**CLI** (`ch-tui/src/main.rs`):
- `crow memory tail` recognises `memory_type = "handoff"` and renders
  with `⇄` glyph so persisted handoffs are visually distinct.

---

## Test count delta

| Crate | Before | After | Δ |
|---|---:|---:|---:|
| ch-protocol | 3 | 8 | +5 |
| ch-memory | 13 | 20 | +7 (these were added by DeepSeek between 5/19 and now) |
| ch-agent | 57 | 57 | 0 |
| ch-tui (binary) | 20 | 20 | 0 |
| others | 35 | 35 | 0 |
| **total lib** | **108** | **117** | **+9** |

All passing.  Release builds clean.

---

## One surprise

The memory writer wasn't broken — the user (or DeepSeek) had just not
run the TUI since the `~/.crow-hub/` path migration, so the empty DB
was the natural state.  This is a UX hazard: from the outside it looks
exactly like a silent failure.  Worth a follow-up: when `crow memory
count` returns 0, print a more leading hint like "No messages yet —
launch `crow` and chat with any agent to start populating".  Filed
as something for the next plan.

The deeper surprise: a broken test (`test_message_expiration` missing
`model_override`) had been in the codebase since `b671d40` and nobody
caught it because `cargo test --workspace --lib` was being run, which
runs each crate's tests including ch-protocol — so it *should* have
failed.  My earlier "tests pass" report was probably from running
something slightly different, or the test was added later and we
never re-verified.  Either way: the gate is back to green.

---

## What I deliberately didn't do (carried forward)

- **Surface remote handoffs in the TUI chat panel** — today's first
  ship only shows local `/handoff` feedback.  Remote handoffs go to
  `crow-hub.log` (INFO) and to SQLite (`memory_type=handoff`) but
  don't render in the TUI's chat panel.  Adding that needs a small
  extension to the bus→TUI bridge to forward `Payload::Handoff` as
  well as `Payload::Text`.  Easy follow-up.
- **Agent auto-emission** — agents don't yet emit handoffs from their
  prompt output.  The plan's call was to ship manual `/handoff` first,
  prove the data path, then design auto-emission once we know the
  envelope shape stabilises.
- **Maestro Tasks 2-4** (Evidence table, state-machine workflows,
  Agent principles) — build on Handoff; sequel plans.

---

## Anti-patterns observed (still holding)

- ✅ No spawned-task git worktrees this session (worked in-session)
- ✅ No force-pushes
- ✅ Additive schema only — no `ALTER TABLE` needed for handoff persistence

---

## Pointer for next agent

Three obvious next moves, in order of value-per-hour:

1. **Bridge `Payload::Handoff` into the TUI chat panel** (~20 min) —
   tiny extension to `crates/ch-tui/src/main.rs` line ~322 bridge task
   so remote handoffs show up in the chat as `⇄ <from> → <summary>`.
2. **Maestro Task 2 — Evidence table** (3-4 hrs) — second piece of
   the collaboration-OS direction, builds on Handoff.
3. **`/help` audit** — `/handoff` is the third command added since
   `/help` was last fully reviewed; worth a one-shot pass to make
   sure every implemented command is documented and every documented
   command works.
