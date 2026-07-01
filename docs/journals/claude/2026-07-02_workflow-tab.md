# 2026-07-02 — Workflow Tab

## What shipped

| Change | File(s) |
|---|---|
| `all_steps(limit)` added to `WorkflowStore` trait | `ch-memory/src/lib.rs` |
| SQLite impl of `all_steps` (ORDER BY claimed_at DESC) | `ch-memory/src/backends/sqlite.rs` |
| Test `workflow_all_steps_returns_across_workflows_and_all_states` | same |
| `Tab::Workflow` — 5th TUI tab, scrolling, state glyphs ○◐●✓✗ | `ch-tui/src/app.rs` |
| `WorkflowClaim` bus dispatch in memory writer | `ch-tui/src/app.rs`, `ch-memory/src/writer.rs` |
| `/workflow claim <step_id>` slash command | `ch-tui/src/app.rs` |
| `run_tui_app` wired with `workflow_store` | `ch-tui/src/main.rs` |
| PR [#6](https://github.com/zhiqing-yu/crow-hub/pull/6) | — |

## Test delta

+1 test (`workflow_all_steps_returns_across_workflows_and_all_states`). All 23 ch-memory lib tests pass.

## Lessons

- `all_steps` was the missing link: without it the Workflow tab could not show steps after they were claimed (they move out of `pending_steps` state immediately).
- Token discipline enforced: all glyphs use theme tokens (`status_idle`, `status_errored`, `status_thinking`, `border_focused`, `summary`), no bare `Color::` in app.rs.

## Carry-over

- Monitor tab: replace placeholder text with real CPU/RAM metrics from `ch-monitor`
- Claimed→InProgress transition: surface a `/workflow start <step_id>` command
- Workflow Done/Failed transitions
