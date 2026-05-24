# 2026-05-24 — Evidence Table (Maestro Task 2)

**Author:** Claude
**Plan:** `C:\Users\zhiqing\.claude\plans\distributed-sauteeing-bee.md`
(focused plan: close the Handoff loop, then ship Evidence)
**Commits:** `1df8ff0 .. bf4a71a` (8 commits)

Continues from `2026-05-22_handoff_envelopes.md`.  DeepSeek did not
pick up yesterday's day plan (`docs/plans/2026-05-23_deepseek_day.md`
at `bcf4866`), so I executed it myself today — scoped to Tasks 1+2+3
per user choice (skipped the stretch Task 4 cross-link).

---

## What shipped

### Task 1 — Bridge `Payload::Handoff` bus → TUI chat (`01405b3`)

Closed the loop on yesterday's Handoff work.  Before: a remote agent
emitting a Handoff envelope landed in `crow-hub.log` (runtime
observer) and SQLite (memory writer), but **never showed in the chat
panel** — the bus→TUI bridge at `ch-tui/src/main.rs:322-331` only
forwarded `Payload::Text`.

Two-line fix in the bridge plus a `__handoff__` synthetic-sender
special-case in `on_tick`:

```rust
match &msg.payload {
    Payload::Text(text)   => tx.send((from_name, text)),
    Payload::Handoff(env) => tx.send(("__handoff__", format!("⇄ {} → {}", env.from_agent, env.summary))),
    _ => {}
}
```

```rust
// on_tick
if agent == "__handoff__" { messages.push(response); continue; }
```

Refactored `on_tick`'s streaming-merge logic into a free function
`append_chat_message(&mut Vec<String>, &str, &str)` so the test
doesn't need to construct a real `App` (which would require a live
`AgentRuntime` + `MessageBus`).  Three unit tests:

- `append_chat_message_handoff_pushes_unprefixed`
- `append_chat_message_text_streams_into_last_when_same_agent`
- `append_chat_message_text_starts_new_line_for_different_agent`

### Task 2 — `SUPPORTED_COMMANDS` const + `/help` regression test (`938d718`)

Pure hygiene.  `/help` text and the slash-command match arms were in
sync today (5 commands: `/clear`, `/model`, `/all`, `/handoff`,
`/help`) but nothing enforced it.  Added:

```rust
pub const SUPPORTED_COMMANDS: &[&str] =
    &["/clear", "/model", "/all", "/handoff", "/help"];
```

… and extracted `/help`'s body into `help_lines() -> Vec<String>`.
Regression test asserts each `SUPPORTED_COMMANDS` entry appears in
`help_lines()` output.  Cheap insurance against the drift that bit us
3× in the past month.

That regression test paid off **within the same session** — when I
added `/evidence` to `SUPPORTED_COMMANDS` in Task 3e but forgot to
update `help_lines()`, the test failed immediately.

### Task 3 — Maestro Task 2: Evidence table (5 sub-commits)

The architectural headline.  Auditable agent claims with verification
status, stored in a new `evidence` SQLite table alongside the existing
`messages` table.

**3a — Storage layer** (`cfad27e`):
- New `evidence` table with indexes on `task_id`, `correlation_id`,
  `status`.  `CREATE TABLE IF NOT EXISTS` only — old DBs upgrade in
  place when the binary first opens them, no migration needed.
- Types in `ch-memory/src/lib.rs`: `EvidenceStatus` (Pending|Verified
  |Failed, snake_case serde, forward-compat `from_str` defaults to
  Pending), `EvidenceRow`, `EvidenceStore` trait.
- `SqliteMemoryStore` implements `EvidenceStore` alongside
  `MemoryStore`.  `fail()` merges `failure_reason` into the row's
  `metadata` JSON via read-modify-write (fine for the expected volume
  of one transition per claim).
- 6 tests including a write/verify/fail lifecycle, by_task ordering,
  pending(limit) with mixed-status seed, and NotFound on unknown id.

**3b — Bus protocol** (`db98628`):
- `MessageType::Evidence` (serialises `"evidence"`) and
  `MessageType::EvidenceVerify` (`"evidence_verify"`).
- `EvidenceClaim { task_id, claim, witness }` and
  `EvidenceVerifyMsg { evidence_id, outcome: bool, note }`.
