# 2026-05-17 — Agents panel layout fix + cursor refinement landed

Continues `2026-05-16_memory_browser_cli_and_deepseek_pickup.md`.

Short day.  One render bug found from a user screenshot, fixed in
~20 lines.  Also pushed someone else's `8e31a4b` refinement that had
been sitting local-only since yesterday.

---

## 1. The bug

User screenshot of the running TUI showed the Agents panel with a
mangled first row:

    ○ openclaw-ssh-1eady  0 erred  11 n

Four pieces of text squashed onto one line:

| Substring | Source |
|---|---|
| `openclaw-ssh-1` | first agent name |
| `eady` | tail of "**R**eady" (status-summary label) |
| `0 erred` | summary's errored-count |
| `11 n` | tail of "11 **n**ew" |

Plus: non-cursored agent rows rendered in italic (the summary's italic
style bleeding through), and the latency suffix on the cursored row
was missing.

## 2. Root cause

DeepSeek's `1cecc11` (P0 UX polish) added a one-line status summary
at the top of the Agents panel.  Layout was *intended* to split the
panel's inner area into `[summary | list]` and render each into its
own sub-region.

What actually happened in `crates/ch-tui/src/app.rs:540–550`:

```rust
let agent_chunks = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(2), Constraint::Min(1)].as_ref())
    .split(agents_block.inner(chunks[0]));
f.render_widget(summary_par, agent_chunks[0]);          // ✓ correct
let agents_list = List::new(items).block(agents_block);
f.render_widget(agents_list, chunks[0]);                // ✗ full sidebar, not agent_chunks[1]
```

The List was rendered to **the entire sidebar** (`chunks[0]`) *with
its own block attached* — ratatui repainted the border + title and
started the list content at the block's first inner row, which is
the same row where the summary lived → overlap.  The italic bleed was
a secondary symptom: the summary widget had `Modifier::ITALIC` and
the overlaid list cells inherited that attribute on cells the list
hadn't fully repainted.

## 3. Fix

Commit `193ccff` — `fix(tui): Agents panel — render block once, then
split inner area`.

Five mechanical edits in the same render block:

1. Compute `let inner = agents_block.inner(chunks[0]);` **before**
   moving the block into `render_widget`.
2. Render the block (border + title) once to the full sidebar.
3. Split `inner` (not the block's inner accessor, which would consume
   it again) into `[Length(1) | Min(1)]`.  Reduced 2 → 1 since the
   summary is a single line.
4. Render the summary Paragraph into `agent_chunks[0]` (unchanged).
5. **Drop `.block(agents_block)`** from the List, render to
   `agent_chunks[1]` (the post-summary sub-region).

Net diff: +22 / -5 lines.  No other file touched.

## 4. Pushed someone else's earlier commit too

While checking git state I found `8e31a4b — refine(tui): remove '>'
cursor indicator, use color+bold for selection` sitting local-only
since yesterday.  That's the reason the user's screenshot had no
`>` prefix on the cursored agent — it was an intentional design
change (color + bold suffices as the selection cue), not part of the
bug.  Pushed along with my fix.

## 5. State of the repo

| Metric | Yesterday | Today |
|---|---:|---:|
| Commits on `origin/main` | 7464ce7 | **193ccff** (+2: `8e31a4b`, `193ccff`) |
| Tests passing | 94 | 94 (no test churn — pure layout fix) |
| Open PRs | 0 | 0 |
| Known TUI bugs | Memory scroll inverted (flagged 5/16) + Agents-panel overlap | **Just** Memory scroll inverted |

## 6. Manual verification

After this commit the Agents panel renders as:

```
┌─ Agents  v0.1.0 ─────────────────────┐
│  0 thinking  11 ready  0 erred  0 new │   ← italic, centered, own row
│ ○ openclaw-ssh-1                      │   ← clean, no overlap
│ ○ hermes-ssh-1                        │
│ ○ claude-ssh-1                        │
│ ...                                   │
└───────────────────────────────────────┘
```

Cursored agent gets bold cyan name (no `>` prefix anymore, per the
intentional refinement).  Multi-selected agents still show `[✓]`
column in yellow when Space-toggled.  Latency / token suffix appears
between the glyph and the name (suffix-before-name from DeepSeek's
`9040813`).  Memory panel still cycles in via Tab → Tab → Tab from
the Input panel.  All regression-clean.

## 7. What's left as outstanding visual bugs

- **Memory panel scroll direction is inverted** (flagged 5/16):
  ↑ moves toward NEWER, ↓ toward OLDER — backwards from chat/log
  viewer convention.  Two-line fix in `run_loop`'s Up/Down handlers,
  but no one's picked it up yet.  Worth knocking out next session.

## 8. Reminders for the next agent

Same as 5/16:

- **No spawned-task worktrees in this repo.**  See `2026-05-13_*`
  Section 1 if you accidentally trigger one and Antigravity's chat
  panel goes silent.
- **Push every commit at the end of the session.**  Today I found
  `8e31a4b` had been sitting local-only for a day; easy to drop.
- **Read the previous journal first** (`2026-05-16_*`) for context
  on what's freshly landed and what's still in flight.
