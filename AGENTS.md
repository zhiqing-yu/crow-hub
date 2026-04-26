# Crow Hub — Agent Guide

This document contains everything an AI coding agent needs to know when working on the **Crow Hub** project.

---

## 1. Project Overview

**Crow Hub** is a universal AI Agent orchestration hub written in Rust. It acts as middleware that connects disparate AI agents (Claude, Kimi, Gemini, local CLI tools, Ollama, etc.) through a unified message bus, allowing them to collaborate, communicate, and execute workflows together in shared sessions.

Tagline: *调度你的 AI Agent 团队* ("Schedule your AI Agent team")

The project is currently in early development (Phase 0 / early Phase 1 of a planned 24-week, 8-phase roadmap).

---

## 2. Technology Stack

| Layer | Technology |
|-------|------------|
| Language | Rust 1.86.0 (MSRV 1.75) |
| Async runtime | Tokio 1.35 |
| Serialization | Serde + JSON/TOML |
| HTTP client | Reqwest 0.11 |
| HTTP server | Axum 0.7 + Tower/Tower-HTTP |
| gRPC | Tonic 0.11 + Prost 0.12 |
| CLI parsing | Clap 4.4 |
| TUI framework | Ratatui 0.24 + Crossterm 0.27 |
| GUI framework | Tauri (planned for Phase 6) |
| Configuration | Figment 0.10 (TOML + ENV) |
| Logging/Tracing | Tracing + tracing-subscriber |
| Error handling | thiserror + anyhow |
| Concurrency | DashMap, parking_lot, crossbeam |
| Testing | mockall, tempfile, tokio-test |

---

## 3. Workspace Structure

This is a Cargo workspace with **9 crates** under `crates/`:

| Crate | Type | Binary | Description |
|-------|------|--------|-------------|
| `ch-protocol` | library | — | Zero-dependency message contracts (`AgentMessage`, `Payload`, `TaskSpec`, metrics structs) |
| `ch-core` | library | — | Core engine: `MessageBus`, `AgentRegistry`, `SessionManager`, `Orchestrator`, `CrowHub` |
| `ch-model` | library | — | Model routing & discovery: `ModelRouter`, `ModelRegistry`, backends (OpenAI-compat, Anthropic, Ollama) |
| `ch-agent` | library | — | Agent plugin system: TOML manifests, `AgentRuntime`, drivers (`api`, `subprocess`, `tmux`, `mcp`) |
| `ch-adapter` | library | — | Legacy adapter trait abstraction (`AgentAdapter`, `AdapterFactory`) — partially superseded by `ch-agent` |
| `ch-memory` | library | — | Shared vector memory: `MemoryStore` trait, SQLite backend (default), embedder stubs |
| `ch-monitor` | library | — | Metrics & observability: token usage, latency, GPU collectors, Prometheus exporter |
| `ch-tui` | binary | `crow` | Terminal UI — the primary entry point |
| `ch-gui` | binary | `crow-gui` | GUI placeholder (prints "coming soon"); Tauri integration planned |

### Dependency Graph

```
ch-tui  →  ch-protocol, ch-core, ch-adapter, ch-memory, ch-monitor, ch-model, ch-agent
ch-gui  →  ch-protocol, ch-core
ch-agent →  ch-protocol, ch-core, ch-model
ch-adapter → ch-protocol, ch-core
ch-monitor → ch-protocol, ch-core
ch-memory → ch-protocol
ch-model → ch-protocol
ch-core → ch-protocol
ch-protocol → (base only)
```

### Key Directories

- `crates/` — Workspace members (see above).
- `examples/` — Example configs: `crow-hub.toml`, `simple-workflow.yaml`.
- `plugins/agents/` — TOML plugin manifests for agents (e.g. `claude-wsl-ubuntu/agent.toml`).
- `adapters/` — **Empty** at the project root; adapter code lives in `crates/ch-adapter/src/adapters/`.
- `docs/` — `ARCHITECTURE.md`, `ROADMAP.md`, diagrams.
- `.github/workflows/` — CI/CD definitions.

---

## 4. Build Commands

Use the **Makefile** for daily development:

| Command | What it does |
|---------|--------------|
| `make build` | `cargo build --all` |
| `make build-release` | `cargo build --release --all` |
| `make test` | `cargo test --all --verbose` |
| `make run` | `cargo run --bin crow` |
| `make dev` | `cargo watch -x 'run --bin crow'` |
| `make fmt` | `cargo fmt --all` |
| `make lint` | `cargo clippy --all-targets --all-features -- -D warnings` |
| `make check` | `cargo check --all` |
| `make ci` | Runs `fmt`, `lint`, `test`, `build` in sequence |
| `make install` | `cargo install --path crates/ch-tui` |
| `make doc` | `cargo doc --all --no-deps --open` |
| `make test-coverage` | `cargo tarpaulin --all --out Html` |
| `make audit` | `cargo audit` |
| `make setup` | Installs dev tools: `cargo-watch`, `cargo-tarpaulin`, `cargo-edit` |

