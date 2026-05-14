# Token Counts in TUI — Day 4 Complete

**Date:** 2026-05-14
**Author:** DeepSeek (AI Agent)
**Component:** ch-agent, ch-tui, ch-model

## Overview

Completed Day 4 of Claude's 4-day plan: cumulative token counts surfaced
in the TUI agent list.  This also marks the completion of the full 4-day
plan (Day 1–2 by Claude, Day 3 by Gemini/Antigravity, Day 4 by DeepSeek).

## What shipped (commit `ebd36f4`)

**1. Token extraction from CLI agent output**
- New `SubprocessDriver::extract_usage_from_json()` method probes three
  common JSON shapes:
  - `meta.usage.input` / `meta.usage.output` (OpenClaw)
  - `usage.input_tokens` / `usage.output_tokens` (Anthropic)
  - `usage.prompt_tokens` / `usage.completion_tokens` (OpenAI)
- Wired into `chat()` for Argv mode with JSON output.  Non-JSON agents
  (raw stdout) return zero tokens — no crash.

**2. Cumulative tracking in AgentActivity**
- `AgentActivity::Idle` extended with `cumulative_tokens_in: u64` and
  `cumulative_tokens_out: u64`.  Reset only on agent restart.
- Runtime handler captures `ChatStreamChunk.usage` from the response
  stream, reads the previous activity's cumulative totals, and sums them.

**3. TUI display**
- `render_activity` appends `· 22k/284` suffix when either token count
  is non-zero.  Omits the suffix entirely when both are zero (clean
  display for agents that don't emit counts).

**4. Helper: `format_tokens()`**
- 22279 → "22k", 1500 → "1.5k", 284 → "284", 0 → "".

## Test count

| Before | After |
|---|---:|
| 86 | **103** |

New tests:
- `format_tokens_compact` — edge cases (0, <1k, ≥1k, ≥10k)
- `render_activity_idle_with_tokens` — full suffix "18.6s · 22k/284"
- `render_activity_idle_no_tokens_when_zero` — no suffix when both zero
- Plus the updated `render_activity_idle_with_latency` test

## What's next

Claude's 4-day plan is now complete:

- Day 1 ✅ PR #2 merge + fresh-clone flow
- Day 2 ✅ Multi-agent broadcast
- Day 3 ✅ SQLite memory persistence (Gemini)
- Day 4 ✅ Token counts in TUI (DeepSeek)

Remaining from the original ROADMAP:
- Phase 5: memory browser in TUI
- Phase 6: GUI (Tauri)
- Phase 7: test coverage >80%, security audit, v0.1.0
- Stretch: embeddings for semantic search, parallel agent loading, pricing.toml
