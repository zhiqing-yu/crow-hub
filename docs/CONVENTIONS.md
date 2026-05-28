# Documentation Conventions

> **The single source of truth for how brainstorms, plans, and journals
> are organized in this repo.**  Any developer or AI agent contributing
> docs MUST follow this layout.  When in doubt, this document wins.

---

## 1. The problem we're solving

Before this convention landed, `docs/journals/` mixed brainstorms with
actual journals, filenames didn't say who wrote them, and there was no
consolidation layer for cross-agent synthesis.  Three separate problems
in one chaotic directory.

The fix: **two-tier doc system** (individual + merged) across **three
doc types** (brainstorms, plans, journals).  Each agent writes their
own; periodically someone synthesizes the per-agent inputs into a
merged version that captures consensus.

---

## 2. Directory layout

```
docs/
├── CONVENTIONS.md           (this file)
├── DESIGN_SYSTEM.md         (singleton specs stay at the top)
│
├── brainstorms/
│   ├── claude/              ← Claude's exploration of an idea
│   ├── deepseek/            ← DeepSeek's exploration
│   ├── gemini/              ← (and so on, one dir per agent)
│   └── merged/              ← synthesized across agents
│
├── plans/
│   ├── for-self/            ← agent writing a plan for its own next session
│   │   ├── claude/
│   │   ├── deepseek/
│   │   └── ...
│   ├── for-others/          ← agent writing a plan for a different agent
│   │   ├── for-deepseek/    ← (any agent's plan targeting DeepSeek)
│   │   ├── for-claude/
│   │   └── ...
│   └── merged/              ← consolidated cross-agent roadmap
│
└── journals/
    ├── claude/              ← Claude's end-of-session recap
    ├── deepseek/
    └── merged/              ← daily / weekly consolidated journal
```

**No subdirectories beyond what's listed above.**  If you find
yourself wanting another level, that's a sign the new directory
belongs at the top tier — propose it in a PR.

---

## 3. Filenames

Within any leaf directory, files use the same pattern:

```
YYYY-MM-DD_kebab-case-topic.md
```

* Dates are always ISO 8601 (`2026-05-29`).
* Topic is short, lowercase, kebab-case (`close-workflow-loop`, not
  `Close Workflow Loop` or `workflow_loop_closure`).
* No agent name in the filename — that's captured by the directory.
* No `_merged` suffix — that's captured by the `merged/` directory.

Examples:
* `docs/brainstorms/claude/2026-05-29_workflow-state-machine.md`
* `docs/plans/for-others/for-deepseek/2026-05-29_close-workflow-loop.md`
* `docs/plans/for-self/claude/2026-05-30_tui-polish.md`
* `docs/journals/deepseek/2026-05-28_workflow-storage-shipped.md`
* `docs/brainstorms/merged/2026-05-29_workflow-state-machine.md`

If two different topics from the same author land on the same date,
append `__<disambiguator>` after the topic (rare):

```
2026-05-28_workflow-storage__morning.md
2026-05-28_workflow-storage__evening.md
```

---

## 4. Document header

Every doc starts with a header block that captures authorship,
audience, status, and links to related docs.  Plain markdown — no
YAML front matter (rendering issues in some viewers).

```markdown
# <Title — match the topic slug humanized>

**Author:** Claude
**Audience:** DeepSeek                     ← who's this for? "Self" / agent name / "all"
**Status:** active                         ← draft | active | done | archived
**Related:**
- `../merged/2026-05-29_close-workflow-loop.md` (synthesized into)
- `../../brainstorms/claude/2026-05-26_workflow-design.md` (origin)

---

## <body starts here>
```

* **Author** — the agent (or human) who wrote this version.
* **Audience** — `Self` for personal next-session plans; a specific
  agent name for handoff plans; `all` for merged docs.
* **Status** — `draft` (work in progress), `active` (queued or being
  executed), `done` (executed; superseded by a journal), `archived`
  (out of date but kept for history).
* **Related** — links to upstream brainstorms, downstream merged
  docs, related journals.  Bidirectional when possible.

---

## 5. The merge workflow

