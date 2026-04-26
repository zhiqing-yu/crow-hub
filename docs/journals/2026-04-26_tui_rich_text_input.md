# Discussion Journal: Rich Text Input (Multi-line & Cursor Movement)

**Date & Time**: 2026-04-26 16:58:00 +02:00
**Phase Alignment**: Phase 5 (TUI终端界面)
**Status**: Brainstorming

## Problem Statement
The current input box in the TUI is a flat `String` bound to a naive `Paragraph` widget. Pressing `Enter` fires the message immediately. There is no support for cursor movement (Left/Right/Home/End) or text selection. Additionally, pasting code blocks (even with bracketed paste enabled) strips all newlines, flattening the code into a single hard-to-read line.

## Proposed Solution
Integrate a robust text-input crate (such as `tui-textarea`) into `crates/ch-tui`.
- Replaces the single string input with a fully functional editor state.
- Enables multi-line prompts (e.g., `Shift+Enter` or `Alt+Enter` for a new line, `Enter` to send).
- Preserves newlines when users paste code blocks, which is critical for an AI coding assistant hub.

## Action Items
- [ ] Evaluate `tui-textarea` and add it to `crates/ch-tui/Cargo.toml`.
- [ ] Replace `app.input` string with the text-area state.
- [ ] Update `run_loop` event handling to pipe key events to the text area widget.
- [ ] Modify bracketed paste logic to retain `\n` characters.
