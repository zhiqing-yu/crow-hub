# Day Plan for DeepSeek — 2026-05-25

> **Audience**: DeepSeek (or any next coding agent).  Self-contained —
> you do NOT need any prior conversation context to execute this.

---

## Where we are

Yesterday (2026-05-24) Claude shipped **Maestro Task 2 — Evidence
table** end-to-end (commits `1df8ff0 .. bf4a71a` + journal at
`docs/journals/2026-05-24_evidence_table.md`).  Latest commits on
`main`:

```
98d9247 docs(journal): 2026-05-24 — Evidence table (Maestro Task 2)
bf4a71a feat(tui): /evidence claim slash command (Maestro Task 2 3e)
fa6206d feat(memory-cli): crow memory evidence subcommand (Maestro Task 2 3d)
45b0168 feat(memory): memory writer dispatches Evidence + EvidenceVerify (3c)
db98628 feat(protocol): Evidence + EvidenceVerify bus variants (3b)
cfad27e feat(memory): evidence table + EvidenceStore trait (3a)
938d718 chore(tui): SUPPORTED_COMMANDS const + /help regression test
01405b3 feat(tui): bridge Payload::Handoff into chat panel
1df8ff0 chore: cargo fmt --all (normalize pre-existing rustfmt drift)
```

**Test count**: 128 lib + 25 ch-tui binary = **153 total**, all green.

**Read first** (in order):
1. `docs/journals/2026-05-24_evidence_table.md` — yesterday's recap +
   carry-over items (this plan picks up directly from there)
2. `docs/plans/2026-05-22_maestro_inspired_features.md` — the
   original Maestro plan from DeepSeek; we're now past Tasks 1 + 2
3. `docs/journals/2026-05-19_brainstorming_maestro_lessons.md` —
   strategic context

---

## Today's scope — close the Evidence lifecycle, then prove it audits itself

The Evidence storage and emission paths are in place.  What's missing
is **the closing half of the audit loop**: an autonomous component
that observes pending evidence and verifies / fails it based on
policy.  Without that, evidence is just a write-only log.

Two warm-up tasks (~75 min total) close the manual lifecycle, then
the main course (~3-4 hrs) ships a polling verifier agent.

| # | Task | Effort | Why |
|---|------|-------:|-----|
| 1 | Auto-emit Evidence rows from Handoff `decisions` | ~30 min | Proves Handoff and Evidence compose; closes Task 4 stretch from 5/23 |
| 2 | `/evidence verify <id>` + `/evidence fail <id> <reason>` slash commands | ~45 min | Completes the manual lifecycle in the TUI (today only `claim` exists) |
| 3 | Polling verifier with keyword-rule engine | ~3-4 hrs | The headline — first autonomous component that closes the audit loop |
| 4 | (Stretch) GitHub-PR-witness rule | ~45 min | Demonstrates the rule engine handles non-trivial inputs without networking |

Total realistic: **~5 hrs**.  Drop Task 4 if time-boxed.

---

## Pre-flight (5 min)

```bash
cd <repo>
git fetch origin
git checkout main
git pull origin main
git status                            # must be clean
cargo test --workspace --lib          # must show 128 passing
cargo test -p ch-tui --bin crow       # must show 25 passing
```

Stop and report if anything fails.

---

## Task 1 — Auto-emit Evidence from Handoff decisions (~30 min)

**Context**: when an agent emits a `HandoffEnvelope` with
non-empty `decisions: Vec<String>`, each decision is something the
next agent should treat as established.  That's exactly what an
evidence claim is.  Right now the writer persists the envelope JSON
into the `messages` table but doesn't fan out the decisions into the
`evidence` table.

**Fix**: in `crates/ch-memory/src/writer.rs`, in the existing
`Payload::Handoff(env)` arm (around line 105), after building the
chat-table `MemoryEntry`, also fan out each `env.decisions` item as a
pending `EvidenceRow` with:

```rust
EvidenceRow {
    id: format!("{}-d{}", msg.message_id, idx),     // deterministic, idempotent
    task_id: <best available task id>,              // see below
    correlation_id: msg.correlation_id.map(|c| c.to_string()),
    agent_id: msg.from.agent_id,
    agent_name: msg.from.agent_name.clone(),
    claim: decision.clone(),
    status: EvidenceStatus::Pending,
    witness: None,
    metadata: serde_json::json!({
        "source": "handoff",
        "handoff_message_id": msg.message_id.to_string(),
    }),
    created_at: msg.timestamp,
    verified_at: None,
    verified_by: None,
}
```

