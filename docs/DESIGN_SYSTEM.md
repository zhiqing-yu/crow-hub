# Crow Hub — TUI Design System

> **The single source of truth for every visual decision in the TUI.**
> Any developer or AI agent modifying `ch-tui` MUST follow these rules.
> When in doubt, this document wins over ad-hoc judgement.

---

## 1. Brand Identity

### 1.1 What Crow Hub Is

Crow Hub is a **multi-agent orchestration terminal**. It is a
professional tool for developers who run multiple AI coding agents
(Claude, Gemini, Kimi, OpenClaw, etc.) simultaneously across local
and remote hosts.

### 1.2 Personality

| Trait | Meaning |
|-------|---------|
| **Calm authority** | The UI never shouts. Information is dense but never chaotic. |
| **Instrument, not decoration** | Every pixel earns its place. No ornamental borders, no gratuitous color. |
| **Glanceable** | A user scanning the screen in 0.5 seconds knows: who's thinking, who's idle, who errored. |
| **Progressive disclosure** | L0 (glyphs + color) → L1 (labels + numbers) → L2 (full detail panels). Never force L2 on someone who only needs L0. |

### 1.3 Design References

- `gitui` — footer bar, focus borders, minimal chrome
- `bottom` / `btm` — information density, sparkline rhythm
- `lazygit` — panel focus cycling, modal overlays
- `yazi` — clean glyph vocabulary, muted palette

---

## 2. Color System

### 2.1 The Rule

> **No `Color::*` literal outside `theme.rs`.**
>
> Every color in `app.rs` (or any future rendering file) MUST come
> from a named field on the `Theme` struct. If you need a new color
> intent, add a semantic field to `Theme` first, then use it.

Grep guard (CI can enforce this):
```bash
grep -rn 'Color::' crates/ch-tui/src/ --include='*.rs' | grep -v theme.rs
# Must return ZERO lines (after migration is complete)
```

### 2.2 Palette

The palette is the raw color pool. Only `theme.rs` touches these.

**Default theme** (dark terminal, 16-color safe):

| Role | Color | ANSI | Notes |
|------|-------|------|-------|
| Background | terminal default | — | Never override the user's terminal bg |
| Surface | `Black` | 0 | Tab bar bg, footer bg — one shade darker than terminal |
| Text primary | terminal default fg | — | Chat text, agent names |
| Text muted | `DarkGray` | 8 | Timestamps, suffixes, secondary info |
| Text dim | `Gray` | 7 | Summaries, inactive tabs |
| Accent | `LightBlue` | 12 | Focused border, active tab bg |
| Cursor | `Cyan` | 6 | Current-agent highlight |
| Selection | `Yellow` | 3 | Multi-select glyph + name |
| Status: idle | `Green` | 2 | Agent ready |
| Status: thinking | `Yellow` | 3 | Agent working (also spinner) |
| Status: error | `Red` | 1 | Agent errored |
| Status: unknown | `DarkGray` | 8 | Agent never spoken |

**High-contrast theme** shifts every color one step brighter
(`Green` → `LightGreen`, `DarkGray` → `Gray`, etc.) for
accessibility on light or washed-out terminals.

### 2.3 Semantic Token Naming

Theme fields use this naming convention:

```
{domain}_{state}
```

Examples: `status_idle`, `status_errored`, `agent_cursor`,
`agent_multi`, `border_focused`.

**Never name a token by its color** (`yellow_accent` — NO).
Name it by its **purpose** (`agent_multi` — YES).

### 2.4 Background Rule

> **Never set a panel background color.**
>
> The user's terminal theme owns the background. We only set bg on
> two surfaces: the **tab bar** (1 row, `Surface/Black`) and the
> **footer** (1 row, `Surface/Black`). Everything else is transparent.

---

## 3. Typography Hierarchy

Terminal "typography" = text style (bold, dim, italic) + color intensity.

