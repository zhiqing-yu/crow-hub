# Crow Hub - System Architecture

> A high-performance AI Agent Orchestration Hub built in Rust.
> Universal middleware for multi-agent communication, scheduling, memory, and monitoring.

## 1. High-Level Overview

```mermaid
graph TB
    subgraph "User Interfaces"
        TUI["TUI (Ratatui)<br/>Phase 5 - Binary: crow"]
        GUI["GUI (Tauri)<br/>Phase 6 - Binary: crow-gui"]
    end

    subgraph "Core Engine"
        ORCH["Orchestrator<br/>Workflow & Task Execution"]
        BUS["Message Bus<br/>Pub/Sub + Request/Response"]
        REG["Agent Registry<br/>Registration & Discovery"]
        SES["Session Manager<br/>Multi-Agent Collaboration"]
        CFG["Configuration<br/>Figment-based Config"]
    end

    subgraph "Extension Layers"
        ADAPTER["Adapter System<br/>5 Agent Adapters"]
        MEMORY["Memory Layer<br/>Vector Store + Embedding"]
        MONITOR["Monitor System<br/>Metrics & Observability"]
    end

    subgraph "Protocol Layer"
        PROTO["ch-protocol<br/>Message Types & Contracts"]
    end

    TUI --> CORE["ch-core"]
    GUI --> CORE
    CORE --> PROTO
    ADAPTER --> PROTO
    ADAPTER --> CORE
    MEMORY --> PROTO
    MONITOR --> PROTO
    MONITOR --> CORE
```

## 2. Workspace Crate Dependency Graph

```mermaid
graph LR
    PROTO["ch-protocol<br/>(Zero dependencies)"]
    CORE["ch-core"]
    ADAPTER["ch-adapter"]
    MEMORY["ch-memory"]
    MONITOR["ch-monitor"]
    TUI["ch-tui<br/>(binary: crow)"]
    GUI["ch-gui<br/>(binary: crow-gui)"]

    CORE --> PROTO
    ADAPTER --> PROTO
    ADAPTER --> CORE
    MEMORY --> PROTO
    MONITOR --> PROTO
    MONITOR --> CORE
    TUI --> PROTO
    TUI --> CORE
    TUI --> ADAPTER
    TUI --> MEMORY
    TUI --> MONITOR
    GUI --> PROTO
    GUI --> CORE

    style PROTO fill:#e1f5fe
    style CORE fill:#c8e6c9
    style ADAPTER fill:#fff3e0
    style MEMORY fill:#f3e5f5
    style MONITOR fill:#fce4ec
```

## 3. Crate Inventory

| Crate | Type | Binary | Purpose | Phase |
|-------|------|--------|---------|-------|
| `ch-protocol` | library | - | Core message types & communication protocol | Phase 1 |
| `ch-core` | library | - | Message bus, agent registry, session manager, orchestrator | Phase 1 |
| `ch-adapter` | library | - | Unified adapter trait + 5 adapter implementations | Phase 2 |
| `ch-memory` | library | - | Pluggable vector memory with semantic search | Phase 3 |
| `ch-monitor` | library | - | Token usage, performance, resource monitoring | Phase 4 |
| `ch-tui` | binary | `crow` | Terminal UI with Ratatui + Clap CLI | Phase 5 |
| `ch-gui` | binary | `crow-gui` | Desktop GUI with Tauri (placeholder) | Phase 6 |

## 4. Core Types (ch-protocol)

This is the foundation layer with zero internal crate dependencies.

