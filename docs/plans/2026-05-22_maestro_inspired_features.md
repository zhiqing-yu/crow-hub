# Plan: Maestro-Inspired Features for Crow Hub

**Date:** 2026-05-22 | **Author:** DeepSeek | **Source:** `docs/journals/2026-05-19_brainstorming_maestro_lessons.md`

## 3 tasks — pick your priority

| # | Task | Effort | What you get |
|---|------|-------:|--------------|
| 1 | Handoff envelope message type | 1-2 hrs | Agent-to-agent task continuity |
| 2 | Evidence table + verification | 3-4 hrs | Auditable agent claims |
| 3 | State-machine workflows | 3-4 hrs | Structured multi-step tasks |
| 4 | (Bonus) Agent principles | 1-2 hrs | Behavioral rules in manifest |

---

## Task 1 — Handoff Envelopes

When an agent completes a task, it emits a structured handoff:

```rust
pub struct HandoffEnvelope {
    pub from_agent: String,
    pub summary: String,           // what was done
    pub decisions: Vec<String>,    // key decisions
    pub open_questions: Vec<String>,
    pub continuation_hint: String, // "pick up at step 3"
}
```

New `MessageType::Handoff` + `Payload::Handoff(...)`.  Handler emits on
task completion.  Memory writer persists it.  TUI shows `⇄` glyph.

~4 new tests.  ~100 lines of code across 3 files.

---

## Task 2 — Evidence Table

SQLite table for auditable claims:

```sql
CREATE TABLE evidence (
    id TEXT PRIMARY KEY,
    task_id TEXT, agent_id TEXT, claim TEXT,
    status TEXT DEFAULT 'pending',  -- pending|verified|failed
    witness TEXT, created_at INTEGER, verified_at INTEGER
);
```

Agent produces evidence → another agent verifies it → status changes.
Gates workflow completion on verified evidence.  `crow memory evidence`
subcommand.

~6 new tests.  ~200 lines across 3 files.

---

## Task 3 — State-Machine Workflows

Extend YAML workflow with state per step:

```yaml
steps:
  - id: "write-code"
    state: "pending"         # pending→claimed→in_progress→review→verified→done
    requires_evidence: true
```

Orchestrator manages transitions.  TUI Workflow panel shows timeline
(already mocked up in Reasonix brainstorming).

~8 new tests.  ~300 lines across 3 files.

---

## Task 4 — Agent Principles

Manifest extension:

```toml
principles = ["always_write_tests_first", "never_commit_secrets"]
```

Orchestrator attaches to task context.  Advisory initially.

~2 new tests.  ~50 lines across 2 files.

---

## Suggested today: Task 1

Quickest win, directly improves multi-agent collaboration, cleanly
separated from the other tasks.  If it goes fast, roll into Task 2.

Which one(s) do you want?