| Level | Style | Color | Usage |
|-------|-------|-------|-------|
| **L0 — Glance** | Normal | Status glyph color | Glyphs only (●/○/✗/✓/spinner). Readable at arm's length. |
| **L1 — Scan** | Bold | `text_primary` (fg default) | Agent names (when cursor-selected), panel titles, user messages. |
| **L2 — Read** | Normal | `text_primary` | Chat message body, monitor table values. |
| **L3 — Reference** | Normal | `text_muted` (DarkGray) | Timestamps, latency suffixes, token counts, model names, help text. |
| **L4 — Chrome** | Normal | `text_dim` (Gray) | Inactive tab labels, version string, separator lines. |

### 3.1 Emphasis Rules

- **Bold** is reserved for: focused agent name, panel titles, table headers.
- **Italic** is reserved for: summary line in the agents panel.
- **Underline**: never used (renders poorly in many terminals).
- **Reverse/Inverse**: only for the active tab label (`fg:Black bg:Accent`).

---

## 4. Glyph Vocabulary

Every symbol has exactly one meaning. Do not reuse glyphs across contexts.

| Glyph | Meaning | Context |
|-------|---------|---------|
| `●` | Agent idle / ready | Agent list (green) |
| `○` | Agent unknown / never spoken | Agent list (dark gray) |
| `✗` | Agent errored | Agent list (red) |
| `✓` | Agent multi-selected | Agent list (yellow) — replaces status glyph |
| `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` | Agent thinking (braille spinner) | Agent list (yellow), animated |
| `→` | Outbound message (task request) | Memory panel |
| `←` | Inbound message (task response) | Memory panel |
| `⇄` | Handoff envelope | Chat panel, memory panel |
| `📋` | Evidence claim | Chat panel |
| `?` | Evidence pending | CLI evidence list |
| `·` | Neutral / other | Memory panel fallback |
| `─` | Separator line | Monitor table, help sections |
| `┈` | Section header separator | Help output |
| `━` | Heavy separator | CLI memory output header |

### 4.1 Glyph Rules

1. **One glyph = one meaning.** Never use `●` for anything other than "agent idle."
2. **Glyphs carry color.** The glyph's color IS the primary information channel. A color-blind user still gets shape distinction (filled vs hollow vs cross vs check).
3. **No emoji in panels.** `📋` is the one exception (evidence claim). All other indicators use Unicode box/geometric characters for consistent monospace width.
4. **Glyph + space + content.** Always one space between a glyph and the text that follows. Never zero, never two.

---

## 5. Layout Grid

### 5.1 Screen Structure

```
┌──────────────────────────────────────────────────────────┐
│ Tab Bar (1 row)                              version     │  ← Surface bg
├──────────────┬───────────────────────────────────────────┤
│              │                                           │
│  Agents      │  Main Content                             │
│  (30%)       │  (70%)                                    │
│              │  (changes per active tab)                  │
│              │                                           │
│              ├───────────────────────────────────────────┤
│              │  Input (7 rows fixed)                     │
├──────────────┴───────────────────────────────────────────┤
│ Footer (1 row)                                           │  ← Surface bg
└──────────────────────────────────────────────────────────┘
```

### 5.2 Proportion Rules

| Element | Constraint | Rationale |
|---------|-----------|-----------|
| Tab bar | `Length(1)` | Single row, never grows |
| Footer | `Length(1)` | Single row, always visible |
| Left panel (Agents) | `Percentage(30)` | Enough for agent name + suffix |
| Right panel (Content) | `Percentage(70)` | Primary workspace |
| Input box | `Length(7)` | Room for multi-line input |
| Content area | `Min(3)` | Everything else |

### 5.3 The Agents Panel is Always Visible

> The left agents sidebar is **never hidden**, regardless of which tab
> is active. It is the persistent anchor of the interface — the user
> always knows who's online.

The tab bar controls what the **right content area** shows:
- **Agents tab** → Chat view (default operating mode, focus on sidebar)
- **Chat tab** → Chat view (focus on chat for scrolling)
- **Monitor tab** → Agent stats dashboard
- **Memory tab** → Persisted message history

---

## 6. Panel & Border Rules

### 6.1 Focus Indication

> **One and only one panel has focus at a time.**
> The focused panel gets `border_focused` color (Accent).
> All other panels get the terminal's default dim border.

