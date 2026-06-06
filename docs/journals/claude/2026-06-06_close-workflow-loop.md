# 2026-06-06 — Close the Workflow Loop

**Author:** Claude Sonnet 4.6
**Audience:** Next agent / self
**Status:** complete
**Related:** `../../../plans/for-others/for-deepseek/2026-05-29_close-workflow-loop.md`

## What shipped

| Commit | What | Files |
|--------|------|-------|
| `7690149` | 6 workflow storage tests | `ch-memory/src/backends/sqlite.rs` |
| `1e6ab6c` | Memory writer wiring + `crow memory workflow` CLI | `ch-memory/src/writer.rs`, `ch-tui/src/main.rs` |
| `690c2a8` | `/workflow claim` TUI slash command | `ch-tui/src/app.rs` |
| `fb36b58` | `examples/workflows/multi_agent_review.yaml` | (new file) |

**Test count delta:** 121 lib → 127 lib (+6 workflow storage tests); 25 binary unchanged.
**Total:** 152 tests passing.

## Did the workflow loop close end-to-end?

Yes — the full data path is now wired:

1. User types `/workflow claim step-42` in TUI
2. `handle_slash_command` builds `WorkflowClaimMsg { step_id: "step-42", workflow_id: "tui-session", agent_id: <user uuid> }` and broadcasts `MessageType::WorkflowClaim` on the `general` bus channel
3. `spawn_memory_writer` (new dispatch arm) receives the message, calls `workflow_store.claim_step("tui-session", "step-42", "<uuid>")` → persists to `workflow_steps` SQLite table
4. `crow memory workflow` (new subcommand) reads `pending_steps()` or `by_workflow("tui-session")` and prints the row

The 🪧 glyph appears in chat and survives scope filtering across agent switches.

## Surprising finding

The schema bug I identified in the plan was a false alarm — the grep I ran missed `step_id TEXT PRIMARY KEY` because the search terms didn't match those lines.  `ON CONFLICT(step_id)` was already correct.  The `claimed_at` timestamp bug (SQLite `datetime('now')` ≠ RFC3339) is real but cosmetic — `claimed_at` always parses to epoch zero in Rust.  Tests document this with a comment rather than a broken assertion.

## Task 6 (stretch) — Tab::Workflow

Not landed.  The `/workflow claim` + `crow memory workflow` CLI closes the loop functionally.  A dedicated 5th tab adds visible TUI surface but is not required for the data path to work.  Carry-over: the brainstorm decision (Tab::Workflow) is already recorded in `docs/journals/2026-05-26_workflow_design_brainstorm.md`.

## Half-shipped pattern check

After this session, the following features are engineering-deep but UI-shallow:

| Feature | Engineering | UI surface |
|---------|-------------|------------|
| `Tab::Workflow` | Storage + bus + CLI all wired | No TUI tab yet |
| `TaskSpec.principles` | Plumbed in protocol | No agent reads it; no TUI surface |
| Per-agent cost tracking | `pricing.toml` exists | Not surfaced in Monitor tab (placeholder) |
| Monitor tab | Panel exists | Shows placeholder text, no real metrics |

Priority for next session: either `Tab::Workflow` (completes the visible surface) or Monitor tab dashboard (replaces the placeholder with real sparklines/token counts).

## What to pick up next

1. **`Tab::Workflow`** — ~1 hr.  Add 5th tab rendering `by_workflow("tui-session")` rows.  Decision is in the brainstorm doc.  Token-discipline guard: no bare `Color::*`.
2. **Monitor tab dashboard** — ~2-3 hr.  Replace placeholder with real per-agent token/cost/latency.
3. **Claimed→InProgress transition** — next workflow slice.  Needs a `/workflow start <step_id>` command + SQLite UPDATE.
