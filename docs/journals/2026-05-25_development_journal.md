# 2026-05-25 — Development Journal

**Author:** DeepSeek | **Plan:** `docs/plans/2026-05-25_deepseek_day.md`

## Feature work

### Handoff → Evidence auto-emit (`f00a435`)
Writer fans out Handoff decisions as pending Evidence rows with deterministic ids.

### /evidence verify/fail commands (`f00a435`)
Manual evidence lifecycle complete: verify/fail via slash commands.

### Polling verifier (`2ca3e93`)
- `VerifierRule` trait + `KeywordRule` (__test_pass__ / __test_fail__ sentinels)
- `spawn_verifier()` polls every N secs, emits verdicts on bus
- Env-configurable interval + off-switch
- +3 tests (ch-memory 13→16)

## Bug fixes (6 commits)

| Commit | Issue | Fix |
|--------|-------|-----|
| 6bf47d8 | Memory tab crash (block_on) | Background task + collect on tick |
| db902c8 | SSH multi-turn broken | Restart persistent subprocess each request |
| 97a01c2 | Space + Version conflicts | Version to tab bar, Space revert |
| 8cb9d10 | Tab bar rendering glitch | Single Line, no overlapping widgets |
| b31681a | First input invisible | terminal.clear() before first draw |
| cf7a0a4 | Input scroll with one line | Gate on input_line_count() > 7 |

## Tests: 146 all green

## Claude shipped: DESIGN_SYSTEM.md (435 lines)

## Surprises
- Persistent subprocess was always broken for multi-turn
- block_on panics in tokio — spawn+collect is correct
- Two widgets in same Rect overwrite — single Line is correct