The tab bar highlight and the border highlight must **always agree**.
If the tab says "Chat" but the border highlights "Agents," the UI is broken.

### 6.2 Border Style

- All panels use `Borders::ALL` (full box).
- Border characters: default ratatui box-drawing (`┌─┐│└─┘`).
- No double borders. No rounded borders (inconsistent cross-terminal).
- Panel titles sit in the top border: `Block::default().title("Name")`.

### 6.3 Titles

Panel titles are **plain nouns or noun phrases**:
- `"Agents"` — not "Agent List" or "🤖 Agents"
- `"Channel: #general"` — descriptive, not decorative
- `"Monitor — Agent Activity"` — em-dash for subtitle
- `"Input (Press Tab to switch focus)"` — parenthetical hint, remove once footer exists

---

## 7. Interaction States

### 7.1 Agent List States

Each agent row has exactly one visual state at a time:

| State | Glyph | Name style | Triggered by |
|-------|-------|-----------|-------------|
| **Default** | Status glyph (color by status) | Normal, default fg | — |
| **Cursor** | Status glyph (color by status) | **Bold**, `agent_cursor` color | ↑↓ navigation |
| **Multi-selected** | `✓` (`agent_multi` color) | `agent_multi` color | Space toggle |
| **Multi-selected + Cursor** | `✓` (`agent_multi` color) | **Bold**, `agent_multi` color | Cursor on a selected agent |

Priority: multi-selected appearance wins over cursor appearance.
The cursor is visible via bold; the ✓ glyph is the primary selection indicator.

### 7.2 Focus Cycling

`Tab` key cycles: **Agents → Chat → Input** (3 stops for Chat/Monitor views,
or Agents → Memory → Input when Memory tab is active).

`Shift+Tab` reverses.

The tab bar switches the **view** (what content is shown).
Focus cycling moves between **panels within** the current view.

### 7.3 Scrolling

- Chat panel: `↑↓` when focused, mouse wheel always.
  Scroll offset 0 = latest message at bottom.
  Sending a message resets scroll to 0 (jump to bottom).
- Memory panel: `↑↓` when focused, `r` to refresh.
- Input panel: `↑↓` for scroll when content overflows.

---

## 8. Information Density

### 8.1 The Suffix Pattern

Agent rows use a **suffix-before-name** pattern:

```
●780ms claude-ssh-1
```

Not `●claude-ssh-1 780ms`. The suffix (latency, token counts, cost)
sits between the glyph and the name because:
1. The suffix is fixed-width-ish, keeping names aligned.
2. Scanning down the left edge gives status (glyph) → performance (suffix) → identity (name).

### 8.2 Number Formatting

| Value | Format | Example |
|-------|--------|---------|
| Latency < 1s | `{ms}ms` | `780ms` |
| Latency 1-60s | `{s:.1}s` | `2.1s` |
| Latency > 60s | `{m}m{s}s` | `4m12s` |
| Tokens < 1000 | raw number | `284` |
| Tokens 1k-10k | `{:.1}k` | `1.5k` |
| Tokens > 10k | `{n}k` | `22k` |
| Cost | `${:.2}` | `$0.12` |
| Zero / absent | `—` (em-dash) | `—` |

### 8.3 Monitor Table

The Monitor tab shows a tabular view. Column order:

```
Glyph  Agent (left-aligned)  Status  Latency  Tok In  Tok Out  Cost
```

- Header row: bold, `text_muted` color
- Separator: `─` repeated to panel width, `text_muted`
- Data rows: glyph colored by status, values in `text_muted`
- Optional sub-row: `  └ model: <name>` in `text_muted`

---

## 9. Tab Bar

### 9.1 Structure

```
 Agents  Chat  Monitor  Memory                    v0.1.0
```

- Active tab: **inverse** (`fg:Black bg:Accent`, bold)
- Inactive tabs: `text_dim` (Gray)
- Version: right-aligned, `text_muted` (DarkGray)
- Background: `Surface` (Black)

### 9.2 Tab ≠ Focus