- `Payload::Evidence(EvidenceClaim)` and
  `Payload::EvidenceVerify(EvidenceVerifyMsg)`.
- All additive, mirrors the Handoff pattern from `48c2e76`.
- 4 round-trip tests including a backward-compat parse without
  `witness`.
- `cargo check --workspace` confirmed nothing exhaustively matches on
  `Payload`, so no upstream breakage from new variants.

**3c — Memory writer** (`45b0168`):
- `spawn_memory_writer` signature gained a third arg:
  `evidence_store: Arc<dyn EvidenceStore>`.  Both ch-tui call sites
  pass the same `SqliteMemoryStore` Arc cloned twice — Rust
  auto-coerces the concrete type to both trait objects.
- Evidence payloads dispatched BEFORE the existing tuple match for
  chat/handoff, with `continue` skipping the message-table path.
  Clean separation between the two write paths.
- DEBUG on success, WARN on failure with id + sender for diagnostics.

**3d — CLI subcommand** (`fa6206d`):
- `crow memory evidence` — mirror of `crow memory tail`.
- Filters: `--task <id>`, `--status pending|verified|failed|all`,
  `--count <n>` (default 50).
- Smart defaults: no `--task` → pending (verifier worklist); with
  `--task` → all (full lifecycle for that task).
- Glyph by status: `?` pending, `✓` verified, `✗` failed.
- Trait extension: added `EvidenceStore::by_status(status, limit)`
  for the verified/failed lookups.  `pending(limit)` now delegates to
  `by_status(Pending, limit)`.  Kept `pending` as the named shortcut
  because it's the common case and reads better at call sites.
- 1 test for `by_status` correctness.

**3e — TUI slash command** (`bf4a71a`):
- `/evidence claim <text>` — builds `EvidenceClaim` with `task_id`
  defaulted to the TUI session's `user_agent_id.to_string()`.
- Local feedback: `📋 You → claimed: <text>`.  `📋` chosen to avoid
  collision with `⇄` (handoff).  Chat scope filter extended to pass
  `📋`-prefixed lines too.
- Sub-command structure (`/evidence claim …`) leaves room for future
  verbs (`verify`, `fail`) without bus collisions.
- `SUPPORTED_COMMANDS` + `help_lines()` updated.

---

## Test count

| Crate          | Yesterday | Today | Δ |
|----------------|----------:|------:|--:|
| ch-protocol    | 8         | 12    | +4 (3b) |
| ch-memory      | 6         | 13    | +7 (3a × 6, 3d × 1) |
| ch-tui (bin)   | 20        | 25    | +5 (Task 1 × 3, Task 2 × 2) |
| All others     | 103       | 103   | unchanged |
| **Total**      | **137**   | **153** | **+16** |

All green.  `cargo check --workspace` clean (only the 3 pre-existing
`dead_code` warnings on `Theme.name`, `App.tx`, `SqliteMemoryStore.config`).

---

## Design decisions worth noting

### `Arc<dyn EvidenceStore>` as a separate parameter

In 3c I had three options for plumbing the writer:
1. Change writer to take `Arc<SqliteMemoryStore>` directly — loses the
   trait abstraction.
2. Keep `Arc<dyn MemoryStore>` and downcast — fragile.
3. Take separate `Arc<dyn MemoryStore>` + `Arc<dyn EvidenceStore>`.

Picked (3).  Caller passes the same Arc cloned twice; Rust handles
the coercion to two different trait objects.  Cleanest separation and
preserves the abstraction for both stores.  Backward-compatible at the
call site (both ch-tui paths needed identical 3-line updates).

### Default `task_id` in `/evidence claim`

For the first cut, `task_id = user_agent_id.to_string()` (the TUI
session's UUID).  All claims from one TUI session group under one
synthetic task_id.  Users can filter with `crow memory evidence --task
<that-uuid>` to recover the set.

The "right" answer is `task_id` from a project tracker (Jira/GitHub
issue), but that needs UX that doesn't exist yet (a `/task` slash
command, or auto-detection from PWD).  Punting until needed.

### `failure_reason` in `metadata` JSON, not a column

`fail(id, by, reason)` does a read-modify-write on the row's `metadata`
JSON to insert `failure_reason`.  Alternative: add a `reason` column.
The JSON approach keeps the schema minimal and avoids a column that's
NULL for the verified-or-pending majority.  Acceptable since the
verify/fail rate is low (one transition per claim).

