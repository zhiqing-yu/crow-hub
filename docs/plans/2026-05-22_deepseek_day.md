# Day Plan for DeepSeek — 2026-05-22

> **Audience**: DeepSeek (or any next coding agent).  Self-contained —
> you do NOT need any prior conversation context to execute this.

---

## Where we are

Crow Hub TUI is functional with all of Claude's original 4-day plan
shipped plus DeepSeek's polish day (`2026-05-18`).  The repo has:

- 108 core unit tests + 20 ch-tui binary tests passing
- Multi-agent broadcast (Space-toggle in Agents panel)
- Memory panel + CLI (`crow memory tail`, `crow memory count`)
- Token counts (`·22k/847`) + cost estimation (`·$0.04`) per agent
- Themes (`CROW_THEME=hc` for high-contrast)
- Parallel agent loading (cold start ~3s)
- Slash command **framework**: `/help`, `/clear`, `/model` (the last is a stub)

Most recent journals:
- `2026-05-19_deepseek_day_execution.md` — DeepSeek's last day
- `2026-05-19_brainstorming_maestro_lessons.md` — strategic future-direction notes
- `2026-05-17_agents_panel_layout_fix.md` — layout fix

---

## Today's focus — close the gaps Claude found while testing

A test pass by Claude on 2026-05-22 surfaced four gaps.  Today plugs
the three most impactful, in order of value-per-hour:

| # | Task | Effort | Why |
|---|------|-------:|-----|
| 1 | Memory writer sanity check | ~30 min | `crow memory count` returns 0 — verify writer actually persists; fix if broken |
| 2 | Scoped chat by agent + `/all` command | ~3-4 hrs | THE feature that makes the TUI feel like a multi-agent hub vs a chat firehose.  P1 in Reasonix brainstorm. |
| 3 | Make `/model` real (not a stub) | ~1.5 hrs | `/model claude-sonnet` currently sets a field that's never read.  Removes a confusing stub. |
| 4 | (Stretch) `/agent <name>` jump-to-focus | ~30 min | Quick keyboard agent picker if Tasks 1–3 land early |

Total: **~5-6 hrs realistic**.  Drop Task 4 if time-boxed.

---

## Pre-flight (5 minutes)

```bash
cd <repo>
git fetch origin
git checkout main
git pull origin main
git status                            # must be clean
cargo test --workspace --lib          # must show 108 passing
cargo test -p ch-tui --bin crow       # must show 20 passing
```

Stop and report if anything fails.

Read first:
- `docs/journals/2026-05-19_deepseek_day_execution.md`
- `docs/journals/2026-05-16_brainstorming_reasonix_design_lessons.md`
  (especially item #5 "List + detail two-panel" — that's Task 2)

---

## Task 1 — Memory writer sanity check (~30 min)

**Symptom**: `crow memory count` returns `Stored messages: 0` even
though the TUI has been used.  Either (a) the DB file is fresh after
the `~/.crow-hub/` path migration (`16fc90b`-ish), (b) the writer
subscriber isn't running, or (c) it errors silently.

**File hint**: search for where the memory writer is spawned.  Should
be in `crates/ch-tui/src/main.rs` (TUI bootstrap) or
`crates/ch-core/src/bus.rs` / `crates/ch-memory/src/`.  Likely shape:

```rust
pub fn spawn_memory_writer(bus: Arc<MessageBus>, store: Arc<dyn MemoryStore>) -> JoinHandle<()> {
    ...
}
```

### Steps

1. `grep -rn "spawn_memory_writer\|memory writer\|MemoryStore" crates/`
   — find the spawn site
2. Confirm it's actually called in the TUI startup path
3. Add a `tracing::info!` log at writer start ("memory writer subscribed to channel <X>") and at each successful write (DEBUG level)
4. Build release.  Run `crow.exe`, send ONE prompt to any agent that responds (`claude-wsl-ubuntu` is fastest), wait for response.
5. Quit (`Ctrl+C`).  Run `crow memory count`.
   - **Expected**: at least 2 (one TaskRequest from "You", one TaskResponse from the agent)
   - **If still 0**: writer is broken.  Likely culprits:
     a) Writer never subscribed to `general` channel (check `bus.join_channel(...)` call)
     b) Writer task panicked on first message (look at `crow-hub.log` for stack trace)
     c) `MessageType` filter is wrong (writer only persists `TaskRequest|TaskResponse`?)
     d) SQLite write path errors silently (`.ok()` swallows errors — fix to `if let Err(e) = ...`)