The tab bar selects the **view mode** (what content fills the right panel).
Focus selects **which panel receives keystrokes**.
These are related but not identical:
- Switching to "Agents" tab auto-focuses the Agents panel.
- Switching to "Chat" tab auto-focuses the Input panel.
- Within a tab, the user can still cycle focus between panels.

---

## 10. Footer

### 10.1 Structure (3 segments)

```
[ context-sensitive keys ]  │  [ active target ]  │  [ session stats ]
```

- Left: keyboard hints for the focused panel
- Center: `You → agent-name` or `You → [3 selected]`
- Right: session-level stats (pending evidence count, etc.)

### 10.2 Key Hint Formatting

Keys in **bold/emphasis**, labels in **muted**:

```
Tab switch  Space select  Enter send  ? help  Ctrl+C quit
```

Hints change based on focused panel. Max 5 hints to prevent overflow.

---

## 11. Modal Overlays

### 11.1 General Rules

- Modals are centered, ~60% width × 60% height.
- Background: `Surface` with `Accent` border.
- Title in the top border: `" Help — ? to close "`.
- Content is scrollable if it overflows.
- `Esc` or the trigger key (e.g., `?`) closes the modal.
- Modals are drawn **last** in the render pass, floating above all panels.

### 11.2 The Help Modal (`?`)

Pressing `?` (when not typing in Input) opens a centered help overlay
showing all keyboard shortcuts and slash commands. This replaces the
need to read `/help` output in the chat stream.

---

## 12. Animation

### 12.1 Spinner

Braille spinner frames: `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏`

- Advance every 3 ticks (tick rate = 250ms → spinner cycle ≈ 7.5s).
- Used only for "Thinking" state.
- Color: `status_thinking` (Yellow).

### 12.2 No Other Animations (For Now)

No fade-ins, no slide transitions, no cursor blink override.
Terminal animation is fragile across environments.
The spinner is the only animated element until toast notifications are implemented.

---

## 13. Anti-Patterns (DO NOT)

| Rule | Why |
|------|-----|
| **No `Color::*` outside `theme.rs`** | Breaks theme switching, causes visual inconsistency |
| **No emoji in structural UI** | Inconsistent width across terminals. `📋` is grandfathered for evidence only |
| **No panel background colors** | Respect the user's terminal theme |
| **No underline** | Renders as box in some terminals |
| **No bright-on-bright** | `LightYellow` on `Yellow` is unreadable |
| **No absolute pixel positioning** | Use ratatui Layout constraints only |
| **No hardcoded strings for repeated patterns** | Use format helpers (`format_latency`, `format_tokens`) |
| **No redundant indicators** | One visual signal per state. Don't show `[✓]` AND a colored glyph for the same thing |
| **No information in titles only** | Titles are chrome. Data lives in the panel body |
| **No lorem ipsum / placeholder panels** | Every tab/panel must show real data. If there's no data, show an empty-state message |

---

## 14. Adding a New Panel / View

Checklist for any new panel:

1. [ ] Add a semantic `Tab` variant if it's a new tab view
2. [ ] Add all new colors as named fields on `Theme` (both default + hc variants)
3. [ ] Use `block.border_style(theme.focused_border(...))` for focus
4. [ ] Follow the layout grid (left 30% / right 70%)
5. [ ] Add context-sensitive footer hints for the new panel's focused state
6. [ ] Define the empty state (what shows when there's no data)
7. [ ] Add at least one test for any new pure logic (formatting, filtering)
8. [ ] Update the help lines (`help_lines()` + `SUPPORTED_COMMANDS` if adding slash commands)

---

## 15. File Ownership

| File | Owns |
|------|------|
| `theme.rs` | All color definitions, palette, token struct, theme constructors |
| `app.rs` | Layout, rendering, input handling, app state |
| `main.rs` | CLI subcommands, runtime wiring (no rendering logic) |

Rendering logic (anything that calls `f.render_widget`) lives in `app.rs` only.
If `app.rs` grows past ~1500 lines, extract panel renderers into `panels/` submodules
that receive `&App` + `&mut Frame` + `Rect` — but the design system rules still apply.

---

*Last updated: 2026-05-25. Revision 1.*