### Sub-command syntax for `/evidence`

`/evidence claim <text>` (not `/evidence <text>`) to leave room for
`/evidence verify <id>` and `/evidence fail <id> <reason>` without
bus collisions.  Mirrors `crow memory <subcommand>`.

---

## Surprise of the day

The `help_lines_documents_every_supported_command` regression test
**caught a real bug within the same session**.  When I added
`/evidence` to `SUPPORTED_COMMANDS` (Task 3e) but forgot to add the
help line, `cargo test -p ch-tui --bin crow` failed.  Took ~30
seconds to fix.  This is exactly the drift Task 2 was supposed to
prevent — and the proof-of-utility came faster than expected.

---

## End-to-end smoke

```
$ CROW_HUB_MEMORY_PATH=:memory: crow memory evidence
━━━ crow memory evidence — task: (any), status: pending, 0 rows ━━━
(no evidence rows match)
Tip: in the TUI, `/evidence claim <text>` emits an evidence
row; agents can also emit them programmatically.
```

```
$ crow memory evidence --status garbage
Error: invalid --status 'garbage'; expected pending|verified|failed|all
```

Both behave as designed.  Live TUI smoke (`/evidence claim` →
`crow memory evidence`) requires running the interactive TUI; can
verify next session or when zhiqing exercises the build.

---

## Carry-over for the next agent

### Definitely-deferred

- **Task 4 stretch — cross-link Handoff `decisions` → Evidence rows.**
  Skipped per user scope.  When implemented: in the writer, on
  `Payload::Handoff(env)`, also emit N `EvidenceRow`s for each item
  in `env.decisions` with status=pending.  Closes the loop showing
  Handoff and Evidence compose, not just coexist.

- **`/evidence verify <id>` and `/evidence fail <id> <reason>`
  slash commands.**  The sub-command structure is in place; just
  need to add the two arms mirroring `claim`.  ~30 min.

- **Verifier agent that flips `pending` → `verified|failed` based
  on policy** (e.g. "verify any claim with a PR-URL witness when CI
  passes").  This is the *interesting* next step — proves the system
  can drive its own audit loop.

### Maestro arc

- **Task 3 (state-machine workflows)** is the next architectural
  piece after Evidence.  Worth a brainstorm session before starting
  — what's the right granularity for a workflow vs a task vs a
  prompt?

- **Task 4 (Agent principles)** — a small text file per agent
  encoding its operating principles, surfaced into each prompt.
  Mostly UX work; can probably be a single-session feature.

### Smaller follow-ups

- The `dead_code` warning on `SqliteMemoryStore.config` has been
  there since the SQLite backend landed.  Either use it (e.g. expose
  via a getter for diagnostics) or `#[allow(dead_code)]` with a
  comment.  Low priority.

- The `dead_code` warning on `App.tx` is similar — the field is
  stored but never read after construction.  Worth investigating
  whether it can be removed entirely.

- Per-pricing CSV currency / region overrides — DeepSeek wrote a TODO
  in `pricing.rs` last week.  Still open.

---

## Files touched

```
crates/ch-protocol/src/lib.rs                 +124
crates/ch-memory/src/lib.rs                   +109
crates/ch-memory/src/backends/sqlite.rs       +334
crates/ch-memory/src/writer.rs                +83 -15
crates/ch-tui/src/app.rs                      +271 -56
crates/ch-tui/src/main.rs                     +156 -8
crates/ch-agent/src/drivers/subprocess.rs     fmt
crates/ch-agent/src/runtime.rs                fmt
crates/ch-core/src/lib.rs                     fmt
crates/ch-core/src/pricing.rs                 fmt
```

Net: ~+1000 lines, ~-80 lines.  ~10 unit tests added.

---

## Anti-patterns (followed)

- ✅ No spawned-task git worktrees.
- ✅ No force-pushes to `main`.
- ✅ No schema migrations — `evidence` is a NEW table, `CREATE TABLE
  IF NOT EXISTS` only.  Existing `messages` table untouched.
- ✅ No auto-detection of evidence in agent prompt output.  Manual
  `/evidence claim` first, agent-side opt-in later.
- ✅ No new top-level workspace deps.
