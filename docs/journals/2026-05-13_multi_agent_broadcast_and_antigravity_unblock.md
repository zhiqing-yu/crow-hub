# 2026-05-13 — Multi-agent broadcast + unblocking Antigravity

Builds on `2026-05-12_runtime_and_setup_milestones.md`.  Two visible
changes today plus one important cleanup.

---

## 1. Unblocked Antigravity's chat in the crow-hub workspace

Antigravity's right-side AI chat had gone silent specifically when this
workspace was open.  Symptom was a popup: *"Error generating commit
message: core.repositoryformatversion does not support extension:
worktreeconfig"*.

Root cause: I (Claude Code) had spawned sub-task worktrees in earlier
sessions (`.claude/worktrees/hopeful-kepler-91a13e` and
`romantic-ptolemy-8004a2`).  Git registered them by adding
`[extensions] worktreeConfig = true` to `.git/config`.  Antigravity's
git integration doesn't understand that extension, so it failed to read
the repo state in this workspace, which silently broke the agent panel.

Cleanup:
- `git worktree remove --force` on both worktrees (commits were already
  in main; nothing lost)
- `git config --unset extensions.worktreeConfig`
- `git branch -D claude/hopeful-kepler-91a13e claude/romantic-ptolemy-8004a2`
- `git push origin --delete claude/romantic-ptolemy-8004a2` (remote leftover)
- `git remote prune origin`

After **Developer: Reload Window** in Antigravity, the chat panel
recovered.

**Lesson for future agents**: never use Claude Code's spawned-task
worktree feature on a user-facing repo.  The worktree config sticks in
`.git/config` even after the worktrees are deleted and breaks tools
that don't recognise the extension.  Today's plan got a warning added.

---

## 2. PR #2 merged — repo is shareable

`chore/strip-user-manifests-for-fresh-clones` landed as commit
`b8e6ad6`.  Fresh clones now get:

- 0 user-specific manifests
- 5 templates in `examples/agents/` (native / wsl / ssh + JSON filter
  demo + setup_script demo)
- A working `plugins/agents/.gitkeep` so `crow setup` has somewhere to write
- A 3-command first-run flow in README: clone → `cargo build` → `crow setup` → `crow`

This is what made it possible for the friend's Mac to actually get a
clean baseline rather than inheriting one user's WSL+SSH config.

---

## 3. Day 2 of the 4-day plan — Multi-agent broadcast in TUI

Commit `4e500b4`.  Single PR-equivalent direct to main (no other agents
on this branch right now, so feature-branch ceremony skipped).

**UX**: with the Agents panel focused —
- **Space** toggles the cursored agent into / out of a multi-selection set
- **Backspace** clears the multi-selection
- **Enter** with any multi-selected agents broadcasts the prompt to all
  of them in parallel; falls back to single-agent send when empty

When at least one agent is multi-selected, a `[✓]` / `[ ]` column
appears in the sidebar (collapses when none, so the default single-
agent view stays compact).  Multi-selected agents render in yellow to
visually separate them from the cyan primary cursor.

**Implementation**:
- New field on `App`: `multi_selected: HashSet<usize>` (indices into
  `agents`)
- Three new methods: `toggle_multi_select_current`, `clear_multi_select`,
  `current_send_targets`
- Two pure helpers extracted for testability: `toggle_multi_select` and
  `resolve_send_targets` (no `App` needed to test the state logic)
- Enter handler refactored: extracted `send_prompt_to_agent` so the
  same code path handles N=1 (single) and N>1 (broadcast)
- Visual cue in `ui()` collapses the checkbox column when the
  multi-selection is empty

**Why it was achievable in one session**: every piece of infrastructure
needed already existed.  The bus already supports addressed fan-out;
per-agent runtime handlers already work in parallel; `on_tick` already
merges chunks by agent prefix; activity tracking already animates per
agent.  Multi-select was purely a TUI-side change.

**Tests**: 85 → 93 (+8 new).  All pass.

Test names worth knowing if you're touching this code:
- `resolve_send_targets_multi_select_overrides_primary_cursor` — locks
  in the UX promise that "if I've selected some, those are who I'm
  talking to, regardless of where my cursor wanders"
- `resolve_send_targets_filters_indices_past_end` — defensive against
  stale indices when the agent list shrinks

---

## 4. Heads-up: another agent has WIP in this repo

When I started today, the working tree had uncommitted changes in
five other files from another coding agent:

| File | Apparent direction |
|---|---|
| `crates/ch-agent/src/drivers/subprocess.rs` | Added `working_dir: Option<&str>` parameter to `compose_remote_invocation` |
| `crates/ch-agent/src/host_env.rs` | 7-line tweak |
| `crates/ch-core/src/config.rs` | Renamed `agenthub.toml` → `crow-hub.toml`; switched config paths to `~/.crow-hub/` |
| `crates/ch-core/src/lib.rs` | Added `get_home_dir()` / `get_plugins_dir()` helpers |
| `crates/ch-tui/src/main.rs` | Wired new home-dir helpers into the TUI/setup paths |

One of their new tests (`compose_remote_invocation_with_working_dir_includes_cd`)
expects `"cd '/tmp/project'"` with single quotes, but the existing
`shell_quote` function leaves paths with only safe characters unquoted
— so the test fails as-is.  Either the test needs to accept
`cd /tmp/project` (no quotes for safe paths) or `shell_quote` needs an
always-quote-paths variant.

I deliberately did **not** touch their files.  Stashed their changes
while I shipped multi-select on a clean tree, then popped the stash to
preserve their work.  Their WIP is back in the working tree as
uncommitted changes, ready for them to continue.

If you (the next agent or the user) want to land their work too:
1. Decide whether to keep `shell_quote` as-is and fix the test, or
   change `shell_quote` to always-quote paths (cleaner shell-injection
   posture but breaks readability of the generated commands)
2. Run `cargo test --workspace --lib` to confirm the rest is healthy
3. Commit as a separate concern from multi-select

---

## 5. Test count + repo state

| Metric | Yesterday | Today |
|---|---:|---:|
| Tests passing | 85 | **93** |
| Public commits on main | 9 (5140dbc) | **10 (4e500b4)** |
| Open PRs | 0 | 0 |
| `.git/config` extensions | `worktreeConfig=true` | none |
| User-visible TUI shortcut keys | 8 | **10** (+ Space, + ctx-Backspace) |

---

## 6. Next: Day 3 — Memory layer SQLite persistence

Plan file unchanged: `docs/plans/2026-05-12_next_four_days.md`.  Day 3
scope is the same — wire a writer that persists every bus message to
`~/.crow-hub/messages.db` via `ch-memory`'s existing SQLite scaffold.
No embeddings yet.

Note: the other agent's WIP touches `ch-core/config.rs` to put the
home dir at `~/.crow-hub/` — which is exactly where Day 3's writer
would persist.  Consider landing their `get_home_dir()` helper as a
prerequisite before Day 3, since the writer should use it instead of
hardcoding a path.
