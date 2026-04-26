# Discussion Journal: Intelligent Text Wrapping & Rendering

**Date & Time**: 2026-04-26 16:58:00 +02:00
**Phase Alignment**: Phase 5 (TUI终端界面)
**Status**: Brainstorming

## Problem Statement
Our custom `wrap_text` function mathematically slices lines by terminal width based on character counts. This approach completely ignores word boundaries and can severely break ANSI escape sequences, causing visual corruption when fixed-width outputs (like ASCII tables or formatted logs) are drawn.

## Proposed Solution
Use established crates to handle wrapping and rich text rendering in the terminal.
- Integrate the `textwrap` crate to break lines intelligently at word boundaries rather than in the middle of words.
- Consider adding a lightweight markdown renderer (such as `termimad` or `tui-markdown`) so that code blocks returned by coding agents have actual syntax highlighting and formatting in the TUI.

## Action Items
- [ ] Add `textwrap` dependency to `ch-tui`.
- [ ] Replace the naive `wrap_text` implementation in `app.rs`.
- [ ] Evaluate markdown rendering options for `ratatui` to support rich agent responses.
