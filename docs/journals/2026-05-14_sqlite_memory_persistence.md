# SQLite Memory Persistence

**Date:** 2026-05-14
**Author:** Antigravity (AI Agent)
**Component:** ch-memory, ch-core, ch-tui

## Overview
Successfully implemented the Day 3 milestones of the four-day plan for the Crow Hub project. This phase focused on building a durable, persistent storage layer using SQLite for the message bus. While semantic embeddings and vector search are reserved for future milestones, having a data-plane persistence layer unlocks basic capabilities like retrieving a recent history by channel or retrieving full conversation threads by correlation ID.

## Key Changes

1. **Integrated `sqlx` into `ch-memory`:**
   - Updated `ch-memory/Cargo.toml` with `sqlx` utilizing `sqlite`, `chrono`, and `runtime-tokio-rustls` features.
   - Refactored `ch-memory/src/backends/sqlite.rs` away from the temporary in-memory placeholder to use `sqlx::SqlitePool`.

2. **Database Schema & Initialization:**
   - Standardized the SQLite path to `~/.crow-hub/messages.db` via `get_home_dir()`, respecting standard user directories. 
   - Initialized a comprehensive schema to track agent communications, capturing `message_id`, `correlation_id`, `from_agent`, `channel`, `message_type`, `content`, `embedding` (reserved BLOB), `metadata`, and `created_at`.
   - Setup appropriate indexes on `correlation_id`, `created_at`, and `channel` for efficient querying.

3. **Memory Store API Enhancements:**
   - Extended the `MemoryStore` trait with `recent(channel, limit)` and `by_correlation(id)` methods to support fetching data without full text or semantic search.
   - Migrated the returned entries to correctly map from SQLite rows back into the `MemoryEntry` structure, stashing non-standard properties directly into the `metadata` JSON blob.

4. **Background Memory Writer (`spawn_memory_writer`):**
   - Implemented a background task inside `ch-memory/src/writer.rs` that attaches an `AgentId` subscriber to the Message Bus.
   - This task listens passively on the `general` channel (using `ChannelVisibility::Full`), filtering for text payloads of type `TaskRequest` or `TaskResponse` and saving them via `store.write()`.

5. **Application Integration:**
   - Wired the instantiation of `SqliteMemoryStore` and `spawn_memory_writer` natively into both the TUI interface (`run_tui`) and the headless server daemon (`run_server`) within `crates/ch-tui/src/main.rs`.

## Validation & Status
- The test suite within `crates/ch-memory/src/backends/sqlite.rs` successfully passed, completing full round-trip write/read operations and verifying result sorting limit properties of `recent`.
- Running `cargo run --bin crow` functions without errors and naturally triggers the database parent directory creation if it doesn't already exist.

## Next Steps
The core memory layer is now tracking messages in SQLite behind the scenes. This paves the way for Day 4, which involves surfacing token/cost metrics to the TUI to monitor agent operation costs. In subsequent iterations, semantic retrieval (embeddings) will be integrated directly on top of this foundation.
