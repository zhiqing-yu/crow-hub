# Plan: TUI polish — semantic tokens, focus indicators, footer, help modal

> **Audience**: Any next agent (Claude, DeepSeek, or zhiqing directly).
> Self-contained — no prior conversation context needed.
>
> **Not tied to a specific date.**  Can be started today (2026-05-24)
> or any day; independent of the parallel verifier plan at
> `docs/plans/2026-05-25_deepseek_day.md`.  Either can land first; no
> shared files of consequence.

---

## Why this exists

The architecture work is outpacing the UX work.  We have Handoff
Envelopes, an Evidence table, a memory writer, a memory CLI, scoped
chat, multi-select, tabs, themes — all functional, all merely
*utilitarian* on screen.  Compared to apps like `gitui`, `bottom`,
`yazi`, or `atuin`, the crow-hub TUI today is information-rich but
visually flat: scattered hex colors, a `/help` that clogs the chat
panel, no persistent keybinding hints, no visible focus indicator
beyond title-color, no animation beyond a basic spinner.

This plan ships **four cheap, high-impact polish items** plus one
stretch.  Foundation first (semantic tokens), then everything else
benefits.

References (the design philosophy informing this plan):
- [`The Terminal Renaissance`](https://dev.to/hyperb1iss/the-terminal-renaissance-designing-beautiful-tuis-in-the-age-of-ai-24do)
  — semantic color, progressive disclosure (L0/L1/L2), spatial consistency
- `gitui` for the footer-bar pattern
- `bottom` for sparkline density tricks (out of scope today; future)
- Tokens approach: palette → tokens → styles → composed

---

## Where we are

Latest commits on `main`:

```
98d9247 docs(journal): 2026-05-24 — Evidence table (Maestro Task 2)
bf4a71a feat(tui): /evidence claim slash command
fa6206d feat(memory-cli): crow memory evidence subcommand
... (Maestro Task 2 stack)
```

Plus a non-committed/recent `app.rs` refactor that makes the memory
refresh non-blocking via `pending_memory: Arc<Mutex<Option<…>>>`.
That refactor follows one of the design principles ("async
everything, never freeze the UI") so it's net-positive — keep it.

**Test count**: 128 lib + 25 ch-tui binary = **153 total**, all green.

---

## Today's scope — four polish items + one stretch

| # | Task | Effort | Foundation? |
|---|------|-------:|:-----------:|
| 1 | Semantic Theme tokens (palette → tokens → helpers) | ~1.5 hr | ✅ unblocks 2-5 |
| 2 | Focused-panel accent border | ~30 min | depends on #1 |
| 3 | Persistent footer status bar | ~1.5 hr | depends on #1 |
| 4 | `?` modal overlay for full help | ~1.5 hr | depends on #1, #3 |
| 5 | (Stretch) Notification toast for handoff/evidence | ~1.5 hr | depends on #1 |

Total realistic: **~5 hrs** (drop #5 if time-boxed).

Sequence is intentional: #1 first so the rest reference tokens, not
hex codes.

---

## Pre-flight (5 min)

```bash
cd <repo>
git fetch origin
git checkout main
git pull origin main
git status                            # must be clean (or has the pending_memory app.rs change)
cargo test --workspace --lib          # 128 passing
cargo test -p ch-tui --bin crow       # 25 passing
```

Stop and report if anything fails.

---

## Task 1 — Semantic Theme tokens (~1.5 hr) — FOUNDATION

**Current state** (`crates/ch-tui/src/theme.rs`): `Theme` is a small
struct with a `name` field and a handful of named colors (`fg`, `bg`,
maybe `accent`).  Color usage is inconsistent — some sites style via
`Color::Yellow` literals, others via `theme.accent`.

**Goal**: split styling into three layers:

```
palette   (hex / named colors)     → 16-color baseline, optional truecolor
tokens    (semantic names)         → text.muted, status.pending, focus.border, ...
helpers   (Style constructors)     → theme.text_muted() returns ratatui::Style
```

Site code calls `theme.text_muted()`, never `Color::Yellow`.

### Token vocabulary (first cut)

```rust
pub struct ThemeTokens {
    // Text intensities
    pub text_primary:   Style,   // default chat text
    pub text_muted:     Style,   // timestamps, hints, ts in footer
    pub text_emphasis:  Style,   // agent names, bold inline

    // Backgrounds (3 depth layers for overlays)
    pub bg_base:        Style,   // app background
    pub bg_surface:     Style,   // panel background
    pub bg_overlay:     Style,   // modal background

    // Accents per feature
    pub accent_primary:  Style,  // active tab, headlines
    pub accent_handoff:  Style,  // ⇄ glyph + handoff lines
    pub accent_evidence: Style,  // 📋 glyph + evidence lines

    // Status (semantic, not domain-specific)
    pub status_pending:  Style,  // ? glyph, pending count
    pub status_verified: Style,  // ✓ glyph, success states
    pub status_failed:   Style,  // ✗ glyph, error states
    pub status_warning:  Style,  // yellow caution states
    pub status_info:     Style,  // ─ separators, neutral notices

    // Focus
    pub focus_border_active:   Style,  // bright border on focused panel
    pub focus_border_inactive: Style,  // dim border elsewhere
}
```

### Construction

`Theme::default()` and the high-contrast variant
(`CROW_THEME=hc`) BOTH return a `ThemeTokens` populated from their
own palettes.  Helpers:

```rust
impl Theme {
    pub fn text_muted(&self) -> Style { self.tokens.text_muted }
    pub fn focused_border(&self, focused: bool) -> Style {
        if focused { self.tokens.focus_border_active }
        else { self.tokens.focus_border_inactive }
    }
    pub fn glyph_for_status(&self, status: EvidenceStatus) -> (&'static str, Style) {
        match status {
            Pending  => ("?", self.tokens.status_pending),
            Verified => ("✓", self.tokens.status_verified),
            Failed   => ("✗", self.tokens.status_failed),
        }
    }
    // ...
}
```

### Migration

Site-by-site grep-replace.  Targets:
- `crates/ch-tui/src/app.rs` — every `Color::*` literal not already
  going through theme
- `crates/ch-tui/src/main.rs::run_memory_evidence` — glyph + color choices
- `crates/ch-tui/src/main.rs::run_memory_tail` — same

Don't try to be exhaustive in one pass.  Migrate the chat panel + tab
bar + memory panel + footer (added in #3) + help modal (added in
#4).  Leave less-trafficked sites with TODO comments for later.

### Tests (~3 in `crates/ch-tui/src/theme.rs::tests`)

- `default_theme_populates_every_token`
- `hc_theme_populates_every_token`
- `glyph_for_status_returns_distinct_glyph_per_variant`

The first two assert presence (no token left at `Style::default()`).

**Commit**: `refactor(tui): semantic Theme tokens (palette → tokens → helpers)`

---

## Task 2 — Focused-panel accent border (~30 min)

**Current**: `FocusedPanel` cycles through Agents/Chat/Memory/Input
on Tab, but the visual difference is *just* the title text style.
Easy to miss which panel has focus.

**Fix**: every panel's `Block::default().borders(Borders::ALL)` call
sets a `border_style` derived from the focus state.  With tokens
from #1:

```rust
let block = Block::default()
    .borders(Borders::ALL)
    .border_style(theme.focused_border(focused_panel == FocusedPanel::Chat))
    .title("Chat");
```

The focused panel gets `focus_border_active` (e.g. bright cyan in
default theme, bright white in hc theme); others get
`focus_border_inactive` (e.g. dim gray).

### Acceptance

- Cycle through panels with Tab — the bright-bordered panel
  visibly moves.  Title style change becomes redundant (can stay or
  go).

### Tests

None needed — pure visual change, no testable predicate beyond
"compiles."  Add a comment in the code referencing this plan.

**Commit**: `feat(tui): bright accent border on focused panel`

---

## Task 3 — Persistent footer status bar (~1.5 hr)

**The gap**: `/help` dumps into the chat panel, scrolling other
messages off-screen.  New users have no idea Tab cycles panels, Space
multi-selects, etc.  Discovery is broken.

**Fix**: reserve the bottom row of the terminal for a status bar
that's *always visible*.  Contents (3 segments, separated by ` │ `):

```
[ context-sensitive keys ]  │  [ active agent / multi-select ]  │  [ session stats ]
```

### Layout integration

In the main `Layout`, before splitting into tabs / panels, peel one
row off the bottom:

```rust
let chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([
        Constraint::Length(1),  // tab bar (existing)
        Constraint::Min(0),     // tabs / panels
        Constraint::Length(1),  // footer (NEW)
    ])
    .split(area);
```

### Context-sensitive keys

A free function `footer_keys(panel: FocusedPanel, has_multi: bool)
-> Vec<(&str, &str)>` returns 3-5 key/label pairs per state:

| Panel    | Footer hints |
|----------|--------------|
| Agents   | `Tab` switch · `Space` multi-select · `Enter` send · `?` help · `Ctrl+C` quit |
| Chat     | `Tab` switch · `PgUp/PgDn` scroll · `/all` global · `?` help · `Ctrl+C` quit |
| Memory   | `Tab` switch · `↑↓` scroll · `?` help · `Ctrl+C` quit |
| Input    | `Enter` send · `Tab` switch · `/help` cmds · `?` help · `Ctrl+C` quit |

Rendered with `text_emphasis` on keys and `text_muted` on labels.

### Middle segment

`"You → <agent>"` or `"You → [<n> selected]"` based on
`multi_selected` / `selected_agent`.

### Right segment

`"3 pending · 12 verified"` from a quick `pending(50)` +
`by_status(Verified, 50)` call.  Refresh once per second via
tick_count to avoid hitting SQLite per frame.

To keep it cheap: store the counts on `App` (`pending_evidence:
usize`, `verified_evidence: usize`), spawn a background refresh
(mirror the `pending_memory` pattern that just landed for memory
rows), update on tick.

### Tests (~2 in `app.rs::tests`)

- `footer_keys_for_agents_panel_includes_space`
- `footer_keys_for_chat_panel_includes_pgup`

**Commit**: `feat(tui): persistent footer status bar with context-sensitive keys`

---

## Task 4 — `?` modal overlay (~1.5 hr)

**Current**: `/help` outputs into chat, mixed with messages.  User
loses scroll position; help text scrolls away as new messages arrive.

**Fix**: pressing `?` (anywhere, any focused panel) opens a centered
modal overlay showing the full `help_lines()` content.  Esc or `?`
again closes.  Modal is drawn last so it visually floats above
everything.

### State

Add to `App`:

```rust
pub show_help_overlay: bool,
```

Initialise to `false`.

### Input handling

In the main key handler (around the existing `KeyCode::Char` match
arm), add **before** the panel-specific handlers so `?` works
globally:

```rust
KeyCode::Char('?') if !app.input_has_focus_for_typing() => {
    app.show_help_overlay = !app.show_help_overlay;
}
KeyCode::Esc if app.show_help_overlay => {
    app.show_help_overlay = false;
}
```

`input_has_focus_for_typing()` returns true only when the Input panel
is focused AND the user is mid-typing — so `?` typed into a chat
message doesn't open the modal.  For first ship, just check
`focused_panel == FocusedPanel::Input`.

### Rendering

After all normal panel drawing, if `show_help_overlay`:

1. Compute a centered rectangle (e.g. 60% width, 60% height).
2. Render a `Clear` widget to blank the area.
3. Render a `Block` with `bg_overlay` background and `accent_primary`
   border, title `" Help — ? to close "`.
4. Inside, render `help_lines()` as a `List` or `Paragraph`.

A small `fn centered_rect(pct_x: u16, pct_y: u16, area: Rect) -> Rect`
helper covers the math.  See `ratatui` examples for the standard
pattern.

### Keep `/help` for backward compat

Don't remove the `/help` slash command — keep it as the
chat-output variant some users may prefer.  Document `?` in the
`SUPPORTED_COMMANDS` regression test... actually no, `?` isn't a
slash command.  Just add a comment in `app.rs` near where `?` is
handled saying "see also `/help` slash command".

### Tests (~2 in `app.rs::tests`)

- `centered_rect_returns_smaller_centered_rect` (math sanity)
- `pressing_question_toggles_help_overlay` — pure helper test:
  extract the toggle logic into a small fn `toggle_help_overlay(&mut
  bool)` and test that

**Commit**: `feat(tui): ? modal overlay for full help (preserves /help slash)`

---

## Task 5 — (Stretch) Notification toast for handoff/evidence (~1.5 hr)

If items 1-4 land early.

**Idea**: when an event of interest fires (handoff received,
evidence verified/failed), a transient pill appears bottom-right and
fades after 3 seconds.  Doesn't displace chat; just floats.

### State

```rust
pub struct Toast {
    pub text: String,
    pub style_kind: ToastKind,   // Info | Success | Warning | Error
    pub created_at_tick: u64,
}
pub toasts: Vec<Toast>,
```

### Producers

Wherever we currently push a message into the chat with a `⇄` or
`📋` prefix (handoff received from remote agent, evidence
verified by verifier), also push a Toast.

### Render

After all normal drawing AND after the help overlay (so the help
overlay covers toasts):

- Filter `toasts` to remove entries older than 3 seconds (using
  tick_count).
- Render each remaining toast as a small pill, stacked
  bottom-right.  Use `bg_overlay` + status-color border.

### Tests (~2)

- `toast_expires_after_threshold_ticks`
- `toast_for_evidence_verified_uses_status_verified_style`

**Commit**: `feat(tui): transient toast notifications for handoff + evidence events`

---

## Out of scope (deliberately, save for future plans)

- **"Agents" management tab** — register / unregister / configure
  CLI clients (a CRUD view on `~/.crow-hub/plugins/*.toml`).  Tab
  vocabulary is reserved (renamed today's dashboard tab to "Home"
  to free up the name).  Future tab would add a 5th `Tab::Agents`
  variant with an editor view; non-trivial because it needs file IO,
  validation, and a live-reload signal to the runtime.  Likely a
  whole-session task on its own.
- **Markdown rendering in chat** — `**bold**`, inline `` `code` ``,
  fenced code blocks.  Useful (most LLM output is markdown) but a
  proper job needs `pulldown-cmark` or similar + careful span
  composition.  Whole-session task.
- **Sparklines in the Monitor panel** — token-rate per agent over
  the last N ticks.  Needs a rolling buffer + `ratatui::Sparkline`
  widget integration.  Whole-session task.
- **Mouse-hover effects** on agents list — possible since we
  already capture mouse, but needs hit-testing.
- **Smooth scrolling** in chat — currently jumpy.  Needs a
  virtual-scroll-offset that interpolates per tick.
- **Theme hot-reload** from a `theme.toml` file — depends on tokens
  landing first (#1), then a watcher.

---

## General conventions (unchanged)

| Topic | Convention |
|---|---|
| Commits | Imperative + scoped (`feat(tui): ...`, `refactor(tui): ...`) |
| Tests | Every behavior change adds ≥1 test |
| CI gate | `cargo test --workspace --lib` must pass (baseline: **128**) |
| Style | `cargo fmt --all` before commit |
| Branches | Direct to `main` for small commits |

## What to AVOID

- ❌ **No spawned-task git worktrees.**  Standing rule since
  2026-05-13.
- ❌ **No force-pushes to `main`.**
- ❌ **No hex color literals outside `theme.rs`.**  After Task 1
  lands, every color choice must go through a token.  Grep guard:
  `grep -rn 'Color::Rgb\|Color::Indexed' crates/ch-tui/src/` should
  only hit `theme.rs`.
- ❌ **Don't replace `/help`.**  Add `?` modal as a parallel option;
  keep the slash command working.
- ❌ **No new top-level workspace deps** unless absolutely
  unavoidable.  Everything in this plan is doable with ratatui +
  what's already in the workspace.

## Reporting back

End-of-day journal at `docs/journals/<date>_tui_polish.md` continuing
the chain.  Cover:

- What shipped (commits + test count delta from 153)
- Which items landed vs. carried over
- One visual-before/after observation (e.g. "footer immediately made
  Tab discoverable")
- Any surprises in ratatui's API or layout math

If you only ship Task 1 + Task 4, that's a strong day — tokens unblock
everything and the help modal is the most user-visible single change.
The footer (Task 3) can carry over.

---

## Coordination with the verifier plan

The other plan at `docs/plans/2026-05-25_deepseek_day.md` is the
**verifier audit loop** (Maestro Task 2's closing half).  It's
backend-heavy: a new `ch-memory::verifier` module, polling
`EvidenceStore::pending`, emitting `EvidenceVerify` messages.

**No file conflicts** with this plan:
- Verifier touches `ch-memory/src/{lib,verifier,writer}.rs`, plus
  two `spawn_verifier` call sites in `ch-tui/src/main.rs` (its
  `run_tui` and `run_server`).
- TUI polish touches `ch-tui/src/{theme,app}.rs` plus rendering in
  `ch-tui/src/app.rs` panel-render call sites.

Pick whichever feels more energising; both can land in either order.
If the verifier ships first, this plan gets a free "pending evidence
count" data source for the footer's right segment.  If this plan
ships first, the verifier's toast notifications (when added) get
proper token-styled rendering.
