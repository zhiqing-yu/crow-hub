# Day Plan for DeepSeek — 2026-05-26

> **Audience**: DeepSeek (or any next coding agent).  Self-contained —
> you do NOT need any prior conversation context to execute this.
>
> Coexists with `docs/plans/2026-05-26_onward.md` (your own multi-week
> forward roadmap).  That's the priority list; **this** is the
> actionable day plan that takes precedence for the next session.

---

## Where we are

You shipped a huge amount overnight (commits `b6695a3 .. f5cfbbf`):

```
f5cfbbf feat(protocol): add WorkflowStepState enum to WorkflowStep
e5e0c18 feat(tui): 3-segment footer — shortcuts | You→agent | v0.1.0
f0c8bb0 refactor(tui): complete Theme token migration — zero bare Color::* in app.rs
a96a446 docs: add Theme token cleanup as P1 prerequisite
b883f5e chore: commit pending fmt + polish plan updates
b6695a3 docs: 2026-05-25 journal + forward development plan
2ca3e93 feat(evidence): polling verifier with KeywordRule — autonomous audit loop
f00a435 feat(evidence): auto-emit evidence from handoff + /evidence verify/fail
```

All bundled into [PR #3](https://github.com/zhiqing-yu/crow-hub/pull/3)
for review (open against `main`).

**Test count**: 121 lib + 25 ch-tui binary = **146 total**, all green.

**Read first** (in order):
1. `docs/DESIGN_SYSTEM.md` — your visual-design contract; sections §11.2 (help modal) and §8.3 (monitor dashboard) get referenced today
2. `docs/journals/2026-05-25_development_journal.md` — yesterday's recap (your own)
3. `docs/plans/2026-05-26_onward.md` — your forward roadmap; this day plan executes the front of it

---

## Today's scope — close the discoverability story, then **think** before workflow code

The temptation is to start wiring orchestrator state transitions
immediately because `WorkflowStepState` exists.  **Don't.**  The enum
is groundwork; the lifecycle it represents isn't designed yet.
Shipping orchestrator wiring without that design = a 1500-line PR
that nobody reviews well, then a refactor in two weeks.

Order: ship the cheap visible win (`?` modal), do the design pass
(brainstorm doc, no code), THEN cut the narrowest vertical slice of
workflow code that proves the pattern.

| # | Task | Effort | Why |
|---|------|-------:|-----|
| 1 | `?` modal overlay for full help | ~1 hr | Closes progressive-disclosure L1 gap; stress-tests new Theme tokens |
| 2 | Workflow design brainstorm (docs-only) | ~1 hr | Answer 4 open questions before code; choose minimum-viable lifecycle |
| 3 | First workflow transition: `Pending → Claimed` | ~2-3 hr | Narrow vertical slice — agent emits, writer persists, TUI renders. Sets the pattern. |
| 4 | (Small) Verify Monitor Tab matches DESIGN_SYSTEM §8.3 | ~20 min | Status check; not previously screenshotted post-tab-rename |
| 5 | (Small) `.gitattributes` for CRLF normalization | ~5 min | Prevents future `b883f5e`-style noise commits |

Total realistic: **~5 hrs** (tasks 4 + 5 can slot between bigger ones).

---

## Pre-flight (5 min)

```bash
cd <repo>
git fetch origin
git checkout main
git pull origin main                  # may auto-merge PR #3 if you've merged it
git status                            # must be clean
cargo test --workspace --lib          # 121 passing
cargo test -p ch-tui --bin crow       # 25 passing
```

Stop and report if anything fails.

If PR #3 is still open and not merged, you can either:
- Merge it via the GitHub UI first (cleanest)
- Or work directly on `main` (which already has the commits locally) and let the PR catch up

---

## Task 1 — `?` modal overlay for full help (~1 hr)

**Why first**: the footer (`e5e0c18`) gives L0 always-visible
shortcuts.  `/help` gives L2 detail but dumps into chat where it
scrolls off.  Missing the L1 middle: on-demand reference that
doesn't pollute scrollback.  DESIGN_SYSTEM §11.2 specs it out
(centered overlay, 60% × 60%, `?` or `Esc` dismisses).

### State

Add to `App`:

```rust
pub show_help_overlay: bool,
```

Initialise `false` in `App::new`.

### Input handling

In the main key handler in `app.rs`, add **before** the panel-specific
handlers so `?` works globally regardless of focus:

```rust
KeyCode::Char('?') if app.focused_panel != FocusedPanel::Input => {
    app.show_help_overlay = !app.show_help_overlay;
    return;  // consume the keystroke
}
KeyCode::Esc if app.show_help_overlay => {
    app.show_help_overlay = false;
    return;
}
```

The `FocusedPanel::Input` guard prevents `?` typed into a chat message
from opening the modal.  Pragmatic for first ship; can refine later
if users complain.

### Rendering

After all normal panel drawing AND after the footer (so the modal
floats above everything):

```rust
if app.show_help_overlay {
    let area = centered_rect(60, 60, frame.size());
    frame.render_widget(Clear, area);          // blank the area
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(app.theme.accent_primary())
        .title(" Help — ? or Esc to close ")
        .style(app.theme.bg_overlay());
    let lines: Vec<Line> = help_lines().into_iter().map(Line::from).collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
```

`centered_rect(pct_x, pct_y, area)` is a standard ratatui helper —
look it up in the ratatui examples if it's not already in our code.
~10-line pure-math helper.

### Token discipline reminder

This is the first new render code since the token migration.  **No
bare `Color::*` literals.**  Every styling decision must come through
`app.theme.<helper>()`.  If you need a token that doesn't exist (e.g.
no `accent_primary()` yet), add it to `theme.rs` first.