### Acceptance

- After sending one TUI message, `crow memory count` ≥ 2.
- `crow memory tail -n 5` shows the round-trip with sensible timestamps and agent names.
- If you had to fix something, add a regression test in
  `crates/ch-memory/src/backends/sqlite.rs` for the failure mode you found.

### Commit

- If no bug: `chore(memory): add tracing on writer subscribe + write` — small,
  improves observability for next time this is doubted.
- If bug: `fix(memory): <describe>` + the regression test.

---

## Task 2 — Scoped chat by agent + `/all` command (~3-4 hrs)

**Goal**: when the user moves the cursor in the Agents panel (↑↓), the
chat panel filters to messages from THAT agent only (plus the user's
own messages to them).  An `/all` slash command unscopes back to the
firehose view (current behavior).

This is the single biggest UX gap.  Currently the TUI shows all
agents' responses interleaved in one chat, which makes broadcast
mode noisy and 1:1 conversations hard to follow.

### Design

**Storage shape**: `app.messages` is currently `Vec<String>`.  Two
realistic approaches:

#### Option A — Keep `Vec<String>`, prefix-filter on render (RECOMMENDED)

- Each message already starts with `"<agent>: "` or `"You: "`
- On render, filter to lines starting with `format!("{}: ", selected_agent)`
  OR with `"You: "`
- An `app.chat_scope: ChatScope` field controls the filter:
  ```rust
  pub enum ChatScope {
      All,                    // current behavior
      Agent(String),          // only messages from this agent + You
  }
  ```
- Default: `ChatScope::All`
- Cursor movement in Agents panel sets `ChatScope::Agent(<name>)`
  ONLY when no multi-selection is active.  (Multi-select implies
  broadcast context — keep showing all.)
- `/all` resets to `ChatScope::All`.

Pros: 50 lines of code, zero schema changes, no migration.
Cons: relies on `<agent>: ` prefix being unique (collisions if an
agent's response naturally starts with `name: `?  Defensive: only
match prefix on whole-line boundary; user-typed lines are always
`"You: "`).

#### Option B — Restructure to `Vec<ChatMessage>` with agent attribution

- Each message becomes a struct: `{ from: String, content: String, timestamp: DateTime }`
- Filter on `from == selected_agent || from == "You"`
- Cleaner long-term but ~150 lines of change including all `messages.push(format!(…))` callsites.

**Pick Option A** for today.  Refactor to Option B later if string-prefix
filtering proves fragile.

### Implementation steps

1. **`App` field**:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum ChatScope {
       All,
       Agent(String),
   }
   pub chat_scope: ChatScope,  // default ChatScope::All
   ```

2. **Cursor handler** (the `KeyCode::Up`/`KeyCode::Down` Agents arms):
   - After updating `selected_agent`, if `multi_selected.is_empty()`,
     set `chat_scope = ChatScope::Agent(app.agents[selected_agent].name.clone())`.
   - Reset `chat_scroll_offset = 0` so user sees the latest in the
     newly-scoped view.

3. **Slash command**:
   - Add `/all` to `handle_slash_command`:
     ```rust
     "/all" => {
         self.chat_scope = ChatScope::All;
         self.messages.push("Chat unscoped — showing all agents.".into());
     }
     ```
   - Update `/help` to document it.

4. **Chat panel render** (around line 660+ of `app.rs`):
   ```rust
   let filter_prefix: Option<String> = match &app.chat_scope {
       ChatScope::All => None,
       ChatScope::Agent(name) => Some(format!("{}: ", name)),
   };

   let mut all_lines: Vec<String> = Vec::new();
   for m in &app.messages {
       // Filter: keep messages from "You:" always, plus messages
       // matching the scope's agent prefix.  If no scope, keep all.
       let keep = match &filter_prefix {
           None => true,
           Some(prefix) => m.starts_with("You: ") || m.starts_with(prefix.as_str()),
       };
       if !keep { continue; }
       let wrapped = wrap_text(m, width);
       all_lines.extend(wrapped);
   }
   ```

5. **Chat panel title** — show the scope:
   ```rust
   let title = match &app.chat_scope {
       ChatScope::All => "Channel: #general  (all agents)".to_string(),
       ChatScope::Agent(name) => format!("Channel: #general  →  {}", name),
   };
   ```

6. **`on_tick` interaction**: when a streaming chunk arrives for an
   agent not currently in scope, the message gets appended to
   `app.messages` as normal — it'll be invisible in the scoped view
   but the agent's status indicator still updates in the sidebar.
   The user can switch scope to see it.

### Tests

In the existing `tests` module of `app.rs`:

```rust
#[test]
fn chat_scope_all_keeps_all_messages() {
    let messages = vec![
        "You: hi".to_string(),
        "claude-wsl-ubuntu: hello".to_string(),
        "gemini-ssh-1: hey".to_string(),
    ];
    let kept = filter_messages_for_scope(&messages, &ChatScope::All);
    assert_eq!(kept.len(), 3);
}

