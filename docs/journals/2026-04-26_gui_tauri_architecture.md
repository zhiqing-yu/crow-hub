# Discussion Journal: GUI Tauri Architecture & IPC Bridge

**Date & Time**: 2026-04-26 16:58:00 +02:00
**Phase Alignment**: Phase 6 (GUI图形界面)
**Status**: Brainstorming

## Problem Statement
Looking ahead to Phase 6 (Week 18-22), we need to ensure that the heavy lifting done by the `MessageBus` and `AgentRuntime` in the Rust core is seamlessly exposed to the React/Vue frontend without duplicating logic or causing state desynchronization. Additionally, MCP tools (Model Context Protocol) require a different UI treatment than standard conversational agents.

## Proposed Solution
- **Unified Event Bridge**: Build a robust IPC (Inter-Process Communication) bridge in `crates/ch-gui` using Tauri commands and events. Instead of writing custom API endpoints, we should directly stream our `AgentMessage` bus events to the WebView via Tauri's `emit` system.
- **MCP Separation**: MCP servers (like `tiny-agents`) should not be mixed into the standard "Agent Chat" roster in the frontend. Instead, they should have a dedicated "Capabilities / Tool Registry" panel where users can visualize what tools are connected and available to the conversational agents.

## Action Items
- [ ] Draft the Tauri IPC layer for the Message Bus in `crates/ch-gui`.
- [ ] Design the frontend React state layer to consume Tauri events efficiently.
- [ ] Plan the UI layout to cleanly separate Conversational Agents (chat list) from MCP Tools (resource registry).