```mermaid
classDiagram
    class AgentId {
        +Uuid inner
        +new() AgentId
    }

    class AgentAddress {
        +AgentId agent_id
        +String agent_name
        +String adapter_type
        +new(name, adapter_type) AgentAddress
    }

    class AgentMessage {
        +Uuid message_id
        +DateTime timestamp
        +Option~Uuid~ correlation_id
        +AgentAddress from
        +Option~AgentAddress~ to
        +MessageType message_type
        +Payload payload
        +String session_id
        +Vec~String~ memory_context
        +Priority priority
        +Option~u32~ ttl
        +new(from, to, type, payload) AgentMessage
        +with_correlation(id) AgentMessage
        +with_session(id) AgentMessage
        +is_expired() bool
    }

    class MessageType {
        <<enumeration>>
        TaskRequest
        TaskResponse
        TaskDelegate
        CollabInvite
        CollabJoin
        MemoryShare
        StatusHeartbeat
        StatusMetrics
        Custom(String)
    }

    class Payload {
        <<enumeration>>
        Text(String)
        Data(Value)
        Task(TaskSpec)
        Result(TaskResult)
        Status(AgentStatus)
        Metrics(MetricsData)
        Memory(MemoryEntry)
        Empty
    }

    class TaskSpec {
        +String task_id
        +String description
        +Vec~String~ requirements
        +Option~DateTime~ deadline
        +Vec~String~ dependencies
        +HashMap metadata
    }

    class AgentStatus {
        +AgentId agent_id
        +AgentState state
        +Option~String~ current_task
        +usize queue_depth
        +HealthStatus health
    }

    class MetricsData {
        +AgentId agent_id
        +TokenMetrics token_metrics
        +PerformanceMetrics performance_metrics
        +ResourceMetrics resource_metrics
    }

    AgentMessage --> AgentAddress
    AgentMessage --> MessageType
    AgentMessage --> Payload
    Payload --> TaskSpec
    Payload --> TaskResult
    Payload --> AgentStatus
    Payload --> MetricsData
    AgentAddress --> AgentId
    AgentStatus --> AgentId
    MetricsData --> AgentId
```

## 5. Core Engine (ch-core)

### 5.1 CrowHub - Main Entry Point

```mermaid
classDiagram
    class CrowHub {
        +Arc~HubConfig~ config
        +Arc~MessageBus~ bus
        +Arc~AgentRegistry~ registry
        +Arc~SessionManager~ sessions
        +Arc~Orchestrator~ orchestrator
        +new(config) CrowHub
        +start() Result~()~
        +shutdown() Result~()~
    }

    CrowHub --> MessageBus
    CrowHub --> AgentRegistry
    CrowHub --> SessionManager
    CrowHub --> Orchestrator
```

### 5.2 Message Bus

```mermaid
sequenceDiagram
    participant A as Agent A
    participant BUS as MessageBus
    participant B as Agent B
    participant MON as Monitor

    A->>BUS: subscribe(agent_id_A)
    B->>BUS: subscribe(agent_id_B)
    MON->>BUS: subscribe_broadcast()

    A->>BUS: publish(msg to B)
    BUS-->>MON: broadcast(msg)
    BUS-->>B: deliver(msg)
    
    Note over A,B: Request-Response Pattern
    A->>BUS: request(msg, timeout)
    BUS-->>B: deliver(request)
    B->>BUS: publish(response)
    BUS-->>A: deliver(response)
```

### 5.3 Orchestrator - Workflow Execution

```mermaid
stateDiagram-v2
    [*] --> Idle
    Idle --> Running: Start
    Running --> Paused: Pause
    Paused --> Running: Resume
    Running --> ShuttingDown: Shutdown
    Paused --> ShuttingDown: Shutdown
    ShuttingDown --> [*]
    
    Running --> Running: ExecuteWorkflow
```

## 6. Adapter System (ch-adapter)

### 6.1 Adapter Trait

```mermaid
classDiagram
    class AgentAdapter {
        <<trait>>
        +init(config) Result~()~
        +chat(messages, tools) Result~Response~
        +stream(messages) Result~Stream~
        +status() Result~AgentStatus~
        +health_check() Result~HealthStatus~
        +capabilities() Vec~Capability~
        +name() &str
        +adapter_type() &str
    }

    class ClaudeAdapter {
        +Client client
        +String api_key
        +String base_url
        +String model
    }

    class KimiAdapter {
        +Client client
        +String api_key
        +String base_url
        +String model
    }

    class GeminiAdapter {
        +Client client
        +String api_key
        +String base_url
        +String model
    }

    class HermesAdapter {
        +Client client
        +String endpoint
        +String transport
    }

    class CodeBuddyAdapter {
        +Client client
        +String endpoint
    }

    AgentAdapter <|.. ClaudeAdapter
    AgentAdapter <|.. KimiAdapter
    AgentAdapter <|.. GeminiAdapter
    AgentAdapter <|.. HermesAdapter
    AgentAdapter <|.. CodeBuddyAdapter
```

### 6.2 Adapter Factory Pattern

