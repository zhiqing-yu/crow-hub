# 2026-05-22 — Project State & Roadmap Sync

**Author:** DeepSeek

## What's been built

### Original ROADMAP (Phases 0–5)
- Phase 0: Scaffolding, CI/CD ✅ Gemini
- Phase 1: Message bus, session, orchestrator ✅ Gemini
- Phase 2: 5 adapters (Claude/Kimi/Gemini/Hermes/CodeBuddy) ✅ Gemini
- Phase 3: SQLite memory persistence, bus integration ✅ Gemini
- Phase 4: Monitor collectors, Prometheus, GPU ✅ Gemini
- Phase 5: TUI with agent list, chat, monitor, multi-agent broadcast ✅ Gemini + Claude

### Claude's 4-day plan (May 12–14)
- Day 1: PR #2 merge, fresh-clone ✅ Claude
- Day 2: Multi-agent broadcast ✅ Claude
- Day 3: SQLite memory writer ✅ Gemini
- Day 4: Token counts in TUI ✅ DeepSeek

### TUI UX polish (May 14–16)
- Animated braille spinner, shortcut footer, agent status summary
- Version number, token suffix before name, 30/70 panel, remove `>` cursor
- Memory browser TUI panel

### Day plan execution (May 19)
- Memory scroll fix, pricing.toml cost ($0.04), parallel agent loading, theme struct

### Design exploration (May 14–22)
- Skill marketplace + agent QQ/Discord
- OpenCode-inspired TUI UX (10 points)
- Reasonix dashboard lessons (tab bar, agent cards, workflow timeline)
- Maestro lessons (handoff, evidence, state-machine workflows)

## Current tests: ~108 lib + 20 ch-tui binary

## Remaining from ROADMAP
- Phase 5: Rich text input, intelligent wrapping ⬜
- Phase 6: GUI (Tauri) ⬜
- Phase 7: Test coverage, security audit, v0.1.0 ⬜

## New directions (from brainstorming)
**Near-term** (Maestro plan): Handoff envelopes → Evidence table → State-machine workflows → Agent principles
**Medium-term** (TUI): Tab bar, slash commands, code highlighting, command palette
**Long-term**: Skill marketplace, agent collaboration OS, GUI dashboard

## Suggested today: Task 1 (Handoff envelopes)
Quickest win, directly improves multi-agent collaboration.
If time, roll into Task 2 (Evidence table).
