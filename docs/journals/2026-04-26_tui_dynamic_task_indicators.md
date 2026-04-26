# Discussion Journal: Dynamic Task Indicators (Streaming/Thinking)

**Date & Time**: 2026-04-26 16:58:00 +02:00
**Phase Alignment**: Phase 5 (TUI终端界面)
**Status**: Brainstorming

## Problem Statement
When a user sends a prompt, the TUI sits idle while waiting for an agent's `ChatResponse`. Because coding tasks can take several seconds to generate, the static UI leaves the user wondering if the request failed or if the background process has frozen.

## Proposed Solution
Provide non-blocking dynamic visual feedback to the user.
- Add a spinner (`⠋⠙⠹⠸`) or a `[Thinking...]` tag next to the active agent in the left panel.
- Update the spinner on each UI tick while a request is pending.
- This maps perfectly to our `MessageBus` async architecture, giving users immediate confidence that the agent is actively processing their prompt.

## Action Items
- [ ] Add an `is_busy` boolean or state enum to `AgentInfo` in `app.rs`.
- [ ] Listen for "task started" and "task completed" bus events to toggle the busy state.
- [ ] Update the `ratatui` list rendering to draw a spinner for busy agents.
