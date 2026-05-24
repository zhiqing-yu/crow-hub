# Day Plan for DeepSeek — 2026-05-23

> **Audience**: DeepSeek (or any next coding agent).  Self-contained —
> you do NOT need any prior conversation context to execute this.

---

## Where we are

Yesterday Claude shipped **Maestro Task 1 — Handoff Envelopes** (commit
`48c2e76`).  Latest commits on `main`:

```
d85b9dc docs(journal): 2026-05-22 — Handoff envelopes (Maestro Task 1)
48c2e76 feat: Handoff envelopes — structured agent-to-agent task continuity
46da902 docs(plan): day plan for DeepSeek — 2026-05-22
b671d40 feat(tui): slash commands + scoped chat + agent metadata display
```

**Test count**: 117 lib + 20 ch-tui binary = 137 total, all green.

**Read first** (in order):
1. `docs/journals/2026-05-22_handoff_envelopes.md` — yesterday's recap +
   three explicit follow-ups for you
2. `docs/plans/2026-05-22_maestro_inspired_features.md` — the Maestro
   plan you wrote; Task 2 (Evidence table) is the headline today
3. `docs/journals/2026-05-19_brainstorming_maestro_lessons.md` — the
   strategic context for why Evidence + state-machine workflows matter

---

## Today's scope — finish the Handoff loop, then ship Evidence

Bias: **close the small loose ends first**, then go big.  Two warm-up
tasks (~20 min each) clear the loop and set up the rails, then the
main course is Maestro Task 2.

| # | Task | Effort | Why |
|---|------|-------:|-----|
| 1 | Bridge `Payload::Handoff` from bus → TUI chat panel | ~20 min | Currently remote handoffs only show in logs + DB; this surfaces them in the chat where agents can see what other agents are saying |
| 2 | `/help` audit | ~20 min | `/handoff` is the 3rd undocumented-by-default command since the last `/help` sweep; reconcile + add `/all` regression test |
| 3 | Maestro Task 2 — Evidence table | ~3-4 hrs | The next architectural piece toward "collaboration OS for agents" — auditable agent claims with verification status |
| 4 | (Stretch) Cross-link Handoff ↔ Evidence | ~30 min | When an agent emits a Handoff envelope, the `decisions` field becomes evidence rows (claim="<decision>", status="pending") — closes the loop |

Total realistic: **~5 hrs**.  Drop Task 4 if time-boxed.

---

## Pre-flight (5 min)

```bash
cd <repo>
git fetch origin
git checkout main
git pull origin main
git status                            # must be clean
cargo test --workspace --lib          # must show 117 passing
cargo test -p ch-tui --bin crow       # must show 20 passing
```

Stop and report if anything fails.

---

## Task 1 — Bridge `Payload::Handoff` from bus into TUI chat (~20 min)

**Symptom**: today, `/handoff <summary>` shows `⇄ You → <summary>` in
the local TUI (because the command pushes the line directly).  But if
a *remote* agent emits a Handoff envelope on the bus, the user sees
nothing in the chat panel — it only lands in `crow-hub.log` (INFO from
the runtime observer) and SQLite (memory writer).

**Fix**: the bus → TUI bridge task in `crates/ch-tui/src/main.rs` (around
line 322) currently only forwards `Payload::Text`.  Add a branch for
`Payload::Handoff`.

### Current code (around line 322)

```rust
tokio::spawn(async move {
    let mut rx = bus_rx;
    while let Some(msg) = rx.recv().await {
        if let Payload::Text(ref text) = msg.payload {
            let _ = tx_bridge
                .send((msg.from.agent_name.clone(), text.clone()))
                .await;
        }
    }
});
```

### New shape

```rust
tokio::spawn(async move {
    let mut rx = bus_rx;
    while let Some(msg) = rx.recv().await {
        match &msg.payload {
            Payload::Text(text) => {
                let _ = tx_bridge
                    .send((msg.from.agent_name.clone(), text.clone()))
                    .await;
            }
            Payload::Handoff(env) => {
                // Pre-format as a handoff line so the TUI's on_tick
                // (which appends "<agent>: <content>") and the chat
                // scope filter (which passes `⇄`-prefixed lines)
                // both render it correctly.  We send the special agent
                // name "⇄" so on_tick produces "⇄: <from> → <summary>".
                let line = format!("⇄ {} → {}", env.from_agent, env.summary);
                let _ = tx_bridge
                    .send(("__handoff__".to_string(), line))
                    .await;
            }
            _ => {}
        }
    }
});
```

But wait — `on_tick` always formats as `"{agent}: {response}"` (see
`crates/ch-tui/src/app.rs` around line 207).  That would render
`__handoff__: ⇄ <from> → <summary>` which doesn't start with `⇄`
and would be filtered by the scope filter.

**Cleaner fix**: extend `on_tick` to special-case `agent == "__handoff__"`
and push the response string as-is (so the line starts with `⇄`
directly).  ~5-line change in `app.rs`.