#[test]
fn chat_scope_agent_keeps_user_and_agent_only() {
    let messages = vec![
        "You: hi".to_string(),
        "claude-wsl-ubuntu: hello".to_string(),
        "gemini-ssh-1: hey".to_string(),
        "You: ping claude".to_string(),
    ];
    let kept = filter_messages_for_scope(
        &messages,
        &ChatScope::Agent("claude-wsl-ubuntu".to_string()),
    );
    assert_eq!(kept, vec![
        "You: hi".to_string(),
        "claude-wsl-ubuntu: hello".to_string(),
        "You: ping claude".to_string(),
    ]);
}

#[test]
fn chat_scope_agent_with_no_matching_messages_keeps_only_user() {
    let messages = vec![
        "You: hi".to_string(),
        "claude-wsl-ubuntu: hello".to_string(),
    ];
    let kept = filter_messages_for_scope(
        &messages,
        &ChatScope::Agent("gemini-ssh-1".to_string()),
    );
    assert_eq!(kept, vec!["You: hi".to_string()]);
}
```

Extract `filter_messages_for_scope` as a pure helper alongside
`resolve_send_targets` / `toggle_multi_select` so it's testable
without an `App`.

### Acceptance

- Launch TUI, send one prompt to each of 2-3 agents (broadcast or
  separately), confirm all responses appear in chat
- Move cursor in Agents panel — chat narrows to that agent's
  conversation only.  Title bar shows `Channel: #general → <name>`
- `/all` restores the firehose view + acknowledgement line
- When multi-selecting agents (Space) → cursor moves do NOT scope
  (because broadcast context — you want to see everyone)
- Tests pass

### Commit

`feat(tui): scoped chat by agent — cursor focus filters chat panel, /all to unscope`

---

## Task 3 — Make `/model` real (~1.5 hrs)

**Problem**: `/model claude-sonnet` sets `app.default_model` but it's
never plumbed into `send_prompt_to_agent` — so the next chat goes out
with the agent's manifest-default model regardless.

### Design

Carry the override in the bus message.  Options:

**A.** New field on `AgentMessage`: `model_override: Option<String>`
**B.** Put it in the existing `Payload::Text(...)` wrapper somehow
**C.** New variant `Payload::TextWithOverride { text, model }`
**D.** Pass through env / context map (`AgentMessage` already has `memory_context: Vec<String>`)

**Pick A** — cleanest schema, minimal invasion, easy to test.  Add
the field with `#[serde(default)]` so existing serialized messages
still parse.

### Implementation

1. **`ch-protocol/src/types.rs`** (or wherever `AgentMessage` lives):
   ```rust
   pub struct AgentMessage {
       ...
       /// Optional per-message model override.  If `Some`, the
       /// runtime's per-agent handler uses this instead of the
       /// agent's manifest-default model.
       #[serde(default)]
       pub model_override: Option<String>,
   }
   ```
   Add a builder method `with_model_override(self, model: String) -> Self`.

2. **`crates/ch-tui/src/app.rs::send_prompt_to_agent`**:
   Take an optional `model_override: Option<String>` parameter; build
   the `AgentMessage` with it set.

