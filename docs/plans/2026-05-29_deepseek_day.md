# Day Plan for DeepSeek — 2026-05-29

> **Audience**: DeepSeek (or any next coding agent).  Self-contained —
> no prior conversation context needed.
>
> **Theme of the day: close the workflow loop end-to-end before
> starting anything new.**  Storage layer landed yesterday; UI side
> didn't.  Same shape as rich-text-input two days ago (movement
> worked, indicator missing).  Pattern is recurring.  Fix it.

---

## Where we are

You shipped Workflow Task 3's storage layer yesterday:

```
da99064 fix: clean up protocol duplicates + add workflow storage/writer
94a6ffa feat(workflow): Pending->Claimed transition — full vertical slice
bc542ed refactor(tui): complete Theme token migration — zero bare Color::* in app.rs
8a4bf46 chore: cargo fmt --all
da801e3 feat(tui): ? modal overlay + workflow design brainstorm
```

**State:**
- Workflow storage exists (`WorkflowClaimMsg`, `WorkflowStepRow`,
  `WorkflowStore` trait, SQLite impl, writer dispatch).
- Workflow **UI surface does not exist** — nothing visible to the user.
- 121 lib + 25 ch-tui binary = **146 tests green**.
- Cursor indicator landed (rich text input is now end-to-end).
- ? modal landed.
- Local `main` is 5 commits ahead of `origin/main`.

**The half-shipped pattern.**  Twice in the last week you've shipped
the engineering side of a feature and left the user-facing surface
for "next session":
- Rich text input: cursor movement worked, indicator missing (fixed
  yesterday)
- Workflow: storage works, no TUI surface (still open)

**Discipline for today:** don't start any new architectural piece
until workflow is end-to-end visible.  Every half-shipped feature is
a future cleanup PR + a confused user.

**Read first:**
1. `docs/journals/2026-05-28_development_journal.md` — yesterday's recap
2. `docs/journals/2026-05-26_workflow_design_brainstorm.md` — what
   you decided about TUI surface (Tab vs overlay).  **Re-read the
   "TUI surface?" decision before Task 4.**
3. `docs/DESIGN_SYSTEM.md` §11 (overlays/modals) — relevant if your
   brainstorm picked overlay

---

## Today's scope — close the loop, then maybe one polish item

| # | Task | Effort | Why |
|---|------|-------:|-----|
| 1 | Protocol doc-comment cleanup in `MessageType` | ~5 min | Sed runs left WorkflowClaim wearing EvidenceVerify's doc + Custom wearing WorkflowClaim's doc.  Readability fix. |
| 2 | Workflow storage tests | ~20 min | Yesterday promised ~6 tests; none landed.  Schema round-trip, claim transition, NotFound, by_workflow ordering. |
| 3 | `/workflow claim <step_id>` slash command | ~45 min | Closes the loop end-to-end.  Proves the data path without a new tab. |
| 4 | TUI surface decision-check (no code) | ~10 min | Re-read your brainstorm; confirm Tab vs overlay choice; **decide before building**.  Avoids building both. |
| 5 | Workflow YAML example with state field | ~15 min | Documents the user-facing format; useful for the next agent. |
| 6 | (Stretch) Build the chosen TUI surface | ~1-1.5 hr | Only if Task 4's decision is clear AND time remains. |

Total realistic: **~80 min for Tasks 1-3** (closes the loop), **~2 hr
for 1-5**, **~3 hr including stretch Task 6**.

---

## Pre-flight (5 min)

```bash
cd <repo>
git fetch origin
git checkout main
git pull origin main                  # noop if you've already pushed
git push origin main                  # 5 local commits ahead; push them
git status                            # must be clean
cargo test --workspace --lib          # 121 passing
cargo test -p ch-tui --bin crow       # 25 passing
```

If push fails because someone else pushed in the meantime, `git pull
--rebase` and try again.  Stop and report if tests fail.

