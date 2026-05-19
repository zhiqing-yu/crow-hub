# 2026-05-19 — DeepSeek Day Plan Execution

**Author:** DeepSeek (AI Agent)
**Plan:** `docs/plans/2026-05-18_deepseek_day.md`
**Status:** All 4 tasks shipped

## Task 1 — Memory scroll inversion fix (`c1c404c`)

Two-line swap: `saturating_add(1)` ↔ `saturating_sub(1)` for Memory
panel Up/Down arms.  94 tests → 94.

## Task 2 — pricing.toml cost estimation (`b12f8c7`)

- New `examples/pricing.toml` with rates for claude-opus/sonnet/haiku,
  gemini-2.0-pro, kimi-code, plus catch-all
- New `ch-core::pricing` module: `Rate`, `PricingTable`, substring
  lookup (case-insensitive, longest-match), `cost()` computation
- `AgentActivity::Idle` gains `cumulative_cost_usd: f64`
- Runtime handler accumulates cost from pricing table × per-request tokens
- TUI shows `·$0.04` suffix when cost > 0
- +7 tests (94 → 107)

Surprise: `include_str!` path was wrong (2 levels vs 3 levels).

## Task 3 — Parallel agent loading (`71e7cd6`)

Pre-probes unique HostKeys via `spawn_blocking` before sequential
`load_plugin`.  Each agent's host env probe is a cold `bash -lc env`
call; doing them in parallel cuts cold start from ~10s to ~3s for
2-host setups.  +1 test (107 → 108).

## Task 4 — Theme struct (`bfcc885`)

- New `ch-tui::theme` module with `Theme` struct + `DEFAULT_THEME` +
  `HIGH_CONTRAST_THEME`
- `CROW_THEME=hc` env var switches themes
- All hardcoded `Color::*` in `app.rs` replaced with `app.theme.*`
- `render_activity` now takes `&Theme` argument
- +3 tests (ch-tui 17 → 20)

## Test count delta

| Milestone | Tests |
|---|---:|
| Plan baseline | 94 |
| Task 2 (+pricing) | +7 |
| Task 3 (+dedup) | +1 |
| Task 4 (+theme) | +3 |
| **Final** | **108** |

Plus ch-tui binary tests: 20

## Commits

```
bfcc885 feat(tui): theme struct + high-contrast built-in
71e7cd6 perf(runtime): probe host env caches in parallel
b12f8c7 feat(monitor): per-agent cost estimation via pricing.toml
c1c404c fix(tui): Memory panel scroll direction
```

## Carry-over (not in today's plan)

- Slash commands (`/model`, `/clear`, `/session`) — needs theme as prereq
- Embedded semantic search
- GUI / Tauri
- Skill marketplace