3. **`crates/ch-tui/src/app.rs::run_loop` Enter handler**:
   Pass `Some(app.default_model.clone())` when `default_model` is
   non-empty, else `None`.

4. **`crates/ch-agent/src/runtime.rs`** per-agent message handler:
   In the message loop, read `msg.model_override`.  When `Some(m)`,
   use `m` instead of `default_model_for_task` in the
   `ChatRequest::simple(&model, &prompt)` call.  Log at INFO when
   the override is applied:
   ```rust
   let model = match &msg.model_override {
       Some(m) => {
           info!("[{}] using model override: {}", agent_name, m);
           m.clone()
       }
       None => default_model_for_task.clone(),
   };
   ```

5. **Update `/help`** — drop the "Set default model" phrasing
   ambiguity:
   ```
   /model <name>       Override the model for outgoing messages (per session)
   /model              Show current override
   ```

6. **Update `/model` ack message** to reinforce the new behavior:
   ```rust
   self.messages.push(format!(
       "/model — outgoing messages will now request model '{}'.  Use '/model' alone to view, '/model -' to clear.",
       arg
   ));
   ```
   Plus: special case `arg == "-"` to clear the override (set to empty string).

### Tests

In `ch-protocol/src/types.rs` tests:

- `AgentMessage` serde round-trip with `model_override: Some("claude-sonnet")`
- Backward compat: deserializing a JSON with no `model_override` field yields `None`
- `with_model_override(...)` builder

In `ch-agent/src/runtime.rs` tests:

- (If you have a mock backend test) Send `AgentMessage` with override
  → confirm `chat()` was called with that model

In `ch-tui` `app.rs` tests: not strictly needed since the wiring is a
single field pass-through; covered by integration test of slash command
+ a small "model_override is None for empty default_model" test.

### Acceptance

- `/model claude-sonnet` → next prompt uses claude-sonnet.  Verify
  in the agent's logs (`crow-hub.log`) for the "using model override"
  INFO line.
- `/model` (no arg) → shows "current: claude-sonnet"
- `/model -` → clears the override; subsequent prompts go back to
  manifest defaults
- Logs show the override being applied
- Existing tests still pass + ~3-5 new ones

### Commit

`feat(tui): /model command now sets a per-session model override (was a stub)`

---

## Task 4 — (Stretch) `/agent <name>` keyboard jump (~30 min)

If you finish 1-3 early, add a quick agent picker.

```
/agent <substring>      Jump to first matching agent in sidebar
/agent                  List loaded agents
```

Implementation: in `handle_slash_command`, find first index `i` such
that `app.agents[i].name.contains(arg)` (case-insensitive),
set `app.selected_agent = i`, and trigger the scope update from
Task 2.  Show "not found" if no match.

### Acceptance

- `/agent claude` jumps to `claude-wsl-ubuntu` (or whichever claude is first)
- `/agent gemini` jumps to a gemini agent
- `/agent xyz` shows "no agent matches 'xyz'"
- Cursor in Agents panel visibly moves; chat re-scopes per Task 2

### Commit

`feat(tui): /agent <substring> command — keyboard jump to first matching agent`

---

## General conventions (same as last time)

- Commits: imperative + scoped (`fix(tui): ...`, `feat(monitor): ...`)
- Tests: every behavior change adds ≥1 test
- CI gate: `cargo test --workspace --lib` must pass before push
- Style: `cargo fmt --all` before commit
- Branches: direct to `main` for these small commits; create a feat
  branch only if Task 2 explodes

## What to AVOID

- ❌ **No spawned-task worktrees.**  Same warning as every recent
  plan.  Working in-session.  Cleanup procedure in
  `2026-05-13_*` Section 1 if accidentally invoked.
- ❌ No force-pushes to `main`.
- ❌ No schema migrations in Task 3 — the `model_override` field is
  additive with `#[serde(default)]`.  Old DB rows / cached messages
  must still parse.

## Reporting back

End-of-day journal at `docs/journals/2026-05-22_<short-topic>.md`:

- What shipped (commits + test count delta)
- Memory writer status (Task 1 outcome — was it broken?  What was the fix?)
- One surprise / design tension / learning
- Carry-over for next agent

If you only shipped Tasks 1 and 2, that's still a strong day — Task
2 alone is the biggest UX improvement since multi-agent broadcast.