```mermaid
flowchart LR
    Client["Client Code"] -->|create("claude")| Factory["AdapterFactory"]
    Factory -->|Box<dyn AgentAdapter>| Claude["ClaudeAdapter"]
    Factory -->|Box<dyn AgentAdapter>| Kimi["KimiAdapter"]
    Factory -->|Box<dyn AgentAdapter>| Gemini["GeminiAdapter"]
    Factory -->|Box<dyn AgentAdapter>| Hermes["HermesAdapter"]
    Factory -->|Box<dyn AgentAdapter>| CodeBuddy["CodeBuddyAdapter"]
```

## 7. Memory Layer (ch-memory)

### 7.1 Memory Store Architecture

```mermaid
classDiagram
    class MemoryStore {
        <<trait>>
        +init() Result~()~
        +write(memory) Result~String~
        +read(memory_id) Result~MemoryEntry~
        +search(query, filter, top_k) Result~Vec~MemoryEntry~~
        +get_session_context(session_id, limit) Result~Vec~MemoryEntry~~
        +get_agent_memories(agent_id, limit) Result~Vec~MemoryEntry~~
        +update(memory_id, content) Result~()~
        +delete(memory_id) Result~()~
        +export(filter, format) Result~Vec~u8~~
        +import(data, format) Result~ImportResult~
        +count() Result~usize~
        +clear() Result~()~
        +close() Result~()~
    }

    class SqliteMemoryStore {
        +SqliteConfig config
        +Arc~RwLock~Vec~MemoryEntry~~ data
        -embed(content) Result~Vec~f32~~
        -cosine_similarity(a, b) f32
    }

    class Embedder {
        <<trait>>
        +embed(text) Result~Vec~f32~~
        +embed_batch(texts) Result~Vec~Vec~f32~~~
        +dimension() usize
    }

    class LocalEmbedder {
        +usize dimension
    }

    class MemoryManager {
        +HashMap~String, Box~MemoryStore~~ stores
        +String default_store
        +register(name, store)
        +get(name) Option~&MemoryStore~
        +default_store() Option~&MemoryStore~
        +create_store(backend) Result~Box~MemoryStore~~
    }

    MemoryStore <|.. SqliteMemoryStore
    Embedder <|.. LocalEmbedder
    SqliteMemoryStore --> Embedder : uses
    MemoryManager --> MemoryStore : manages
    MemoryManager --> MemoryBackend : creates from
```

### 7.2 Supported Backends

```mermaid
graph TD
    MM["MemoryManager"] -->|Default| SQLITE["SQLite (embedded)<br/>Zero external dependencies"]
    MM -->|Optional| CHROMA["ChromaDB<br/>Lightweight vector DB"]
    MM -->|Optional| QDRANT["Qdrant<br/>High-performance"]
    MM -->|Optional| MILVUS["Milvus<br/>Large-scale"]
    MM -->|Optional| PGVEC["PgVector<br/>PostgreSQL"]

    style SQLITE fill:#c8e6c9
    style CHROMA fill:#fff9c4
    style QDRANT fill:#fff9c4
    style MILVUS fill:#fff9c4
    style PGVEC fill:#fff9c4
```

## 8. Monitor System (ch-monitor)

### 8.1 Metrics Collection Pipeline

```mermaid
flowchart LR
    AGENTS["Agent Adapters"] -->|record_tokens| MON["Monitor"]
    AGENTS -->|record_performance| MON
    AGENTS -->|record_resources| MON
    AGENTS -->|record_request| MON
    
    MON -->|DashMap| CURRENT["Current Metrics<br/>(per agent)"]
    MON -->|AgentMetricsHistory| HISTORY["Historical Metrics<br/>(ring buffer)"]
    
    CURRENT --> SNAPSHOT["MetricsSnapshot"]
    HISTORY --> SNAPSHOT
    
    SNAPSHOT --> PROM["Prometheus Exporter"]
    SNAPSHOT --> CONSOLE["Console Exporter"]
    
    PROM -->|HTTP :9090/metrics| GRAFANA["Grafana / Prometheus"]
```

### 8.2 Metrics Hierarchy