**Task ID choice**: there's no `task_id` on a `HandoffEnvelope` today.
For first ship, use `msg.correlation_id.map(|c| c.to_string())
.unwrap_or_else(|| msg.message_id.to_string())` — group decisions
from one handoff under one logical task.  This is internally
consistent: a verifier looking at `--task <correlation_id>` sees all
the evidence from that conversation.

**Acceptance**:
- `/handoff finished auth refactor` with no decisions → 0 evidence rows.
- A programmatic handoff with `decisions = vec!["use JWT".into(),
  "drop sessions".into()]` → 2 pending evidence rows visible in
  `crow memory evidence --task <correlation_id_or_msg_id>`.

**Tests** (~2 in `ch-memory::writer` or `ch-memory::backends::sqlite::tests`):
- `handoff_with_decisions_writes_pending_evidence_per_decision`
- `handoff_with_empty_decisions_writes_no_evidence`

The writer doesn't have its own test module today (it's exercised
indirectly).  Either add one in `ch-memory/src/writer.rs::tests` with
a real bus + sqlite, OR add the tests in `sqlite.rs::tests` by
calling the writer's spawn function and seeding the bus.  Pick
whichever is cleaner — the latter probably is.

**Commit**: `feat(memory): auto-emit evidence rows from Handoff decisions`

---

## Task 2 — `/evidence verify` and `/evidence fail` slash commands (~45 min)

The sub-command structure landed yesterday (`/evidence claim …`); now
add the other two verbs.  Both in `crates/ch-tui/src/app.rs` next to
the existing `claim` arm.

### Syntax

```
/evidence verify <id>                  # mark verified, no witness, no note
/evidence verify <id> <witness>        # mark verified with witness URL/hash
/evidence fail <id> <reason>           # mark failed with reason
```

### Implementation

In the existing `"/evidence" =>` arm in `handle_slash_command`, extend
the inner `match subcmd` to handle the new verbs.  Each verb:

1. Splits the remaining arg into `<id>` + `<rest>`.
2. Builds an `EvidenceVerifyMsg { evidence_id, outcome, note }`:
   - `verify` → `outcome: true`, `note: Option<rest>`
   - `fail`   → `outcome: false`, `note: Some(rest)` (reject empty rest)
3. Renders local feedback:
   - `✓ verified: <id>` (verify, green if possible)
   - `✗ failed: <id> — <reason>` (fail, red if possible)
4. Broadcasts on the bus as
   `MessageType::EvidenceVerify + Payload::EvidenceVerify`.

The memory writer (wired in 45b0168) already routes these to
`EvidenceStore::verify` / `fail`.  No new writer work needed.

### Chat scope filter

Add `✓` and `✗` to the pass-through prefixes alongside `⇄` and `📋`
(see `app.rs:~890`).

### SUPPORTED_COMMANDS

No new entries — `/evidence` is already there.  Update `help_lines()`
to document the two new verbs:

```
/evidence claim <text>         Emit an Evidence claim on the bus
/evidence verify <id> [<w>]    Verify a pending claim
/evidence fail <id> <reason>   Mark a pending claim as failed
```

Bump the help text accordingly.

### Tests (~2 in `ch-tui::app::tests`):
- `help_lines_documents_evidence_verify_and_fail`
- (optional) `append_chat_message_passes_check_and_cross_glyphs` if
  you decide to test the scope filter

**Commit**: `feat(tui): /evidence verify and /evidence fail slash commands`

---

## Task 3 — Polling verifier with keyword-rule engine (~3-4 hrs)

**The headline.**  An autonomous component that closes the audit
loop: observes pending evidence, applies rules, emits
`EvidenceVerify` messages.  No LLM, no network, no fancy
heuristics for first ship — just a polling loop + a pluggable rule
trait + one trivial keyword rule.

### Design

**Where it lives**: a new module `crates/ch-memory/src/verifier.rs`
(promotable to its own crate later if it grows).  Mirrors the
`writer.rs` pattern — `spawn_verifier(bus, store, rules,
interval) -> JoinHandle<()>`.

**Trait**:

```rust
#[async_trait]
pub trait VerifierRule: Send + Sync {
    /// Short identifier for logging (e.g. "keyword", "github-pr").
    fn name(&self) -> &'static str;

    /// Evaluate one pending row.  Return `Skip` if this rule doesn't
    /// apply; the verifier will try the next rule.
    async fn evaluate(&self, row: &EvidenceRow) -> RuleOutcome;
}

pub enum RuleOutcome {
    /// Rule fired — verify the row with optional witness.
    Verify { witness: Option<String> },
    /// Rule fired — fail the row with a reason.
    Fail { reason: String },
    /// Rule did not apply; try the next rule.
    Skip,
}
```

**Spawn fn**:

```rust
pub fn spawn_verifier(
    bus: Arc<MessageBus>,
    store: Arc<dyn EvidenceStore>,
    rules: Vec<Box<dyn VerifierRule>>,
    poll_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(poll_interval);
        let verifier_id = AgentId::new();
        info!(
            "verifier subscribed with {} rule(s), polling every {}s",
            rules.len(),
            poll_interval.as_secs()
        );
        loop {
            interval.tick().await;
            let pending = match store.pending(50).await {
                Ok(p) => p,
                Err(e) => {
                    warn!("verifier: pending() failed: {}", e);
                    continue;
                }
            };
            for row in pending {
                // Try each rule in registration order, first match wins.
                for rule in &rules {
                    match rule.evaluate(&row).await {
                        RuleOutcome::Skip => continue,
                        outcome => {
                            emit_verdict(&bus, verifier_id, &row.id, outcome, rule.name()).await;
                            break;
                        }
                    }
                }
            }
        }
    })
}
```

`emit_verdict` builds an `EvidenceVerifyMsg`, wraps it in an
`AgentMessage` from `"verifier:<rule_name>"`, and broadcasts on
`"general"`.  The memory writer (already wired) flips the DB.

### First-ship rule: `KeywordRule`

Scans `row.claim` for sentinel substrings:

- Contains `__test_pass__` → `Verify { witness: None }`
- Contains `__test_fail__` → `Fail { reason: "test sentinel".into() }`
- Otherwise → `Skip`

This is enough to *demonstrate the closed loop* without any
intelligence.  Real rules come later.

```rust
pub struct KeywordRule;

#[async_trait]
impl VerifierRule for KeywordRule {
    fn name(&self) -> &'static str { "keyword" }

    async fn evaluate(&self, row: &EvidenceRow) -> RuleOutcome {
        if row.claim.contains("__test_pass__") {
            RuleOutcome::Verify { witness: None }
        } else if row.claim.contains("__test_fail__") {
            RuleOutcome::Fail { reason: "test sentinel: __test_fail__".to_string() }
        } else {
            RuleOutcome::Skip
        }
    }
}
```

### Wire into the TUI

In both `run_tui` and `run_server` in `ch-tui/src/main.rs`, after
spawning the writer, spawn the verifier with a single `KeywordRule`
and a 10-second poll interval:

```rust
let _verifier_handle = ch_memory::verifier::spawn_verifier(
    hub.bus.clone(),
    memory_store.clone(),
    vec![Box::new(ch_memory::verifier::KeywordRule)],
    std::time::Duration::from_secs(10),
);
```

Make the poll interval configurable via the `CROW_VERIFIER_INTERVAL_SECS`
env var (defaulting to 10).  Off-switch via `CROW_VERIFIER_OFF=1` so
users can disable it without recompiling.

### Tests (~4)

- `keyword_rule_verifies_on_test_pass_marker`
- `keyword_rule_fails_on_test_fail_marker`
- `keyword_rule_skips_unmarked_claims`
- `verifier_end_to_end_flips_pending_to_verified` —
  integration test: spawn writer + verifier with a real
  `SqliteMemoryStore` (`:memory:`), inject an `EvidenceClaim` with
  `__test_pass__` in the claim text via the bus, wait one poll
  interval (force interval = 100ms), assert the row's status is now
  `Verified`.

### Acceptance

Smoke test:

```
$ crow
... type in TUI:
/evidence claim built auth __test_pass__
/evidence claim deployed __test_fail__
... wait 10-15 seconds, then quit ...
$ crow memory evidence --status all
... two rows, one verified, one failed, both verified_by="verifier:keyword"
```

**Commits** (split as you wish — single commit is fine if cohesive):
- `feat(memory): VerifierRule trait + KeywordRule + spawn_verifier`
- `feat(tui): wire verifier into run_tui/run_server with env-configurable interval`

