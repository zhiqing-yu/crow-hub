# Brainstorming: Design lessons from Reasonix Dashboard

**Date:** 2026-05-16
**Author:** DeepSeek + zhiqing
**Source:** https://esengine.github.io/DeepSeek-Reasonix/design/agent-dashboard.html

## What Reasonix does well (and what crow-hub can learn)

### 1. Semantic color palette

Every color has fixed meaning. Currently crow-hub is ad-hoc.

**Adopt**: 6-color semantic palette for agent status, focus, errors.

### 2. Tab navigation

Tabs at top: [Dashboard] [Chat] [Agents] [Config] [Memory]

**Adopt**: Replace focus-cycling with a tab bar: [Agents] [Chat] [Monitor] [Memory]

### 3. Agent cards with metadata

Each agent shows: name, model, token summary, capability tags.

**Adopt**: Show model name in dim text below agent name. Add capability tags from manifest.

### 4. Workflow timeline

Steps as vertical timeline with colored status dots.

**Adopt**: Workflow execution view in TUI showing step status, timing, agent assignment.

### 5. List + detail two-panel

Select agent on left → detail on right (status, config, recent messages).

**Adopt**: Scoped chat — right panel shows only selected agent's messages.

### 6. Token block charts

Horizontal bar charts for token usage per agent.

**Adopt**: In Monitor tab, show ████░░ token bars per agent.

### 7. Memory card grid

Memory sessions as cards with entry counts, dates, previews.

**Adopt**: Memory tab browsing SQLite-persisted sessions.

---

## Proposed redesign

```
┌──────────────────────────────────────────────────────┐
│ [Agents]  [Chat]  [Monitor]  [Memory]     Crow v0.1 │  tab bar
├──────────┬───────────────────────────────────────────┤
│ ● Active │  Chat (scoped to selected agent)          │
│ ⠋ 12s…   │                                           │
│ claude   │  claude: I'll refactor the auth module    │
│ ● Ready  │  claude: Here's the updated code          │
│ ◉ 2.1s   │                                           │
│ gemini   │                                           │
├──────────┴───────────────────────────────────────────┤
│ > /model claude-sonnet                                │
├───────────────────────────────────────────────────────┤
│ Tab:switch  /:command  Ctrl+C:quit                    │
└───────────────────────────────────────────────────────┘
```

## Implementation order

P1 → Tab bar + scoped chat
P1 → Color palette standardization
P2 → Workflow execution view
P2 → Memory card grid
P2 → Token bar chart in Monitor

## What NOT to copy

Reasonix is a web dashboard. Skip: SVG charts, hover tooltips, fluid CSS, modals.
Focus on terminal strengths: keyboard nav, dense info, immediate feedback.