```rust
pub fn on_tick(&mut self) {
    self.tick_count = self.tick_count.wrapping_add(1);
    while let Ok((agent, response)) = self.response_rx.try_recv() {
        if agent == "__handoff__" {
            // Pre-formatted handoff line — push as-is, no merging.
            self.messages.push(response);
            continue;
        }
        // Existing streaming-merge logic...
        if let Some(last_msg) = self.messages.last_mut() {
            let prefix = format!("{}: ", agent);
            if last_msg.starts_with(&prefix) {
                last_msg.push_str(&response);
                continue;
            }
        }
        self.messages.push(format!("{}: {}", agent, response));
    }
}
```

### Test

Add one to `app.rs::tests`:

```rust
#[test]
fn on_tick_handoff_pushes_unprefixed() {
    // Pseudo-app construction omitted — use whatever helper exists
    // or construct App with mocked channels.
    // Send a ("__handoff__", "⇄ claude → done") tuple, tick, verify
    // the resulting message starts with "⇄" not "__handoff__:"
}
```

### Acceptance

- From one TUI instance, `/handoff testing remote display`.  Verify
  immediate local display as before.
- (Manual; can't be CI-tested) from `crow-hub.log`, confirm the runtime
  observer line is also there.
- The persisted entry shows in `crow memory tail` with `⇄`.
- If you have two TUI sessions on the same bus (or can simulate it),
  the second session also sees the `⇄ <from> → <summary>` line.

**Commit**: `feat(tui): bridge Payload::Handoff into chat panel`

---

## Task 2 — `/help` audit (~20 min)

`/help` was last touched when `/handoff` and `/all` were added but
the help text wasn't always updated together with the implementation.
Reconcile.

### Steps

1. `grep -n '"/'  crates/ch-tui/src/app.rs | head -30` — list every
   slash command match arm.
2. Cross-reference with the strings in the `/help` arm.  Make sure:
   - Every implemented command is documented
   - Every documented command exists
   - Examples in help text match actual usage (e.g. `/model -` to
     clear should be documented if implemented)
3. Add a single unit test that doesn't require a real TUI:
   ```rust
   #[test]
   fn help_text_lists_every_implemented_command() {
       // Build a list of command verbs by scanning the match arms
       // (or maintain a SUPPORTED_COMMANDS const and assert /help
       // mentions each).  Pick the option that fits your code style.
   }
   ```
   Pragmatic version: define a `pub const SUPPORTED_COMMANDS: &[&str] =
   &["/clear", "/model", "/all", "/handoff", "/help"];` and have the
   test assert each appears in the `/help` output.

### Acceptance

- `/help` lists every command, no orphan documentation
- The new test would fail if someone adds a `/foo` command without
  updating the constant + `/help` text

**Commit**: `chore(tui): audit /help + add SUPPORTED_COMMANDS regression test`

---

## Task 3 — Maestro Task 2: Evidence table (~3-4 hrs)

This is the headline.  See `docs/plans/2026-05-22_maestro_inspired_features.md`
Task 2 for the original design — your own write-up.  Recap:

> Every agent action becomes an evidence row: command, exit code,
> witness.  crow-hub: add `evidence` SQLite table (task_id, agent_id,
> claim, verification_status, witnessed_by).

### Sub-tasks

#### 3a. SQLite schema + crate API (`crates/ch-memory/`)

Add to `SqliteMemoryStore::init` (or wherever schema lives):

```sql
CREATE TABLE IF NOT EXISTS evidence (
    id              TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL,
    correlation_id  TEXT,
    agent_id        TEXT NOT NULL,
    agent_name      TEXT,
    claim           TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',  -- pending|verified|failed
    witness         TEXT,
    metadata        TEXT,                              -- JSON blob, optional
    created_at      INTEGER NOT NULL,
    verified_at     INTEGER,
    verified_by     TEXT
);
CREATE INDEX IF NOT EXISTS idx_evidence_task_id ON evidence(task_id);
CREATE INDEX IF NOT EXISTS idx_evidence_correlation ON evidence(correlation_id);
CREATE INDEX IF NOT EXISTS idx_evidence_status ON evidence(status);
```

Add new trait methods (or a separate `EvidenceStore` trait — your
call):

```rust
pub struct EvidenceRow {
    pub id: String,
    pub task_id: String,
    pub correlation_id: Option<Uuid>,
    pub agent_id: AgentId,
    pub agent_name: String,
    pub claim: String,
    pub status: EvidenceStatus,  // enum Pending|Verified|Failed
    pub witness: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    pub verified_by: Option<String>,
}

#[async_trait]
pub trait EvidenceStore {
    async fn write_evidence(&self, row: EvidenceRow) -> Result<()>;
    async fn verify(&self, id: &str, by: &str, witness: Option<String>) -> Result<()>;
    async fn fail(&self, id: &str, by: &str, reason: String) -> Result<()>;
    async fn by_task(&self, task_id: &str) -> Result<Vec<EvidenceRow>>;
    async fn pending(&self, limit: usize) -> Result<Vec<EvidenceRow>>;
}
```

Implement on `SqliteMemoryStore`.

#### 3b. Bus protocol (`crates/ch-protocol/src/lib.rs`)

Following the Handoff pattern from yesterday — additive:

```rust
pub enum MessageType {
    // ...existing variants...
    Evidence,
    EvidenceVerify,
}

pub struct EvidenceClaim {
    pub task_id: String,
    pub claim: String,
    pub witness: Option<String>,
}

pub struct EvidenceVerifyMsg {
    pub evidence_id: String,
    pub outcome: bool,           // true = verified, false = failed
    pub note: Option<String>,
}

pub enum Payload {
    // ...existing variants...
    Evidence(EvidenceClaim),
    EvidenceVerify(EvidenceVerifyMsg),
}
```

#### 3c. Memory writer subscriber

Extend `crates/ch-memory/src/writer.rs` — for `Payload::Evidence`, write
to the evidence table (not the messages table).  For
`Payload::EvidenceVerify`, call `verify()` or `fail()` on the matching
row.

#### 3d. CLI subcommand: `crow memory evidence`

```
crow memory evidence                       # all pending evidence
crow memory evidence --task <task-id>      # evidence for a task
crow memory evidence --status verified
```

Implementation pattern: mirror `crow memory tail` in
`crates/ch-tui/src/main.rs`.

#### 3e. TUI: `/evidence claim <text>` slash command

For testing the data path end-to-end without depending on agents:

```
/evidence claim built auth module
```

Builds an `EvidenceClaim` with `task_id = <current session_id or
last-correlation>`, emits as `MessageType::Evidence`.  Renders locally
as `📋 You → claimed: built auth module`.

(Use `📋` or another non-conflicting glyph; `⇄` is taken by Handoff.)

### Tests (~6)

- SqliteMemoryStore evidence schema round-trip
- `verify()` transitions Pending → Verified, sets verified_at + verified_by
- `fail()` transitions Pending → Failed
- `by_task` returns evidence for one task in created-at order
- `pending(limit)` returns only pending rows
- Protocol round-trip: Payload::Evidence and Payload::EvidenceVerify

### Acceptance

- `crow` TUI → `/evidence claim test claim`.  Then exit, run
  `crow memory evidence` — should show one pending row with claim="test
  claim", witness=None, status=pending.
- A future "verifier agent" can come along and emit
  `MessageType::EvidenceVerify` to flip status — for today, just having
  the data path work is enough; the verification agent is out of scope.

**Commit**: `feat(memory): evidence table + EvidenceStore trait + crow memory evidence`

(If 3a, 3b, 3c, 3d, 3e end up too big for one commit, split as
`feat(memory)` for the storage layer, `feat(protocol)` for the bus
messages, `feat(tui)` for the slash command, `feat(memory-cli)` for the
subcommand.)

---

## Task 4 — (Stretch) Cross-link Handoff and Evidence (~30 min)

If Tasks 1-3 land early, close the loop.

**Idea**: when an agent emits a `HandoffEnvelope`, each item in
`envelope.decisions` becomes an Evidence row with status="pending".
Hooks into the memory writer — when persisting a handoff, also
write N evidence rows for the decisions.

### Acceptance

- `/handoff finished auth refactor` with `decisions = ["use JWT"]`
  (TBD how `/handoff` collects decisions — extend the syntax later
  or have agents emit programmatically) creates 1 evidence row.
- `crow memory evidence` shows it.

This task is the smallest concrete proof that Handoff and Evidence
are designed to compose, not just coexist.

**Commit**: `feat(handoff): handoff decisions auto-emit as pending evidence`

---

## General conventions (unchanged from prior plans)

| Topic | Convention |
|---|---|
| Commits | Imperative + scoped (`feat(memory): ...`, `chore(tui): ...`) |
| Tests | Every behavior change adds ≥1 test |
| CI gate | `cargo test --workspace --lib` must pass before push (current baseline: 117) |
| Style | `cargo fmt --all` before commit |
| Branches | Direct to `main` for small commits |

## What to AVOID

- ❌ **No spawned-task git worktrees.**  The standing warning since
  2026-05-13.  Cleanup procedure: `2026-05-13_*.md` Section 1.
- ❌ No force-pushes to `main`.
- ❌ No schema migrations.  Evidence is a *new* table — `CREATE TABLE
  IF NOT EXISTS` is fine.  Don't `ALTER TABLE messages`.
- ❌ No new top-level deps without checking workspace `Cargo.toml`.

## Reporting back

End-of-day journal at `docs/journals/2026-05-23_<topic>.md` following
the chain.  Cover:

- What shipped (commits + test count delta)
- Whether Task 1 + 2 also landed, or just Task 3
- One surprise / design tension / learning
- Carry-over for the next agent (any partial Maestro tasks left)

If you only shipped Task 1 + Task 3, that's a strong day — Task 3 is
the architectural piece.  Task 2 can carry over.