OR roll up as `feat(memory): polling verifier with keyword rule (Maestro audit loop)`.

---

## Task 4 — (Stretch) GitHub-PR-witness rule (~45 min)

If Tasks 1-3 land early, add a second rule that demonstrates
non-trivial-input handling without networking.

### Idea

`GitHubPrRule` looks at `row.witness`.  If it's a URL matching
`^https://github\.com/[^/]+/[^/]+/pull/\d+$`, it consults a hardcoded
in-memory map of "what's the CI status of this PR".  For first ship,
that map is just a const `&[(&str, RuleOutcome)]`.  For real, you'd
hit the GitHub API — out of scope.

```rust
pub struct GitHubPrRule {
    pub known_statuses: HashMap<String, bool>,  // url → passed
}

#[async_trait]
impl VerifierRule for GitHubPrRule {
    fn name(&self) -> &'static str { "github-pr" }

    async fn evaluate(&self, row: &EvidenceRow) -> RuleOutcome {
        let Some(witness) = row.witness.as_deref() else { return RuleOutcome::Skip; };
        if !is_github_pr_url(witness) { return RuleOutcome::Skip; }
        match self.known_statuses.get(witness) {
            Some(true)  => RuleOutcome::Verify { witness: Some(witness.to_string()) },
            Some(false) => RuleOutcome::Fail { reason: format!("CI failed for {}", witness) },
            None        => RuleOutcome::Skip,  // unknown PR; leave for human
        }
    }
}
```

### Tests (~2)

- `github_pr_rule_verifies_when_status_known_pass`
- `github_pr_rule_skips_unknown_pr_url`

### Acceptance

In the TUI:

```
/evidence claim deployed auth refactor
... then somehow set the witness to a known-pass URL ...
```

Hmm — `/evidence claim` doesn't currently let you set a witness.
Either:
- Extend the syntax: `/evidence claim --witness <url> <text>`
- Or just programmatic seeding for the test

For first ship, the test covers it; live demo can come when the slash
command grows a `--witness` flag.

**Commit**: `feat(memory): GitHubPrRule for verifier (witness-URL pattern)`

---

## General conventions (unchanged)

| Topic | Convention |
|---|---|
| Commits | Imperative + scoped (`feat(memory): ...`, `feat(tui): ...`) |
| Tests | Every behavior change adds ≥1 test |
| CI gate | `cargo test --workspace --lib` must pass (current baseline: **128**) |
| Style | `cargo fmt --all` before commit |
| Branches | Direct to `main` for small commits |

## What to AVOID

- ❌ **No spawned-task git worktrees.**  Standing warning since
  2026-05-13.  Cleanup: `docs/journals/2026-05-13_*.md` Section 1.
- ❌ **No force-pushes to `main`.**
- ❌ **No schema migrations.**  All Evidence work today is additive
  (new rows, new metadata keys).  Don't `ALTER TABLE`.
- ❌ **No LLM calls in the verifier.**  The first-ship rule engine is
  intentionally dumb — keyword sentinels.  LLM-based rules are a
  future plan that needs its own design pass (cost budgets, prompt
  caching, latency).
- ❌ **No network calls in `GitHubPrRule`.**  Hardcoded map only for
  first ship; real GitHub-API integration is out of scope.
- ❌ No new top-level workspace deps without checking `Cargo.toml`.

## Reporting back

End-of-day journal at `docs/journals/2026-05-25_<topic>.md` following
the chain.  Cover:

- What shipped (commits + test count delta from 153)
- Whether Tasks 1 + 2 also landed, or just Task 3
- One surprise / design tension / learning
- Carry-over for the next agent

If you only ship Task 2 + Task 3, that's a strong day — Task 3 is the
architectural piece.  Task 1 can carry over.

## Out of scope (deliberately, save for future plans)

- **LLM-based verifier rules** — needs cost / latency / caching design
- **GitHub API integration** for the PR rule — auth, rate limits,
  webhook vs poll
- **Maestro Task 3 (state-machine workflows)** — the next architectural
  piece after the audit loop closes; needs a brainstorm session first
- **Maestro Task 4 (Agent principles)** — small UX work; one-session
  feature when we get to it
- **Tauri GUI** — Phase 6, separate week-scale effort
- **`crow memory evidence --witness <url>`** filter — useful but not
  in today's path
