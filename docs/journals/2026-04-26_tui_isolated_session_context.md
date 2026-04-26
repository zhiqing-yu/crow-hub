# Discussion Journal: Isolated Session Context (Per-Agent Histories)

**Date & Time**: 2026-04-26 16:58:00 +02:00
**Phase Alignment**: Phase 5 (TUI终端界面)
**Status**: Brainstorming

## Problem Statement
Currently, all agents in the TUI dump their messages into a single, global `#general` channel vector. When a user switches between agents in the left panel, the chat history in the right panel remains the same, mixing context from completely different agents.

## Proposed Solution
Refactor the state management in `crates/ch-tui/src/app.rs` to maintain independent message histories.
- Map chat history to `HashMap<AgentId, Vec<Message>>`.
- When an agent is selected in the left panel, the right panel instantly loads that specific context.
- This aligns perfectly with the architecture defined in `ch-core::session` and allows users to run parallel, independent tasks without visual clutter.

## Action Items
- [ ] Implement `HashMap` for per-agent chat state in `App` struct.
- [ ] Update the message receiving loop to route messages to the correct vector based on the sender's `AgentId`.
- [ ] Add visual cues in the left panel to show which agents have unread messages.