**Quick sanity grep** (per yesterday's flag about cursor_pos):

```bash
grep -n 'cursor_pos:' crates/ch-tui/src/app.rs
# Should return exactly 2 hits: field declaration + 1 init in App::new
```

If you get more than 2 hits, there's a duplicate init to remove.

---

## Task 1 — Protocol doc-comment cleanup (~5 min)

**Current state of `crates/ch-protocol/src/lib.rs::MessageType`** (per
the system snapshot):

```rust
Evidence,
/// Verifier flips an existing evidence row to `verified` or `failed`.
/// Maestro Task 2.
WorkflowClaim,              // ← inherits the wrong doc comment
EvidenceVerify,             // ← lost its doc
/// Custom message type
/// Agent claims a workflow step (Pending -> Claimed)
Custom(String),             // ← wears WorkflowClaim's doc on top of its own
```

**Two problems:**
1. Doc comments orphaned by the sed-driven insertion of `WorkflowClaim`
2. `WorkflowClaim` wedged between `Evidence` and `EvidenceVerify`,
   breaking the Evidence/EvidenceVerify pairing readability

**Fix:**

```rust
/// Agent claim that a task has been completed — written to the
/// `evidence` table with `status = pending` until a verifier flips it.
/// Maestro Task 2.
Evidence,
/// Verifier flips an existing evidence row to `verified` or `failed`.
/// Maestro Task 2.
EvidenceVerify,
/// Agent claims a workflow step (Pending -> Claimed).  Maestro Task 3.
WorkflowClaim,
/// Custom message type
Custom(String),
```

No behaviour change (serde rename_all means variant order doesn't
affect serialization).  Pure readability.

**Commit**: `chore(protocol): restore doc comments after WorkflowClaim insertion`

---

## Task 2 — Workflow storage tests (~20 min)

**Yesterday's plan promised ~6 tests; the summary says 121 passing
but no new workflow tests landed.**  Add them now while the API is
fresh in your head.

Mirror the Evidence test set in `ch-memory::backends::sqlite::tests`:

```rust
// ~6 tests, ~150 LOC including helpers
async fn fresh_store() -> SqliteMemoryStore { ... }
fn sample_workflow_step(...) -> WorkflowStepRow { ... }

#[tokio::test] async fn workflow_step_schema_round_trip()
#[tokio::test] async fn workflow_claim_transitions_pending_to_claimed()
#[tokio::test] async fn workflow_claim_unknown_step_returns_not_found()
#[tokio::test] async fn workflow_by_workflow_returns_steps_in_order()
#[tokio::test] async fn workflow_by_status_filters_correctly()  // if you added by_status
#[tokio::test] async fn workflow_already_claimed_step_is_idempotent()  // OR errors — design choice
```

The last one — what happens when you `claim()` an already-Claimed
step?  Pick one (idempotent / error / advance to next state) and
document the choice in the test name.  Whichever you pick, this is
a question Workflow Task 3 slice 2 will care about, so codify the
answer now.

**Test count**: 121 → ~127 lib tests.

**Commit**: `test(workflow): storage layer tests for claim transition`

---

## Task 3 — `/workflow claim <step_id>` slash command (~45 min)

The closing-the-loop task.  Without this, workflow is invisible to
users — they can only exercise it from Rust code.

### Implementation

Mirror `/evidence claim` (in `crates/ch-tui/src/app.rs`).  Same shape:

1. Add `"/workflow"` arm to `handle_slash_command`
2. Sub-command parsing: `claim` is the only verb for now (room for
   `/workflow done`, `/workflow fail` later)
3. Build `WorkflowClaimMsg { step_id, ... }`
4. Render local feedback: e.g. `🪧 You → claimed step <step_id>`
   (pick a glyph that doesn't collide with `⇄` or `📋`)
5. Broadcast as `MessageType::WorkflowClaim + Payload::WorkflowClaim`
6. Add `/workflow` to `SUPPORTED_COMMANDS` + `help_lines()`
7. Add `🪧` to the chat scope filter pass-through list

### Tests (~2)

- `help_lines_documents_workflow_command` — your existing regression
  test will probably fail until you add `/workflow` to SUPPORTED_COMMANDS,
  which is the point.
- One more for the slash parsing if you can extract it.

### Acceptance

- TUI manual: `/workflow claim step-42` → shows `🪧 You → claimed
  step step-42` in chat
- Quit, then... wait, there's no `crow memory workflow` CLI yet.
  Either:
  - **(a)** Add `crow memory workflow` subcommand in this task
    (~20 extra min, mirrors `crow memory evidence`)
  - **(b)** Defer and just verify via `cargo test` for now

I'd lean **(a)** — adds 20 min but makes the manual smoke complete.
Your call based on time.

**Commit**: `feat(workflow): /workflow claim slash command + (optional) memory CLI`

---

## Task 4 — TUI surface decision-check (~10 min, no code)

**Re-read** `docs/journals/2026-05-26_workflow_design_brainstorm.md`,
specifically the "TUI surface?" section.  Confirm what you decided
between:
- New `Tab::Workflow` (5th top-level tab)
- Overlay on Monitor (extra column or expandable row)
- Side panel in Home tab
- Standalone slash command (just `/workflow status` shows transient overlay)

If the brainstorm explicitly picked one, you're done — move to Task 5.

If the brainstorm left it open or you've changed your mind based on
shipping the storage layer, **make the decision now** and write it
into the brainstorm doc as an update.  Don't write any TUI code in
this task — that's Task 6 stretch.

**Why this matters:** a 5th tab is significant chrome that needs
keybinding cycling updates, focus management, render layout
changes, scope filter updates.  An overlay is much cheaper (~50
lines, no keybinding changes).  Building both wastes a session.

**Commit (if decision changed)**: `docs(brainstorm): finalize workflow TUI surface choice`

---

## Task 5 — Workflow YAML example with state field (~15 min)

The user-facing surface for *defining* workflows (vs the bus-level
transitions in Task 3) is YAML.  Add an example to
`examples/workflows/` (create the dir if it doesn't exist):

```yaml
# examples/workflows/multi_agent_review.yaml
name: multi_agent_review
description: Two-step code review with a writer and a reviewer
steps:
  - id: write_patch
    agent: claude-wsl-ubuntu
    prompt: |
      Implement the feature described in the issue.
    state: pending          # initial state; orchestrator transitions
  - id: review_patch
    agent: gemini-wsl-ubuntu
    prompt: |
      Review {{ write_patch.output }} for correctness.
    depends_on: [write_patch]
    state: pending
```

This example doesn't have to *execute* — the orchestrator wiring is
out of scope for today.  It just **documents the format** so a user
(or future agent) knows what a state-aware workflow looks like.

**Commit**: `docs(workflows): add multi_agent_review YAML example`

---

## Task 6 — (Stretch) Build the chosen TUI surface (~1-1.5 hr)

**Only if Tasks 1-5 land with time to spare** AND Task 4 produced a
clear decision.

Don't sketch implementation here — the brainstorm + your decision
in Task 4 are the spec.  Just one reminder: **token discipline**.
First new render code since the migration must not introduce bare
`Color::*`.  Grep guard at end:

```bash
grep -n 'Color::' crates/ch-tui/src/app.rs | grep -v '^.*//'
# Should return zero hits outside the `use` import line
```

**Commit**: `feat(tui): <Tab::Workflow|workflow overlay|...> renders claimed steps`

---

## General conventions (unchanged)

| Topic | Convention |
|---|---|
| Commits | Imperative + scoped (`feat(workflow): ...`, `chore(protocol): ...`) |
| Tests | Every behavior change adds ≥1 test |
| CI gate | `cargo test --workspace --lib` must pass (current baseline: **121**) |
| Style | `cargo fmt --all` before commit |
| Token discipline | No bare `Color::*` outside `theme.rs` |
| Branches | Direct to `main` for small commits; PR for cumulative reviews |

## What to AVOID

- ❌ **No new architectural pieces today.**  Close workflow's loop
  first.  P3 items (Skill marketplace, Agent collab OS, GUI/Tauri)
  stay deferred until workflow is end-to-end visible.
- ❌ **No building both Tab::Workflow AND overlay.**  Task 4 decides;
  Task 6 builds.
- ❌ **No spawned-task git worktrees.**
- ❌ **No force-pushes to `main`.**
- ❌ **No schema migrations.**  Workflow steps table already exists
  from yesterday; don't `ALTER` it.
- ❌ **No orchestrator wiring beyond `Pending → Claimed`.**  Other
  transitions (Claimed → InProgress → Done, Failed paths) each get
  their own narrow slice in future sessions.
- ❌ **No LLM step-detection.**  Manual bus messages only, same
  discipline as Evidence.

## Reporting back

End-of-day journal at `docs/journals/2026-05-29_<topic>.md`.  Cover:

- What shipped (commits + test count delta from 121 lib / 146 total)
- Did the workflow loop close end-to-end? (Smoke: `/workflow claim`
  → query the table → row visible?)
- Task 4 decision (Tab vs overlay) and the one-line motivation
- Whether Task 6 stretch landed; if not, the carry-over plan
- The most surprising thing about the storage tests in Task 2 (e.g.
  "I expected double-claim to be idempotent but the spec ended up
  preferring an error")
- Half-shipped-pattern check: at end of day, is anything else
  in the codebase in the "engineering ships, UI trails" state that
  should go on next session's list?

If you only ship Tasks 1-3, that's the headline (workflow is
end-to-end visible).  Tasks 4-5 are decision + documentation.  Task 6
is gravy.

---

## Out of scope (future plans)

- **Other workflow transitions** (Claimed → InProgress → Done, Failed
  paths) — each its own narrow PR per Task 3's pattern.
- **Workflow YAML execution** — running the example through an
  orchestrator that actually drives state transitions.  Needs
  another design pass on how the orchestrator picks agents and
  threads correlation_ids.
- **Per-agent principles wiring** — the `TaskSpec.principles` field
  is plumbed but no agent reads it yet.  Slot in when you have a 1-2
  hr gap.
- **GitHubPrRule** for verifier — non-trivial-input demo (~45 min).
- **Notification toast** for handoff/evidence/workflow events.
- **Markdown rendering in chat** — whole-session task.
- **Sparklines in Monitor** — depends on whether you verified §8.3
  compliance yet.
- **Skill marketplace, Agent collab OS, Tauri GUI** — P3, untouched.

---

## One process flag for the day

You've shipped 2 features in the half-shipped pattern (rich text input,
workflow).  After today, check: **what other features are
engineering-deep but UI-shallow?**  Add the answer to the carry-over
section of tomorrow's journal so the next agent doesn't have to
discover it from a screenshot.  Examples worth checking:
- `TaskSpec.principles` (added, plumbed where?)
- Per-agent cost tracking (in pricing.toml, surfaced in Monitor?)
- Anything in `ch-memory` that has a CLI subcommand but no TUI panel