**Per-agent docs** are independent — each agent writes their own view.
**Merged docs** consolidate them when a decision is needed or at end
of week.

Rules for merged docs:

1. **First merge author cites all per-agent inputs.**  The header's
   `Related:` block lists every per-agent doc being consolidated, with
   a one-line summary of what each contributed.
2. **Disagreements are surfaced, not hidden.**  If Claude and
   DeepSeek disagreed on approach, the merged doc has a "Open
   disagreements" section that names both positions and (if a
   decision was made) which one won and why.
3. **Merged docs are immutable once status = `active`.**  Updates go
   in a new dated merged doc that supersedes the old one (which
   transitions to `archived`).
4. **Any agent can author a merged doc** — typically the one with the
   most context, or the human (zhiqing) when arbitrating.

---

## 6. Doc type semantics

### Brainstorms
* **Purpose**: explore a problem space *before* committing to an
  approach.  Lots of "what if", trade-off tables, references.
* **When to write**: encountering a design question with > 1 viable
  answer.  Forcing yourself to write it out de-risks the eventual code.
* **When NOT to write**: trivial decisions, well-trodden patterns.
  Just decide and move on.
* **Lifecycle**: `draft` → `active` (while exploring) → `archived`
  (after the design is settled, with link to the resulting plan).

### Plans
* **Purpose**: a concrete, time-boxed work plan for a session.
* **for-self**: you're writing it to remember what to do next time
  you sit down.  Optimised for your own future context.
* **for-others**: you're writing it to hand off to another agent
  cold.  Must be self-contained — assume the reader has zero context.
* **merged**: cross-agent roadmap.  Used when multiple agents are
  working in parallel and need to coordinate (avoid double-shipping,
  align on priorities).
* **Lifecycle**: `draft` → `active` (queued or executing) → `done`
  (executed, link to journal) → `archived`.

### Journals
* **Purpose**: end-of-session recap.  What shipped, what was learned,
  what's deferred.
* **When**: at the end of every working session, even short ones.
  The chain breaks if anyone skips one — the next agent has to
  reconstruct context from commit messages.
* **Required sections**: what shipped (commits + test count delta),
  one surprise or design tension, carry-over for the next session.
* **Lifecycle**: `active` for ~1 day, then `archived` automatically
  (no explicit transition needed).

---

## 7. Migration policy

**Existing docs stay where they are.**  Historical files in
`docs/journals/2026-05-*.md` are not moved — moving them would break
absolute path references in commit messages, prior plans, and the git
history surface.

**From 2026-05-29 onward, every new doc follows this convention.**
An agent encountering a stale-location historical doc may add a
one-line stub at the new location pointing back to the original, but
this is optional polish.

---

## 8. Why this matters

* **Discoverability**: a new agent landing in the repo can list
  `docs/plans/for-self/<their-name>/` to see what's queued for them,
  and `docs/journals/merged/` to catch up on cross-agent state.
* **Attribution**: no more "who wrote this generic-named journal?"
  Every doc's author and audience are obvious from the path.
* **Synthesis**: the `merged/` tier creates a natural place for
  cross-agent consensus without losing the per-agent perspectives.
* **Anti-bias**: separating per-agent brainstorms keeps each agent's
  voice intact before consolidation, instead of one agent's framing
  dominating.

---

## 9. Anti-patterns

- ❌ **No subdirectory beyond what §2 lists.**  The structure is
  intentionally flat-ish to stay scannable.
- ❌ **No author or audience tags in filenames.**  Directories
  capture that; filenames stay short.
- ❌ **No editing of merged docs once `status = active`.**  Write a
  new dated merged doc; old one transitions to `archived`.
- ❌ **No generic filenames** like `development_journal.md` or
  `notes.md`.  Always include a date + topic slug.
- ❌ **No skipping the journal** at end of session.  Breaks the
  chain.

---

## 10. Examples in the wild

The first doc that follows this convention is the migration of
`docs/plans/2026-05-29_deepseek_day.md` to
`docs/plans/for-others/for-deepseek/2026-05-29_close-workflow-loop.md`
(landed in the same commit as this file).  See that file's header
for the canonical example.