### `/help` stays

Don't remove the `/help` slash command — keep it as the chat-output
variant some users prefer.  The `?` modal is *additive*, not a
replacement.

### Tests (~2 in `app.rs::tests`)

- `centered_rect_returns_proportional_centered_rect` — math sanity:
  given a 100×100 area and 60/60 pct, returns a 60×60 rect centered.
- `pressing_question_toggles_help_overlay` — extract toggle into a
  small helper `toggle_help_overlay(&mut bool)` and test it (avoids
  having to construct a full `App`).

### Acceptance

- In the running TUI, `?` opens the modal regardless of focused
  panel (except Input typing).
- `Esc` dismisses it.
- The modal renders above the footer (no clipping).
- Theme tokens used — no new `Color::*` literals in app.rs (grep
  guard: `grep -n 'Color::' crates/ch-tui/src/app.rs` returns zero
  hits outside imports).

**Commit**: `feat(tui): ? modal overlay for full help (preserves /help slash)`

---

## Task 2 — Workflow design brainstorm (~1 hr, **docs-only**)

**Output**: a new file at
`docs/journals/2026-05-26_workflow_design_brainstorm.md`.  No code.
Pick one answer per question, motivate briefly, propose the
minimum-viable lifecycle and TUI surface.

### Four questions to answer

1. **Who emits state transitions?**
   - Option A: agent self-reports ("I claim step 3", "I'm done with step 3")
   - Option B: orchestrator infers from bus events (TaskResponse → step done)
   - Option C: hybrid (orchestrator infers default, agents can override via explicit ack)

2. **What's the trigger for `Pending → Claimed`?**
   - First TaskRequest emitted with `step_id` correlation?
   - First TaskResponse received on a workflow channel?
   - An explicit `WorkflowClaim` bus message from an agent?

3. **How does `WorkflowStepState::Failed` interact with the existing Evidence `fail()` path?**
   - They stay independent (steps and claims are orthogonal)
   - A failed step auto-fails all its evidence rows
   - A failed evidence row escalates to step failure if it was the only one

