# Debugging Rules

> **Accumulated lessons from bugs encountered.**  Append-only.  Each
> rule cites the bug postmortem that originated it.  Read
> top-to-bottom periodically as a refresher — the rules at the
> bottom are recent and most likely being violated.
>
> Workflow: at end of every session, the journal author appends any
> rules extracted from today's bugs (or rough edges).  Zero rules
> from a smooth session is fine.  See `CONVENTIONS.md` §11 for the
> postmortem + rule-extraction workflow.

---

## Rule 1: After every batch of source-file modifications, run `cargo check` before moving to the next file.

**Source:** [`bugs/2026-05-29_sed-cr-scaffolding-damage.md`](bugs/2026-05-29_sed-cr-scaffolding-damage.md)

**Trigger:** You've just edited a Rust source file — especially via
sed, regex, multi-line Edit, or any tool that doesn't have semantic
awareness of Rust syntax.

**Action:** Run `cargo check -p <crate>` (or `cargo check --workspace`
if the change spans crates) **before** moving on to the next file
or task.  Don't batch up 5 edits and check at the end.

**Rationale:** Incremental `cargo check` is ~5 seconds.  Catching a
broken paren, missed import, or string-literal injection within one
file is trivial.  Catching it across four files (after four edits
have compounded into 6 errors split across 3 crates) costs an hour
of triage.  The 2026-05-29 sed-CR incident took 4 recovery commits
+ 1 rescue commit because the original sed batch's damage wasn't
validated.

---

## Rule 2: Don't use sed for multi-line Rust changes.  Use Edit/Write (line-aware tools) instead.

**Source:** [`bugs/2026-05-29_sed-cr-scaffolding-damage.md`](bugs/2026-05-29_sed-cr-scaffolding-damage.md)

**Trigger:** You're about to run a sed (or perl one-liner, or
`s///` regex) substitution that spans multiple lines, touches string
literals, or modifies brace blocks in a Rust source file.

**Action:** Use the Edit or Write tool instead.  They're aware of
line boundaries and don't break out of string context.  If you
genuinely need bulk text substitution, do it on a per-file basis
and run Rule 1's `cargo check` after each file.

**Rationale:** Sed is byte-substitution.  It doesn't know about Rust
string boundaries, paren matching, or even character encoding (CR
vs LF mid-line is invisible to sed and most editors).  The
2026-05-29 incident involved sed injecting a Rust statement *inside*
a SQL string literal, leaving duplicate `impl` blocks, and inserting
literal `\r` characters mid-line that tripped the Rust lexer.  None
of these could have happened with line-aware tools.

If sed is truly unavoidable (e.g. mechanical rename across 50+ files
where the pattern is identical and trivial), enforce Rule 1
*per-file*, not per-batch.

---

<!--
Template for new rules:

## Rule N: <One-line actionable statement>

**Source:** [`bugs/YYYY-MM-DD_short-name.md`](bugs/YYYY-MM-DD_short-name.md)

**Trigger:** <when to apply this rule — what situation>

**Action:** <what to do when the trigger fires>

**Rationale:** <one or two sentences on why this saves time>

---
-->