### Direct Cargo

```bash
# Build everything
cargo build --release

# Run the TUI binary
cargo run --bin crow

# Run the GUI placeholder
cargo run --bin crow-gui

# Install TUI binary to ~/.cargo/bin as `crow`
cargo install --path crates/ch-tui
```

---

## 5. Code Style Guidelines

- **Formatting:** Use `cargo fmt --all`. There is **no** `.rustfmt.toml`; rely on default rustfmt settings.
- **Linting:** CI treats all Clippy warnings as errors (`-D warnings`). Run `make lint` before pushing.
- **Error handling:**
  - Use `thiserror` for library error enums (e.g. `ProtocolError`, `AgentError`).
  - Use `anyhow` for application-level error propagation (e.g. in `ch-tui`/`ch-gui` binaries).
- **Async:** Prefer `tokio::sync::mpsc`/`broadcast` for channels. Use `async-trait` for trait-based async APIs.
- **Concurrency:** Use `DashMap` for lock-free concurrent maps (agent registry, metrics). Use `parking_lot` for lightweight locks when needed.
- **Naming:** Follow standard Rust naming (`PascalCase` types, `snake_case` functions/variables, `SCREAMING_SNAKE_CASE` constants).
- **Documentation:** Document public APIs with `///` doc comments. Keep `ARCHITECTURE.md` and `ROADMAP.md` in sync when making architectural changes.

---

## 6. Testing Instructions

### Test Organization

There are **no dedicated `tests/` integration directories**. All tests are **inline unit tests** inside source files under `#[cfg(test)]` modules.

### Crates with Test Coverage

- `ch-protocol` — `src/lib.rs`
- `ch-core` — `src/lib.rs`, `src/bus.rs`, `src/channel.rs`
- `ch-model` — `src/lib.rs`, `src/router.rs`, `src/registry.rs`, `src/backends/mock.rs`
- `ch-agent` — `src/lib.rs`, `src/manifest.rs`, `src/loader.rs`, `src/runtime.rs`, `src/scanner.rs`, `src/drivers/api.rs`, `src/drivers/subprocess.rs`
- `ch-adapter` — `src/lib.rs`
- `ch-memory` — `src/lib.rs`, `src/backends/sqlite.rs`, `src/embedder/local.rs`
- `ch-monitor` — `src/lib.rs`
- `ch-tui` / `ch-gui` — listed test dependencies, inline tests minimal

### Test Dependencies

- `mockall = "0.12"` — mocking trait objects (used in `ch-core`, `ch-model`, `ch-adapter`).
- `tempfile = "3.9"` — temporary files/directories (used in `ch-core`, `ch-memory`, `ch-agent`).
- `tokio-test = "0.4"` — async test runtime (used in **all 9 crates**).

### Running Tests

```bash
# Run all tests
make test

# Or directly
cargo test --all --verbose
cargo test --doc --all

# Coverage report (requires cargo-tarpaulin)
make test-coverage
```

### Testing Conventions

- Use `#[tokio::test]` for async tests.
- Mock external dependencies with `mockall` when testing traits like `ModelBackend` or `AgentAdapter`.
- Clean up temporary resources using `tempfile::TempDir` or `tempfile::NamedTempFile`.

---

## 7. Security Considerations

- **Configuration secrets:** `HubConfig` and `AgentManifest` may contain API keys. The config loader uses Figment; ensure secrets are loaded from environment variables (`AGENTHUB_*` prefix) rather than hard-coded in committed TOML files.
- **Subprocess driver:** `SubprocessDriver` spawns external CLI processes. Validate command paths and arguments in manifests to avoid command injection. Shell modes (`Native`, `Wsl`, `Ssh`) must sanitize inputs before spawning.
- **WSL / SSH scanning:** `EnvironmentScanner` auto-discovers agents across WSL distros and SSH hosts. Be cautious about executing remote commands; ensure discovery is read-only where possible.
- **WebSocket / HTTP adapters:** Adapters communicate with external services over HTTP/WebSocket. Use TLS (`rustls-tls`) and validate server certificates.
- **Model Context Protocol (MCP):** The `mcp` driver is reserved/future. When implemented, enforce strict capability boundaries and input validation.
- **Dependencies:** Run `make audit` periodically to check for vulnerable crates.

---

## 8. Runtime Architecture

### Entry Points