4. **TUI surface for workflow visibility?**
   - New `Tab::Workflow` (5th tab)
   - Overlay on Monitor tab (extra column or expandable row per agent)
   - Side panel in Home tab
   - Standalone slash command (`/workflow` shows a transient overlay)

### Required output format

```markdown
# 2026-05-26 — Workflow Design Brainstorm

## Decisions
| Question | Choice | One-line reason |
|----------|--------|-----------------|
| Who emits transitions? | <choice> | ... |
| Pending→Claimed trigger? | <choice> | ... |
| Failed ↔ Evidence fail? | <choice> | ... |
| TUI surface? | <choice> | ... |

## Minimum-viable lifecycle (for first ship)
... 5-10 lines describing the simplest path through the state machine
that you'll implement in Task 3.

## Out of scope for first ship
- Parallel step execution
- Rollback / state reversal
- ... (etc — list anything you considered and deferred)

## Implications for Task 3
Concrete sketch (file paths + 1-line description per file) of what
Task 3 will actually touch.  Should be < 200 LOC across 4-6 files.
```

### Why force the format

This is *the* artifact that makes Task 3 reviewable.  If Task 3's
diff matches the "Implications for Task 3" sketch, review is
mechanical.  If it diverges, the brainstorm gets updated first, then
the code.  No mid-PR scope creep.

**Commit**: `docs(brainstorm): workflow design — answers + minimum-viable lifecycle`

---

## Task 3 — First workflow transition (~2-3 hr, scope set by Task 2)

**The narrow vertical slice.**  Implement *only* the
`Pending → Claimed` transition, exactly as Task 2's brainstorm
specified.  Everything else (Claimed → InProgress, etc.) is
out-of-scope for this session — they each get their own narrow PR
later.

### Likely shape (refine based on brainstorm)

If the brainstorm picks "hybrid emission + explicit `WorkflowClaim`
message + new Workflow tab", the diff sketch is:

| File | Lines | What |
|------|------:|------|
| `ch-protocol/src/types.rs` | ~30 | `WorkflowClaim { step_id, task_id, agent_id }` struct, `MessageType::WorkflowClaim`, `Payload::WorkflowClaim` |
| `ch-memory/src/lib.rs` | ~40 | `WorkflowStepRow` + `WorkflowStore` trait (`write_step`, `claim`, `by_workflow`) |
| `ch-memory/src/backends/sqlite.rs` | ~80 | New `workflow_steps` table + impl |
| `ch-memory/src/writer.rs` | ~25 | Dispatch `Payload::WorkflowClaim` → `claim()` |
| `ch-tui/src/app.rs` | ~50 | `/workflow claim <step_id>` slash command OR Workflow tab/overlay render |
| Tests | ~6 | Schema, claim transition, NotFound on unknown step |

**Total**: ~230 LOC + ~6 tests.  Brainstorm sketch in Task 2 should
match this scale; if it balloons past 400 LOC, flag it and split.

### Pattern to mirror

The Evidence stack (`cfad27e`, `db98628`, `45b0168`, `fa6206d`,
`bf4a71a`) is the template.  Same shape:
- Protocol additions (additive, `#[serde(default)]`)
- Storage trait + SQLite impl + tests
- Memory writer dispatch
- (Optional) CLI subcommand
- TUI slash command for manual testing

### Anti-patterns specific to workflows

