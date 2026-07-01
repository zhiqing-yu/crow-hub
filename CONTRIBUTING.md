# Contributing to Crow Hub

## Setup

```bash
git clone https://github.com/zhiqing-yu/crow-hub.git
cd crow-hub
cargo build --workspace
```

See the [README](README.md#-快速开始) for first-run setup (`crow setup`, `crow doctor`).

## Before opening a PR

```bash
cargo test --workspace --lib   # all tests must pass
cargo fmt --all                # formatting must be clean
cargo build --workspace        # no new warnings you introduced
```

## Code style

- No bare `ratatui::style::Color::*` outside `crates/ch-tui/src/theme.rs` — add a
  token to the `Theme` struct instead and reference it (`app.theme.xyz`). This
  keeps `CROW_THEME=hc` (high-contrast) working everywhere.
- New TUI slash commands: add the verb to `SUPPORTED_COMMANDS` in
  `crates/ch-tui/src/app.rs` and document it in `help_lines()` — a regression
  test asserts every entry in `SUPPORTED_COMMANDS` appears in `/help` output.
- Prefer adding a test alongside new storage/protocol code over a one-off
  manual check.

## Documentation

This repo uses a two-tier doc system for internal design docs (brainstorms,
plans, journals) under `docs/` — see [`docs/CONVENTIONS.md`](docs/CONVENTIONS.md)
if you're contributing design discussion or session journals. User-facing
docs (README, this file) don't need to follow that convention.

## Commit messages

Conventional-commit-style prefixes (`feat:`, `fix:`, `docs:`, `refactor:`)
are preferred but not enforced by tooling.

## License

By contributing, you agree your contributions are licensed under the
Apache License 2.0 (see [LICENSE](LICENSE)).