```mermaid
classDiagram
    class MetricsSnapshot {
        +DateTime timestamp
        +Vec~AgentMetrics~ agents
        +SystemMetrics system
    }

    class AgentMetrics {
        +String agent_id
        +String agent_name
        +String adapter_type
        +TokenMetrics tokens
        +PerformanceMetrics performance
        +ResourceMetrics resources
        +u64 requests_total
        +u64 errors_total
        +f64 latency_avg_ms
    }

    class SystemMetrics {
        +usize total_agents
        +usize active_agents
        +u64 total_tokens
        +f64 total_cost_usd
        +f64 requests_per_second
        +f32 cpu_usage_percent
        +u64 memory_usage_mb
    }

    class TokenMetrics {
        +u64 input_tokens
        +u64 output_tokens
        +u64 total_tokens
        +f64 tokens_per_second
        +f64 cost_usd
    }

    class PerformanceMetrics {
        +u32 ttft_ms
        +f64 throughput_tps
        +u32 latency_p50_ms
        +u32 latency_p99_ms
    }

    class ResourceMetrics {
        +f32 cpu_usage_percent
        +u64 memory_usage_mb
        +Option~f32~ gpu_usage_percent
        +Option~u64~ gpu_memory_usage_mb
        +Option~f32~ kv_cache_usage
    }

    MetricsSnapshot --> AgentMetrics
    MetricsSnapshot --> SystemMetrics
    AgentMetrics --> TokenMetrics
    AgentMetrics --> PerformanceMetrics
    AgentMetrics --> ResourceMetrics
```

## 9. Session Management

```mermaid
stateDiagram-v2
    [*] --> Created: create(config)
    Created --> Active: start()
    Active --> Paused: pause()
    Paused --> Active: resume()
    Active --> Completed: next_round() >= max_rounds
    Active --> Failed: error
    Failed --> [*]
    Completed --> [*]
    
    state Active {
        [*] --> Round1: round 1
        Round1 --> Round2: next_round()
        Round2 --> Round3: next_round()
        Round3 --> [*]: max_rounds reached
    }
```

## 10. Development Roadmap

```mermaid
gantt
    title Crow Hub Development Roadmap (24 Weeks)
    dateFormat  YYYY-MM-DD
    axisFormat  %Y-%m

    section Foundation
    Phase 0: Project Init     :p0, 2026-04-13, 14d
    Phase 1: Core Engine      :p1, after p0, 21d

    section Adapters
    Phase 2: Adapter System   :p2, after p1, 28d

    section Intelligence
    Phase 3: Memory Layer     :p3, after p2, 21d

    section Observability
    Phase 4: Monitor System   :p4, after p3, 14d

    section Interfaces
    Phase 5: TUI              :p5, after p4, 21d
    Phase 6: GUI              :p6, after p5, 35d

    section Release
    Phase 7: Testing & Release:p7, after p6, 14d
```

## 11. Technology Stack

| Component | Technology | Purpose |
|-----------|-----------|---------|
| Language | Rust 1.86+ | Safety, performance, concurrency |
| Async Runtime | Tokio | Event-driven I/O |
| Serialization | Serde + JSON | Message serialization |
| HTTP Client | Reqwest | API calls to agents |
| gRPC | Tonic | Future inter-process communication |
| Configuration | Figment | Multi-source config (TOML + ENV) |
| CLI | Clap v4 | Command-line argument parsing |
| Logging | Tracing | Structured logging & observability |
| Error Handling | thiserror + anyhow | Typed errors + flexible errors |
| Concurrency | DashMap + parking_lot | Lock-free concurrent maps |
| TUI Framework | Ratatui | Terminal user interface |
| GUI Framework | Tauri | Cross-platform desktop GUI |
| Testing | Mockall + tokio-test | Mocking & async testing |

## 12. Key Design Decisions

1. **Protocol-first design**: `ch-protocol` has zero internal dependencies, ensuring all crates share the same message contracts.
2. **Trait-based extensibility**: `AgentAdapter`, `MemoryStore`, `Collector`, `Exporter` traits enable hot-pluggable implementations.
3. **DashMap for concurrency**: Lock-free concurrent maps avoid global locks for agent registry and metrics.
4. **Broadcast + direct delivery**: Message bus uses broadcast channel for monitors and direct mpsc for targeted delivery.
5. **Local-first memory**: Default SQLite backend with local embedding model requires zero external services.
6. **Factory pattern for adapters**: `AdapterFactory` provides runtime adapter selection by type string.
