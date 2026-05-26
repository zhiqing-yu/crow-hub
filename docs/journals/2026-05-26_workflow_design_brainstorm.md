# 2026-05-26 — Workflow Design Brainstorm

**Author:** DeepSeek | **Source:** `docs/plans/2026-05-26_deepseek_day.md` Task 2

## Decisions

| Question | Choice | Reasoning |
|----------|--------|-----------|
| Who emits transitions? | **Hybrid (C)** | Orchestrator infers basic (TaskResponse→Done). Agents self-report nuanced states (Claim, Review) via explicit bus messages. |
| Pending→Claimed trigger? | **Explicit WorkflowClaim message** | Cleanest audit trail: message in log, writer persists, TUI renders. No implicit correlation. Same pattern as Evidence. |
| Failed↔Evidence? | **Independent (first ship)** | Steps and claims are orthogonal. Linking adds complexity for a future PR. |
| TUI surface? | **New Tab::Workflow (5th tab)** | First-class space, follows existing tab pattern. Transient overlay is missable. |

## Minimum-viable lifecycle

```
Pending ──(WorkflowClaim msg)──→ Claimed ──(future PR)──→ InProgress ──→ Done
                                                          │
                                                          └──(future PR)──→ Failed
```

Task 3 scope: only Pending → Claimed.

## Out of scope for first ship

- Parallel step execution, rollback, deadline transitions
- Automatic inference (hybrid deferred — all transitions via explicit messages)
- Workflow CRUD in TUI (read-only view only)

## Implications for Task 3 (~175 LOC, ~5 tests)

| File | Lines | What |
|------|------:|------|
| ch-protocol/src/types.rs | ~15 | WorkflowClaim struct + MessageType + Payload variants |
| ch-memory/src/lib.rs | ~30 | WorkflowStepRow + WorkflowStore trait + claim_step() |
| ch-memory/src/backends/sqlite.rs | ~60 | CREATE TABLE workflow_steps + impl WorkflowStore |
| ch-memory/src/writer.rs | ~20 | Dispatch Payload::WorkflowClaim → claim_step() |
| ch-tui/src/app.rs | ~50 | Tab::Workflow + tab bar + render panel + /workflow claim |
| Tests | ~5 | table migration, claim transition, NotFound on unknown step |

Pattern mirror: Evidence stack (cfad27e..bf4a71a).
