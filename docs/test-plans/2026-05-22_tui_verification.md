# Test Plan: crow-hub TUI (2026-05-22)

**For:** Claude | **After:** `ae080f0` | **Pre-req:** `cargo build --release --bin crow`

## 0. Pre-flight
- `crow status` — agents visible
- `crow memory count` — ideally >0

## 1. Slash commands
1.1 `/help` → shows command list
1.2 `/model claude-sonnet` → "override set to: claude-sonnet"
1.3 `/model` (blank) → shows current override
1.4 `/clear` → chat cleared
1.5 `/all` → unscopes, shows all agents
1.6 `/xyz` → "Unknown command: /xyz (try /help)"

## 2. Scoped chat
2.1 ↑↓ to claude → name turns cyan bold
2.2 Send "hello" → chat shows only claude + You
2.3 ↑↓ to gemini → chat shows only gemini + You (claude hidden)
2.4 `/all` → all agents visible
2.5 ↑↓ to any agent → scopes back (auto-reset)

## 3. /model override
3.1 `/model gpt-4` → "override set to: gpt-4"
3.2 Send prompt → no crash. If CLI uses model name, should appear in response
3.3 `/model` (blank) → shows current

## 4. Multi-agent broadcast
4.1 Space on 2 agents → [✓] yellow
4.2 Send → both think, both respond
4.3 Chat shows all (broadcast unscopes)

## 5. Token counts + cost
5.1 Send 3-4 prompts → sidebar shows `·22k/284`
5.2 If model in pricing.toml → shows `·$0.04`
5.3 No match → only token counts, no cost

## 6. Memory panel
6.1 Tab to Memory → shows persisted messages
6.2 ↑↓ scroll → ↑ older, ↓ newer
6.3 `r` refresh → reloads
6.4 Tab back → chat restored

## 7. Theme
7.1 `CROW_THEME=hc crow` → high-contrast
7.2 `crow` → default dark

## 8. Agent metadata
8.1 Agent list → model name in dim text below name
8.2 No model → only name, no second line

## Report
| # | P/F | Notes |
|---|-----|-------|
