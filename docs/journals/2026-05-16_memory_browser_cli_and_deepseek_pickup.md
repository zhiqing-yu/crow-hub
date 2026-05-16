# 2026-05-16 — Memory browser CLI + handoff back from DeepSeek

Continues `2026-05-13_multi_agent_broadcast_and_antigravity_unblock.md`.

DeepSeek took the controls May 14–15 and shipped Days 3 & 4 of the
4-day plan plus heavy polish.  Today I picked the work back up, pushed
DeepSeek's 17 local-only commits to GitHub, and added the first CLI
window into the persisted memory store.

---

## 1. Caught up on DeepSeek's two days

What landed while I was away (commits May 14 → today AM):

**Day 3 — SQLite memory persistence** (`afec1ce`, `16fc90b`)
- Landed the other agent's `get_home_dir` helper that had been sitting
  as WIP — paths now go to `~/.crow-hub/messages.db` etc.
- Background writer (`crates/ch-memory/src/writer.rs`) subscribes to
  the bus, writes every TaskRequest/TaskResponse to SQLite.
- Schema has `correlation_id`, `from_agent`, `to_agent`, `channel`,
  `message_type`, `content`, `created_at` + indexes.

**Day 4 — Token counts in the TUI** (`ebd36f4` then 3 iterations:
`bb8ac19`, `fe14cc6`, `03b8820`)
- `AgentActivity::Idle` gains `cumulative_tokens_in/out`
- 3-layer extraction: JSON usage shapes → Claude stderr regex →
  char-count heuristic fallback (~4 chars/token ASCII, ~1.5 CJK)
- Critical iteration #3 fix: TUI goes through `stream_chat()`, not
  `chat()` — earlier token extraction was dead code on the wrong path.
  Now `stream_chat` tracks output via `Arc<AtomicU64>` and chains a
  final chunk with estimated counts.
- TUI shows `· 22k/284` suffix per agent, compact

**TUI polish** (`2a0b50d`, `cf2185f`, `9040813`, `1cecc11`)
- Version number in Agents panel title
- Format compaction: `18.6s·22k/284` (no extra spaces)
- Reorder spans: `[●] [suffix] [name]` so wide token text never clips
  the agent name
- P0 UX polish (animated spinner via `tick_count`, keyboard shortcut
  footer, status summary)
- Agents panel widened 25% → 30% → 32%

**Three brainstorming docs**, all today, all preserved:
- `2026-05-16_brainstorming_reasonix_design_lessons.md` — semantic
  palette, tab nav, two-panel scoped chat, token bar charts
- `2026-05-16_brainstorming_tui_ux_inspired_by_opencode.md` — slash
  commands, themes, animated spinner (P0 done), keyboard help (P0 done)
- `2026-05-16_brainstorming_skill_marketplace_and_agent_communication.md` —
  Skill marketplace + QQ/Discord-for-agents metaphor

**Test count**: 86 → **111** (DeepSeek added 25 new tests across
token extraction, char estimation, Gemini/Claude usage shapes).

DeepSeek's recap: `docs/journals/2026-05-16_deepseek_session_recap.md`.

I pushed all 17 of their local commits to `origin/main` in one push —
`28034b1..0d269b2`.

---

## 2. Today's small ship: `crow memory tail` / `count`

The memory writer has been running and (presumably) persisting messages
for two days, but there was no way to actually *see* what's stored short
of opening the SQLite file with a third-party client.  Added two
read-only CLI subcommands so the data plane is inspectable from the
shell that built it.

```
$ crow memory count
Stored messages: 0

$ crow memory tail -n 20 -c general
━━━ crow memory tail — channel: general, last 0 of 20 ━━━
(no messages persisted yet for channel 'general')
Tip: run the TUI (`crow`) and chat with an agent; messages
are written to ~/.crow-hub/messages.db by the bus subscriber.
```

Once the user runs the TUI and chats, rows look like:

```
05-16 22:14:33  →  You                     hi
05-16 22:14:35  ←  claude-wsl-ubuntu       Hello! What can I help you with today?
05-16 22:14:48  →  You                     can you summarize the architecture?
05-16 22:14:56  ←  claude-wsl-ubuntu       The crow-hub project is a Rust-based multi-agent orchestration…
```

**Implementation** (commit `5e35d80`, +127 lines, no new tests since
it's a thin read-only wrapper over already-tested store APIs):
- `Commands::Memory { command: MemoryCommands }` + `MemoryCommands::{Tail, Count}`
- `open_memory_store()` opens `~/.crow-hub/messages.db` for reads
- `run_memory_tail` queries `MemoryStore::recent`, reverses to chrono
  order, formats per-row.  UTF-8-safe truncation to 120 chars
  (char-count, not byte-index, so CJK content doesn't get sliced).
- `run_memory_count` queries `MemoryStore::count`.
- Helpful empty-state when no messages yet.

**Writer side-tweak**: extended `crates/ch-memory/src/writer.rs` to
also persist `metadata.from_agent_name` (and `to_agent_name`)
alongside the raw UUIDs.  Previously the writer threw away the readable
display name, leaving the tail output stuck rendering ugly 8-char UUID
prefixes.  Backward compatible — entries written before today fall back
to the UUID prefix.

---

## 3. State of the repo

| Metric | Yesterday's journal | Today |
|---|---:|---:|
| Tests passing | 93 | **111** (DeepSeek +18) |
| Commits on `origin/main` | 28034b1 | **5e35d80** (+18 since 13th) |
| `~/.crow-hub/` artifacts | env-cache only | env-cache **+ messages.db** (schema, no data yet) |
| CLI subcommands | tui / server / run / agent / status / send / setup / doctor / refresh-env | **+ memory {tail, count}** |
| 4-day plan | Days 1–2 done | **All 4 days complete** |
| Open PRs | 0 | 0 |

---

## 4. What to do next

DeepSeek's "What Remains" list, with my read on each:

| Item | My take |
|---|---|
| **Phase 5 — memory browser in TUI** | Natural next step.  CLI shipped today gives a clean reference for the UI: list rows, scrollable, maybe filter by agent/channel.  ~1-day feature. |
| Phase 6 — GUI (Tauri) | Multi-week.  Don't start without a real reason — the TUI is healthy. |
| Phase 7 — test coverage, security audit, v0.1.0 | Worth doing before any public-facing announcement.  Test coverage is at 111 (good for a young project) but security audit would catch SSH/subprocess quoting edge cases. |
| Embeddings for memory search | Big jump, but unlocks "ask any agent: what did claude say about X last week?".  Requires picking an embedder (DeepSeek noted `ort` + sentence-transformers in the plan). |
| Parallel agent loading | Quick win — agents load sequentially today; with 11 agents × 2 host probes that's ~10s cold-start.  Could be ~3s. |
| `pricing.toml` for cost estimation | Small + visible.  Multiply existing token counts by a per-model rate. |

The brainstorming docs from DeepSeek (today) point at three bigger
directions: **semantic color palette + tabs (Reasonix)**,
**slash commands + themes (OpenCode)**, **skill marketplace + agent
private groups**.  None of them are in the original 4-day plan;
worth a separate planning conversation with the user before picking
one to execute on.

---

## 5. Reminders for the next agent (so the chain stays clean)

- **No spawned-task worktrees** in this repo.  Claude Code's `Agent`
  tool creates git worktrees under `.claude/worktrees/` that flip
  `[extensions] worktreeConfig = true` in `.git/config`, which breaks
  Antigravity's chat panel for this workspace.  See journal
  `2026-05-13_*` Section 1 for the cleanup procedure if it happens again.
- **Read `docs/plans/2026-05-12_next_four_days.md` and the prior
  journal** before starting — the conventions there (branch naming,
  PR template, test-count gate) keep momentum.
- **Push every commit**.  Today I found 17 of DeepSeek's commits sitting
  in local-only state; pushing them was the first thing I did.  Easy
  to drop: just `git push origin main` at the end of every session.
- **Write a journal**.  This file continues a chain.  Future agents
  read these to know why the architecture is the way it is.