- ❌ Don't implement state reversal (`Claimed → Pending`) in this PR
- ❌ Don't auto-detect transitions from chat content; require explicit
  bus messages for first ship (mirror Evidence's manual-first design)
- ❌ Don't add an LLM-based step-completion classifier; same rule

### Acceptance

- Manual smoke: `crow` TUI → `/workflow claim step-1` → status flips
  in chosen UI surface
- `crow memory workflow` (if you add the CLI in this PR) shows the
  step's history

**Commit**: `feat(workflow): Pending → Claimed transition (Maestro 3, slice 1)`

---

## Task 4 — Verify Monitor Tab matches DESIGN_SYSTEM §8.3 (~20 min)

Status check, not a build task.  You claimed Monitor Tab was "already
in code" but the latest screenshot showed a basic table — no
sparklines, no per-agent cost column.  Confirm the gap, then either:

- **If gap is real**: file as a P1 carry-over in
  `docs/plans/2026-05-26_onward.md` (move it back to the front of P1)
- **If actually built**: take a screenshot for the journal and call
  it done

```bash
crow                                  # launch TUI
# navigate to Monitor tab, screenshot
# compare to DESIGN_SYSTEM.md §8.3
```

No commit unless you find + fix something.

---

## Task 5 — `.gitattributes` for CRLF normalization (~5 min)

The `b883f5e` commit added 1643 line "insertions" and 1643 "deletions"
in `app.rs` that were pure CRLF → LF normalization.  This will recur
every time someone with default Windows git settings touches a file
the previous committer's tooling formatted with LF.  Fix it once.

```bash
echo '* text=auto eol=lf' > .gitattributes
git add .gitattributes
git commit -m "chore: enforce LF line endings via .gitattributes

Eliminates the b883f5e-style CRLF normalization noise commits when
Windows + WSL + native macOS contributors interact via git."
```

That's it.  No code changes.

---

## General conventions (unchanged)

| Topic | Convention |
|---|---|
| Commits | Imperative + scoped (`feat(memory): ...`, `feat(tui): ...`) |
| Tests | Every behavior change adds ≥1 test |
| CI gate | `cargo test --workspace --lib` must pass (current baseline: **121**) |
| Style | `cargo fmt --all` before commit |
| Token discipline | No bare `Color::*` outside `theme.rs` (grep guard) |
| Branches | Direct to `main` for small commits; PR for cumulative reviews |

## What to AVOID

- ❌ **No spawned-task git worktrees.**  Standing rule since
  2026-05-13.
- ❌ **No force-pushes to `main`.**
- ❌ **No schema migrations.**  Workflow steps go in a NEW table —
  `CREATE TABLE IF NOT EXISTS`.
- ❌ **No orchestrator wiring beyond `Pending → Claimed` in Task 3.**
  Resist the urge to wire all transitions at once; review-fatigue
  kills quality.
- ❌ **No LLM step-detection** in Task 3.  Manual bus messages only,
  same discipline as Evidence.
- ❌ **No new top-level workspace deps** without checking `Cargo.toml`.

## Reporting back

End-of-day journal at `docs/journals/2026-05-26_<topic>.md`.  Cover:

- What shipped (commits + test count delta from 146)
- Whether Task 3 actually fit the < 230 LOC budget, or how much it
  exceeded
- One sentence per Task 4 / Task 5 outcome
- The most surprising thing in Task 2's brainstorm (the design
  decision you weren't expecting to make)
- Carry-over: what's the *next* workflow transition (claimed →
  in_progress probably) and what shape it'll take

If you only ship Task 1 + Task 2, that's a respectable day — Task 2's
brainstorm is the architectural piece that protects all future
workflow work.  Task 3 can carry over.

---

## Out of scope (for future plans)

- **Other workflow transitions** (Claimed → InProgress → Done, Failed
  paths) — each its own narrow PR per Task 3's pattern
- **Workflow YAML format** — extending the existing workflow file
  format to encode step state; needs another design pass
- **Per-agent principles** (Maestro Task 4) — small UX work, slot in
  when there's a 1-2 hr gap
- **GitHubPrRule** for verifier — useful demo of non-trivial rule
  inputs without networking; ~45 min when you want it
- **Notification toast** (handoff/evidence transient overlays) — from
  the polish plan; lower priority now that the footer + `?` modal
  cover the discoverability story
- **Markdown rendering in chat** — whole-session task
- **Sparklines in Monitor** — depends on Task 4's outcome
- **Tauri GUI** — Phase 6, week-scale effort
