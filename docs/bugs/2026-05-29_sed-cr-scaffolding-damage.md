# Sed-CR scaffolding damage — 2026-05-29

**Author:** Claude
**Discovered by:** zhiqing (pasted compile errors)
**Originally introduced by:** DeepSeek (via sed-driven workflow scaffolding in prior session)
**Severity:** Compile-broken
**Status:** Fixed in commit `45a7ee5`
**Related:**
- `../DEBUGGING_RULES.md#rule-1` (cargo-check-after-batch — extracted from this)
- `../DEBUGGING_RULES.md#rule-2` (no-sed-for-rust — extracted from this)
- `../plans/for-others/for-deepseek/2026-05-29_close-workflow-loop.md` (Task 1 marked done because of this rescue)

---

## Symptom

`cargo check --workspace` failed with four errors, all in
`crates/ch-memory/src/backends/sqlite.rs`:

```
error: character literal may only contain one codepoint
   --> sqlite.rs:115:150
error: mismatched closing delimiter: `)`
   --> sqlite.rs:711:84
error: mismatched closing delimiter: `)`
   --> sqlite.rs:716:81
error: unexpected closing delimiter: `}`
   --> sqlite.rs:720:5
```

After the first round of fixes there were two more errors:

```
crates/ch-tui/src/app.rs       — unbalanced let bindings in input render
crates/ch-protocol/src/lib.rs  — #[derive] applied to assert_eq! macro
```

## Root cause

DeepSeek used sed in an earlier session to scaffold the workflow code
(`WorkflowStepRow`, `WorkflowStore` trait, SQLite impl, cursor
indicator). The sed batch left multiple distinct injuries across
three files:

1. **`sqlite.rs` line 115**: a Rust statement
   `sqlx::query("CREATE TABLE workflow_steps ...").execute(...).await?;`
   was injected **inside the SQL schema string literal**, between
   `CREATE TABLE evidence (` and its columns.  The compiler tried to
   parse `'pending'` (the SQL default) as a Rust character literal
   because the surrounding string had been broken.

2. **`sqlite.rs` lines 714, 719**: trailing `)` on both
   `Ok(rows.into_iter().map(...).collect()))` — one extra close paren
   per line.

3. **`sqlite.rs` lines 722-741**: an entire **duplicate**
   `impl WorkflowStore for SqliteMemoryStore` block with stub
   `Ok(vec![])` implementations.  Real impl was earlier in the file;
   stub copy conflicted (multiple impls of the same trait for the
   same type is a compile error).

4. **`lib.rs` lines 428-460**: **duplicate**
   `WorkflowStepRow + WorkflowStore` struct + trait definitions.
   The first copy used unqualified `WorkflowStepState` (not
   imported); the second used `ch_protocol::WorkflowStepState`.

5. **`app.rs` lines 1224, 1228, 1236**: a literal fragment
   `    let input_par = Paragraph::new(app.input.as_str())` (with an
   embedded `\r`) was injected in place of the intended slice
   references.  Three separate lines, same garbage.  The embedded CR
   was invisible in editors but tripped the Rust lexer.

6. **`ch-protocol/src/lib.rs`**: `MessageType` variants had doc
   comments shuffled — `WorkflowClaim` wore `EvidenceVerify`'s doc,
   `EvidenceVerify` lost its doc, `Custom` wore `WorkflowClaim`'s
   doc on top of its own.  Also `WorkflowClaim` was wedged between
   `Evidence` and `EvidenceVerify`, breaking the pair readability.

7. **`ch-protocol/src/lib.rs` line 636**: a stray
   `#[derive(Debug, Clone, Serialize, Deserialize)]` attribute was
   injected between two `assert_eq!` lines inside a test.

DeepSeek noticed and attempted recovery in 4 commits (`8328368`,
`9d74c7c`, `e562d17`, `cae84f9`) but the code was still
non-compiling when zhiqing pasted the errors.

## Fix

Single commit `45a7ee5` (`fix: rescue compile after sed-driven
scaffolding damage`):

| File | Lines | What |
|------|------:|------|
| `ch-memory/src/backends/sqlite.rs` | -121, +123 | Stripped injection from SQL string; added proper `CREATE TABLE workflow_steps` clause; deleted duplicate impl; fixed paren balance; restored imports (`WorkflowStepState`, `WorkflowStepRow`, `WorkflowStore`) |
| `ch-memory/src/lib.rs` | -23, +6 | Deleted first duplicate `WorkflowStepRow + WorkflowStore`; kept fully-qualified version (no import change needed) |
| `ch-tui/src/app.rs` | -3, +3 | Three lines fixed via `awk` (Edit tool couldn't match the embedded `\r`); replaced `    let input_par = Paragraph::new(app.input.as_str())\r` fragments with the intended slice references |
| `ch-protocol/src/lib.rs` | -8, +6 | Restored MessageType doc comments + re-paired Evidence/EvidenceVerify; removed stray `#[derive]` from test |

After fix: `cargo test --workspace --lib` → **121 passing**,
`cargo test -p ch-tui --bin crow` → **25 passing**, matches the
pre-corruption baseline.

## Why it wasn't caught earlier

- **DeepSeek didn't run `cargo check` after the sed batch.**  Each
  recovery commit fixed *some* of the damage but introduced or left
  other parts broken.  Six errors in three files compounded across
  four commits before zhiqing's user-visible report.
- **The CR-mid-line damage was invisible in the editor.**
  `ch-tui/src/app.rs:1224` looked normal in most viewers but had
  `...as_str())\rapp.input;` — the CR rendered as nothing visible
  but tripped the Rust lexer.  Even `grep` returned the line as a
  match for `let input_par`; only `od -c` exposed the actual bytes.
- **The Edit tool couldn't match lines with embedded CRs** because
  the input `old_string` was LF-only.  Required falling back to
  `awk` for the byte-level replacement.
- **`.gitattributes` enforces LF at commit boundaries but doesn't
  catch mid-line CRs** introduced by sed running over CRLF text.
  The `.gitattributes` file (added a few sessions ago) covers
  line-ending normalization, not mid-line stray chars.
- **The sed-injection of Rust code into the SQL string literal**
  is a class of damage that *can't* happen with line-aware tools
  (Edit/Write) because they don't break out of string context — sed
  is byte-substitution and doesn't know about Rust syntax.

## Prevention

Two rules extracted (see DEBUGGING_RULES.md):

1. **After every batch of source-file modifications, run `cargo
   check` before moving to the next file.**  Cheap (~5 sec on
   incremental builds).  Catches injection, paren misbalance, type
   mismatches before they compound.

2. **Don't use sed for multi-line Rust changes.  Use Edit/Write
   (line-aware tools) instead.**  Sed is byte-substitution; it
   doesn't know about string boundaries, paren matching, or
   character encoding.  When the change spans multiple lines or
   touches string literals / brace blocks, sed will hurt more than
   it helps.

If sed is unavoidable (e.g. very-large mechanical rewrite),
`cargo check` after EVERY file is non-negotiable — not "after the
batch", but after each file.
