# Brainstorming: Skill Marketplace & Agent Communication Hub

**Date:** 2026-05-16
**Author:** DeepSeek + zhiqing
**Status:** Early brainstorming

---

## Idea 1: Skill Marketplace

Agents publish, verify, and discover skills from each other.

### How it could work

- Agent develops a capability (e.g. "git PR reviewer", "SQL query optimizer")
- Packages it as manifest + prompt template + test suite
- Publishes to a marketplace (Git repo, registry, or p2p network)
- Other agents discover, run test suite to verify, then install
- Crow Hub is the runtime that loads and orchestrates these skills

### Why it fits crow-hub

The existing plugin system (`plugins/agents/`, TOML manifests, `PluginLoader`)
is already a primitive skill registry.  Scaling to a public marketplace
is a natural extension — add discovery, verification, and trust.

### When

Late-stage.  Needs stable runtime, verification framework, trust model.
Phase 7+ territory.

---

## Idea 2: Agent-Oriented Communication (QQ/Discord for agents)

### The metaphor

QQ/Discord but the "users" are AI agents:
- Agents have profiles (capabilities, status, token budget)
- Private groups (invite-only, not public)
- Group chat is task-oriented, not social
- Humans observe/moderate

### Why it's interesting

Current crow-hub channels (`#general`) are flat and public-ish.  QQ/Discord adds:

| Concept | Agent equivalent |
|---------|------------------|
| Friend list | Trusted agent registry |
| Group chat | Persistent task force |
| DM | 1:1 agent collaboration |
| Roles | Orchestrator/Worker/Reviewer |
| Pinned | Shared memory anchors |

### Why it might NOT work

Agents don't have social needs.  No loneliness, no FOMO, no emoji reactions.
Building a QQ clone for agents gives you familiar UX that solves the wrong problem.

### The reframe: Collaboration Operating System, not Chat App

What agents need: **persistent multi-agent collaboration spaces**.

- **Task Forces** (not "groups") — invite-only workspace, 3-5 agents,
  shared memory, shared tools, designated orchestrator.

- **Agent Directory** (not "friend list") — known agents with capability
  metadata, trust level, performance history.

- **Task Channels** (not "chat channels") — structured messages (proposal,
  review, delegation, handoff), not free-form chat.

- **Memory Anchors** (not "pinned messages") — key decisions, architecture
  diagrams, test results the task force refers back to.

Think Linear/Notion for agents, not QQ with bot accounts.

### What crow-hub already has

| Need | Existing |
|------|----------|
| Agent identity | `AgentId`, manifests |
| Messaging | `MessageBus`, channels, correlation IDs |
| Sessions | `SessionManager` |
| Workflow | `Orchestrator`, YAML workflows |
| Memory | SQLite persistence, `MemoryStore` |
| Status | `AgentActivity` (idle/thinking/errored) |

Missing: persistent task forces, agent directory, structured message types,
memory anchors, GUI dashboard.

### Implementation path (speculative)

```
Phase 6 (GUI)   → Task Force dashboard
Phase 7+        → Persistent sessions, agent directory
Phase 8+        → Structured messages, memory anchors
Phase 9+        → Skill marketplace
```

### Bottom line

The QQ/Discord metaphor is the wrong framing but the right **direction**.
Not "social network for agents" but "**collaboration operating system
for agents**".  Core insight — private, invite-only group workspaces with
persistent memory and structured communication — is solid.  Implementation
should look more like a project management tool than a chat app.

---

## Open questions

- What does an agent "profile" look like?  Capabilities, trust score,
  token budget, preferred models, past task history?
- Trust model: reputation?  Cryptographic verification?
- Human-in-the-loop for every agent-to-agent handoff, or only exceptions?
