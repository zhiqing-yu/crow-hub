# Brainstorming: Lessons from Maestro (agent harness)

**Date:** 2026-05-19
**Author:** DeepSeek
**Source:** https://github.com/ReinaMacCredy/maestro

Maestro is a local-first agent harness for spec-to-ship. State lives in `.maestro/` on disk, not chat history. 7 ideas crow-hub can adopt:

## 1. Structured handoff envelopes
Maestro emits handoff envelopes at lifecycle transitions. Next agent picks up the file and resumes. crow-hub: add `Handoff` message type with summary/decisions/open questions. Already have `correlation_id`.

## 2. Evidence rows (auditable claims)
Every agent action = evidence row: command, exit code, witness. crow-hub: add `evidence` SQLite table (task_id, agent_id, claim, verification_status, witnessed_by).

## 3. Spec-driven workflows with state machines
Pending → claimed → in_progress → review → verified → done. Each transition emits bus event. Extend crow-hub's YAML workflow format.

## 4. Principles as behavioral rules
`principles.jsonl` stores rules agents must follow. crow-hub: add `principles` to manifests or shared TOML. Orchestrator enforces them.

## 5. Multi-screen Mission Control
Maestro TUI has 10 preview screens. crow-hub: add Dashboard, Events, Graph screens. Tab bar already planned.

## 6. Git-tracked ledgers
Maestro uses append-only JSONL tracked in git. crow-hub: add `crow memory export` to JSONL for reproducible collaboration.

## 7. Layer architecture enforcement
Maestro mechanically enforces dependency direction. crow-hub: document within-crate layer boundaries in AGENTS.md.

## What NOT to copy
- TypeScript vs Rust
- Human-conductor vs multi-agent autonomous
- PR-oriented exec-plans (no PR integration yet)
- Bespoke `.maestro/` directory

## Adoption order
P1 → Handoff envelopes, P2 → Evidence + state-machine workflows + principles, P3 → Mission Control screens + JSONL export