- **`crates/ch-tui/src/main.rs`** (`crow` binary) — Primary CLI/TUI entry point.
  - Subcommands: `tui` (default), `server`, `run <workflow>`, `agent {list|add|remove|show}`, `status`, `send`, `setup`.
- **`crates/ch-gui/src/main.rs`** (`crow-gui` binary) — Placeholder GUI.

### Typical Data Flow (TUI Chat)

1. User types a message in `ch-tui` and presses Enter.
2. TUI builds an `AgentMessage` (`MessageType::TaskRequest`) addressed to the selected agent.
3. Message is sent via `MessageBus::send_to_channel("general", ...)`.
4. The agent's async handler in `AgentRuntime` receives the message.
5. Handler delegates to the agent's driver:
   - `APIDriver` → `ModelRouter` → `ModelBackend` (e.g. Anthropic).
   - `SubprocessDriver` → spawns CLI process.
   - `TmuxDriver` → sends input to tmux session.
6. Response is wrapped in `AgentMessage` (`MessageType::TaskResponse`) with the same `correlation_id` and published back to `#general`.
7. TUI's bus bridge receives the response and displays it.

### Core Components

- **`CrowHub`** (`ch-core`) — Root struct that owns `MessageBus`, `AgentRegistry`, `SessionManager`, and `Orchestrator`.
- **`MessageBus`** (`ch-core`) — Async pub/sub with named channels (e.g. `#general`), direct messaging, request/response patterns, and a 10k-message circular history buffer.
- **`AgentRegistry`** (`ch-core`) — `DashMap`-backed registry of agents with capabilities and heartbeats.
- **`Orchestrator`** (`ch-core`) — DAG workflow executor with dependency resolution, retries, and timeouts.
- **`AgentRuntime`** (`ch-agent`) — Loads plugins from `plugins/agents/`, assigns `AgentId`s, and manages per-agent message handlers.
- **`ModelRouter`** (`ch-model`) — Routes chat requests to the appropriate backend by model name.

---

## 9. CI/CD and Deployment

### GitHub Actions Workflows

- **`.github/workflows/ci.yml`** — Runs on pushes to `main`/`develop` and PRs to `main`.
  - **Check** job (`ubuntu-latest`): `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo check --all`.
  - **Test** job (matrix: Ubuntu, Windows, macOS): `cargo test --all --verbose` → `cargo test --doc --all`.
  - **Build** job (matrix: Linux, macOS, Windows x64): `cargo build --release --target <target>` and upload artifacts (`ah` / `ah.exe`).
- **`.github/workflows/release.yml`** — Runs on tags `v*`.
  - Builds for 6 targets (Linux x64/ARM64, macOS x64/ARM64, Windows x64/ARM64).
  - Packages into `.tar.gz` (Unix) or `.zip` (Windows).
  - Creates a GitHub Release with all artifacts.

Both workflows cache `~/.cargo/registry`, `~/.cargo/git`, and `target/` via `actions/cache@v3`.

### Deployment Notes

- The primary artifact is the `crow` binary from `crates/ch-tui`.
- Cross-compilation targets are pre-declared in `rust-toolchain.toml`.
- No containerization (Docker) is currently configured.

---

## 10. Important Conventions & Gotchas

- **Protocol-first design:** `ch-protocol` must remain free of internal crate dependencies. Do NOT add `ch-core` or `ch-model` as dependencies to `ch-protocol`.
- **Workspace inheritance:** All crates inherit `version`, `edition`, `authors`, `license`, and `rust-version` from the root `Cargo.toml`. Update the root when bumping versions.
- **Naming consistency:** Use `ch-*` prefixes for crates. Avoid `ah-*` (an older prefix that occasionally appears in `ROADMAP.md` snippets but is not used in the actual codebase).
- **`ch-adapter` vs. `ch-agent`:** The `AgentAdapter` trait in `ch-adapter` is partially superseded by the driver system in `ch-agent`. New adapter-like functionality should generally go into `ch-agent` drivers or `ch-model` backends, unless you are explicitly extending the legacy adapter trait.
- **TUI is the main frontend:** `ch-tui` pulls in nearly all crates. When adding new public APIs, ensure they are accessible from `ch-tui`.
- **GUI is a placeholder:** `ch-gui` currently only prints a welcome message. Do not expect it to exercise any real functionality.
- **No `.cargo/config.toml`:** The `.cargo/` directory is empty. Do not add one unless you specifically need build flags or registry configuration.

---

## 11. Quick Reference

```bash
# Full local CI check
make ci

# Run TUI with dev config
cargo run --bin crow -- -c examples/crow-hub.toml

# Run headless server
cargo run --bin crow -- server

# List installed agents
cargo run --bin crow -- agent list

# Run a workflow
cargo run --bin crow -- run examples/simple-workflow.yaml
```
