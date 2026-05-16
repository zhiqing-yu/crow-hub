# Brainstorming: TUI UX Improvements (inspired by OpenCode)

**Date:** 2026-05-16
**Author:** DeepSeek + zhiqing
**Status:** Brainstorming

## What OpenCode does well vs crow-hub

| Feature | OpenCode | crow-hub |
|---------|----------|----------|
| Message list | Markdown, syntax-highlighted code blocks | Plain text, ANSI only |
| Input bar | Slash-commands (/model, /file), @-references | Raw text |
| Command palette | Ctrl+P fuzzy search | None |
| Themes | Multiple built-in, runtime switch | Hardcoded colors |
| Loading anim | Animated spinner during LLM calls | Static glyph |
| Keyboard help | Footer shows active shortcuts | None |
| Session mgmt | Save/restore conversations | No UI |
| Multi-panel | Resizable splits, file tree + chat | Fixed 30/70 |
| Status line | Model, tokens, session name in footer | None |

---

## Proposed improvements (ordered by impact/effort)

### P0 — Quick wins (hours)

**1. Animated thinking spinner**
Replace `● yellow` during thinking with braille spinner: ◐ claude 12s…⠋

**2. Keyboard shortcut bar**
Footer: `Tab:switch  Space:multi  Enter:send  Backspace:clear  Ctrl+C:quit`

**3. Agent categories**
Group by status (Online/Offline/Errored) or driver type (Local/WSL/SSH/API)

### P1 — Days

**4. Slash commands**
`/model`, `/clear`, `/session save`, `/theme`, `/help` in the input bar

**5. Theme support**
`Theme` struct, 2-3 built-in themes, `/theme dark|light|monokai`

**6. Better message rendering**
Code block detection, per-agent color coding, truncation indicators

### P2 — Weeks

**7. Session management UI**
Save/load/switch sessions with `/session` commands

**8. Context panel**
Show what files/memory an agent used (toggle with `i`)

**9. Resizable panels**
Ctrl+←/→ to resize agent panel by 5%

**10. Command palette (Ctrl+P)**
Fuzzy-search all actions — OpenCode's signature feature

---

## What NOT to copy

OpenCode = VS Code for 1 person + 1 LLM.
crow-hub = **mission control for a team of agents**.
Skip: file tree, diff viewer, git integration (not core UX here).

---

## Suggested order

```
P0 first (1 week)  → dramatic UX feel improvement, minimal code
P1 next (2 weeks)  → feature parity with OpenCode polish
P2 later           → when the foundation is solid
```

P0 alone would make the TUI feel significantly more polished.
