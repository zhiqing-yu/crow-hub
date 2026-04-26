# Crow Hub - Rust 开发路线图

> 基于Rust的高性能Agent调度中枢

## 🎯 项目定位

- **核心语言**: Rust（安全、高性能、跨平台）
- **目标平台**: ARM64/x86 - macOS/Windows/Linux
- **优先架构**: NVIDIA GPU + Apple Silicon
- **开发顺序**: 底层核心 → TUI → GUI

---

## 📊 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                         GUI (Phase 6)                           │
│                   (Tauri + React/Vue)                           │
├─────────────────────────────────────────────────────────────────┤
│                         TUI (Phase 5)                           │
│                   (Ratatui - 终端界面)                          │
├─────────────────────────────────────────────────────────────────┤
│                         Core (Rust)                             │
│  ┌─────────────┬─────────────┬─────────────┬─────────────┐     │
│  │   Adapter   │   Memory    │   Monitor   │   Message   │     │
│  │   System    │    Layer    │   System    │    Bus      │     │
│  │  (Phase 2)  │  (Phase 3)  │  (Phase 4)  │  (Phase 1)  │     │
│  └─────────────┴─────────────┴─────────────┴─────────────┘     │
└─────────────────────────────────────────────────────────────────┘
```

---

## 🛤️ 详细开发路线图

### Phase 0: 项目初始化与架构设计 (Week 1-2)

**目标**: 搭建项目骨架，确定技术选型

```rust
// 项目结构
crow-hub/
├── Cargo.toml                    # Workspace配置
├── Cargo.lock
├── rust-toolchain.toml          # Rust版本锁定
├── .github/
│   └── workflows/
│       ├── ci.yml               # CI/CD
│       └── release.yml          # 自动发布
│
├── crates/                      # 多crate工作区
│   ├── ch-core/                 # 核心库
│   ├── ch-adapter/              # 适配器系统
│   ├── ch-memory/               # 记忆层
│   ├── ch-monitor/              # 监控系统
│   ├── ch-protocol/             # 通讯协议
│   ├── ch-tui/                  # TUI界面
│   └── ch-gui/                  # GUI界面
│
├── adapters/                    # 适配器实现
│   ├── claude/
│   ├── kimi/
│   ├── gemini/
│   ├── hermes/
│   └── codebuddy/
│
├── docs/
├── examples/
└── scripts/
    ├── build.sh                 # 跨平台构建
    └── install.sh               # 安装脚本
```

**技术选型**:

| 组件 | 选择 | 理由 |
|------|------|------|
| 异步运行时 | Tokio | 生态成熟，性能优秀 |
| 序列化 | Serde + JSON | 标准选择 |
| HTTP客户端 | Reqwest | 基于Hyper，功能完善 |
| gRPC | Tonic | Rust原生gRPC |
| 配置 | Figment | 多源配置合并 |
| CLI | Clap v4 | 强大的命令行解析 |
| 日志 | Tracing | 结构化日志，可观测性 |
| 错误处理 | Thiserror + Anyhow | 标准组合 |
| 测试 | Cargo test + Mockall | 单元+集成测试 |

**跨平台构建**:

```yaml
# .github/workflows/release.yml
strategy:
  matrix:
    include:
      # macOS
      - target: x86_64-apple-darwin
        os: macos-latest
      - target: aarch64-apple-darwin  # Apple Silicon
        os: macos-latest
      
      # Linux
      - target: x86_64-unknown-linux-gnu
        os: ubuntu-latest
      - target: aarch64-unknown-linux-gnu  # ARM64 Linux
        os: ubuntu-latest
      
      # Windows
      - target: x86_64-pc-windows-msvc
        os: windows-latest
      - target: aarch64-pc-windows-msvc    # ARM64 Windows
        os: windows-latest
```

**里程碑**:
- [x] 项目脚手架搭建
- [x] CI/CD流水线配置
- [x] 跨平台编译验证
- [x] 架构文档完成

**已完成文件**:
| 类别 | 文件 | 说明 |
|------|------|------|
| 工作区 | `Cargo.toml` | Workspace 配置 |
| 工作区 | `rust-toolchain.toml` | Rust 工具链配置 |
| 工作区 | `Makefile` | 开发命令 |
| 协议 | `crates/ah-protocol/src/lib.rs` | 消息协议定义 |
| 协议 | `crates/ah-protocol/src/error.rs` | 错误类型 |
| 协议 | `crates/ah-protocol/src/types.rs` | 协议类型 |
| 核心 | `crates/ah-core/src/lib.rs` | 核心引擎 |
| 核心 | `crates/ah-core/src/bus.rs` | 消息总线 |
| 核心 | `crates/ah-core/src/config.rs` | 配置管理 |
| 核心 | `crates/ah-core/src/registry.rs` | Agent 注册表 |
| 核心 | `crates/ah-core/src/session.rs` | 会话管理 |
| 核心 | `crates/ah-core/src/orchestrator.rs` | 编排引擎 |
| 适配器 | `crates/ah-adapter/src/lib.rs` | 适配器 trait |
| 适配器 | `crates/ah-adapter/src/adapters/*.rs` | 5个适配器实现 |
| 记忆 | `crates/ah-memory/src/lib.rs` | 记忆层 trait |
| 记忆 | `crates/ah-memory/src/backends/sqlite.rs` | SQLite 实现 |
| 记忆 | `crates/ah-memory/src/embedder/local.rs` | 本地嵌入模型 |
| 监控 | `crates/ah-monitor/src/lib.rs` | 监控系统 |
| 监控 | `crates/ah-monitor/src/collectors/*.rs` | 指标采集器 |
| 监控 | `crates/ah-monitor/src/exporters/*.rs` | 指标导出器 |
| TUI | `crates/ah-tui/src/main.rs` | TUI 入口 |
| GUI | `crates/ah-gui/src/main.rs` | GUI 入口 |
| CI/CD | `.github/workflows/ci.yml` | CI 配置 |
| CI/CD | `.github/workflows/release.yml` | 发布配置 |
| 示例 | `examples/agenthub.toml` | 配置示例 |
| 示例 | `examples/simple-workflow.yaml` | 工作流示例 |
| 文档 | `docs/ARCHITECTURE.md` | 架构设计文档 |
| 文档 | `README.md` | 项目说明 |

---

### Phase 1: 核心引擎 - 消息总线与协议 (Week 3-5)

**目标**: 实现Agent间通讯的基础设施

```rust
// crates/ch-protocol/src/lib.rs

/// 统一消息格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub message_id: Uuid,
    pub timestamp: u64,
    pub correlation_id: Option<Uuid>,
    pub from: AgentAddress,
    pub to: AgentAddress,
    pub message_type: MessageType,
    pub payload: Payload,
    pub session_id: String,
    pub memory_context: Vec<String>,
}

/// 消息总线 - 核心组件
pub struct MessageBus {
    subscribers: DashMap<String, mpsc::Sender<AgentMessage>>,
    history: Arc<RwLock<Vec<AgentMessage>>>,
}

impl MessageBus {
    /// 发布消息
    pub async fn publish(&self, message: AgentMessage) -> Result<()>;
    
    /// 订阅消息
    pub async fn subscribe(&self, agent_id: String) -> mpsc::Receiver<AgentMessage>;
    
    /// 请求-响应模式
    pub async fn request(&self, message: AgentMessage, timeout: Duration) -> Result<AgentMessage>;
}
```

**关键实现**:

| 模块 | 功能 | 技术点 |
|------|------|--------|
| `ch-protocol` | 消息定义、序列化 | Protobuf/JSON双支持 |
| `ch-core` | 消息总线、Session管理 | Tokio mpsc广播 |
| `ch-core` | 编排引擎 | 状态机、工作流图 |

**API设计**:

```rust
// 核心API预览
pub struct AgentHub {
    bus: Arc<MessageBus>,
    registry: Arc<AgentRegistry>,
    memory: Arc<dyn MemoryStore>,
    monitor: Arc<Monitor>,
}

impl AgentHub {
    /// 注册Agent
    pub async fn register_agent(&self, config: AgentConfig) -> Result<AgentHandle>;
    
    /// 发送消息
    pub async fn send_message(&self, message: AgentMessage) -> Result<()>;
    
    /// 创建会话
    pub async fn create_session(&self, agents: Vec<String>) -> Result<Session>;
    
    /// 运行工作流
    pub async fn run_workflow(&self, workflow: Workflow) -> Result<WorkflowResult>;
}
```

**里程碑**:
- [ ] 消息总线实现
- [ ] Session管理
- [ ] 基础编排逻辑
- [ ] 单元测试覆盖 >80%

---

### Phase 2: 适配器系统 - 5个核心适配器 (Week 6-9)

**目标**: 实现5个核心Agent适配器

```rust
// crates/ah-adapter/src/lib.rs

/// 适配器trait - 所有Agent必须实现
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    /// 初始化适配器
    async fn init(&mut self, config: AdapterConfig) -> Result<()>;
    
    /// 发送消息并获取响应
    async fn chat(&self, messages: Vec<Message>, tools: Option<Vec<Tool>>) -> Result<Response>;
    
    /// 流式响应
    async fn stream(&self, messages: Vec<Message>) -> Result<BoxStream<'static, Chunk>>;
    
    /// 获取Agent状态
    async fn status(&self) -> Result<AgentStatus>;
    
    /// 健康检查
    async fn health_check(&self) -> Result<HealthStatus>;
}

/// 适配器工厂
pub struct AdapterFactory;

impl AdapterFactory {
    pub fn create(adapter_type: &str) -> Result<Box<dyn AgentAdapter>> {
        match adapter_type {
            "claude" => Ok(Box::new(ClaudeAdapter::new())),
            "kimi" => Ok(Box::new(KimiAdapter::new())),
            "gemini" => Ok(Box::new(GeminiAdapter::new())),
            "hermes" => Ok(Box::new(HermesAdapter::new())),
            "codebuddy" => Ok(Box::new(CodeBuddyAdapter::new())),
            _ => Err(Error::UnknownAdapter(adapter_type.to_string())),
        }
    }
}
```

**适配器实现计划**:

| 优先级 | 适配器 | 协议 | 难度 | 预计时间 |
|--------|--------|------|------|----------|
| P0 | Claude | HTTP REST | 低 | 3天 |
| P0 | Kimi | OpenAI兼容 | 低 | 2天 |
| P0 | Gemini | HTTP REST | 低 | 2天 |
| P1 | Hermes | gRPC/HTTP | 中 | 5天 |
| P1 | CodeBuddy | WebSocket | 中 | 5天 |

**Claude适配器示例**:

```rust
// adapters/claude/src/lib.rs

pub struct ClaudeAdapter {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

#[async_trait]
impl AgentAdapter for ClaudeAdapter {
    async fn chat(&self, messages: Vec<Message>, tools: Option<Vec<Tool>>) -> Result<Response> {
        let request = AnthropicRequest {
            model: self.model.clone(),
            messages: convert_messages(messages),
            tools: tools.map(convert_tools),
            max_tokens: 4096,
        };
        
        let response = self.client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .json(&request)
            .send()
            .await?
            .json::<AnthropicResponse>()
            .await?;
            
        Ok(convert_response(response))
    }
    
    async fn status(&self) -> Result<AgentStatus> {
        // 返回token用量、速率限制等
        Ok(AgentStatus {
            tokens_used: self.metrics.tokens_total(),
            rate_limit_remaining: self.rate_limit.remaining(),
            latency_ms: self.metrics.avg_latency(),
        })
    }
}
```

**里程碑**:
- [ ] Claude适配器完成
- [ ] Kimi适配器完成
- [ ] Gemini适配器完成
- [ ] Hermes适配器完成
- [ ] CodeBuddy适配器完成
- [ ] 适配器热插拔机制

---

### Phase 3: 共享记忆层 - 向量数据库 (Week 10-12)

**目标**: 实现可插拔的向量记忆系统

```rust
// crates/ah-memory/src/lib.rs

/// 记忆存储trait
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// 写入记忆
    async fn write(&self, memory: Memory) -> Result<String>;
    
    /// 语义搜索
    async fn search(&self, query: &str, filters: MemoryFilter, top_k: usize) -> Result<Vec<Memory>>;
    
    /// 获取会话上下文
    async fn get_context(&self, session_id: &str, window: usize) -> Result<Vec<Memory>>;
    
    /// 导出记忆
    async fn export(&self, format: ExportFormat) -> Result<Vec<u8>>;
    
    /// 导入记忆
    async fn import(&self, data: &[u8], format: ExportFormat) -> Result<ImportResult>;
}

/// 向量数据库后端枚举
pub enum VectorBackend {
    Chroma(ChromaConfig),       // 本地轻量
    Qdrant(QdrantConfig),       // 高性能
    Milvus(MilvusConfig),       // 大规模
    PgVector(PgVectorConfig),   // PostgreSQL
    Sqlite(SqliteConfig),       // 嵌入式
}
```

**默认实现 - SQLite + 本地嵌入**:

```rust
/// SQLite + Rust原生嵌入（零依赖）
pub struct SqliteMemoryStore {
    db: SqlitePool,
    embedder: Arc<dyn Embedder>,
}

impl SqliteMemoryStore {
    pub async fn new(path: &Path) -> Result<Self> {
        // 使用rusqlite + sqlite-vec扩展
        let db = SqlitePool::connect(path).await?;
        
        // 初始化向量表
        sqlx::query(
            r#"
            CREATE VIRTUAL TABLE IF NOT EXISTS memories USING vec0(
                embedding FLOAT[768],
                agent_id TEXT,
                session_id TEXT,
                content TEXT,
                metadata JSON,
                created_at INTEGER
            )
            "#
        ).execute(&db).await?;
        
        Ok(Self { db, embedder: Arc::new(LocalEmbedder::new()) })
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    async fn write(&self, memory: Memory) -> Result<String> {
        let embedding = self.embedder.embed(&memory.content).await?;
        let id = Uuid::new_v4().to_string();
        
        sqlx::query(
            r#"
            INSERT INTO memories (id, embedding, agent_id, session_id, content, metadata, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#
        )
        .bind(&id)
        .bind(&embedding)
        .bind(&memory.agent_id)
        .bind(&memory.session_id)
        .bind(&memory.content)
        .bind(&memory.metadata)
        .bind(memory.created_at)
        .execute(&self.db)
        .await?;
        
        Ok(id)
    }
    
    async fn search(&self, query: &str, filters: MemoryFilter, top_k: usize) -> Result<Vec<Memory>> {
        let query_embedding = self.embedder.embed(query).await?;
        
        let memories = sqlx::query_as::<_, Memory>(
            r#"
            SELECT id, agent_id, session_id, content, metadata, created_at,
                   vec_distance_L2(embedding, ?1) as distance
            FROM memories
            WHERE agent_id IN (?2) AND session_id IN (?3)
            ORDER BY distance
            LIMIT ?4
            "#
        )
        .bind(&query_embedding)
        .bind(&filters.agent_ids)
        .bind(&filters.session_ids)
        .bind(top_k as i64)
        .fetch_all(&self.db)
        .await?;
        
        Ok(memories)
    }
}
```

**里程碑**:
- [ ] MemoryStore trait定义
- [ ] SQLite + 本地嵌入实现
- [ ] ChromaDB适配
- [ ] 记忆导入导出
- [ ] 与消息总线集成

---

### Phase 4: 监控与指标系统 (Week 13-14)

**目标**: 全链路可观测性

```rust
// crates/ah-monitor/src/lib.rs

/// 监控中心
pub struct Monitor {
    metrics: Arc<RwLock<MetricsStore>>,
    exporters: Vec<Box<dyn MetricsExporter>>,
}

impl Monitor {
    /// 记录Token用量
    pub async fn record_tokens(&self, agent_id: &str, input: u32, output: u32);
    
    /// 记录延迟
    pub async fn record_latency(&self, agent_id: &str, operation: &str, duration: Duration);
    
    /// 记录性能指标
    pub async fn record_performance(&self, agent_id: &str, perf: PerformanceMetrics);
    
    /// 获取实时指标
    pub async fn get_metrics(&self, agent_id: Option<&str>) -> MetricsSnapshot;
}

/// 指标类型
#[derive(Debug, Clone)]
pub struct AgentMetrics {
    pub agent_id: String,
    pub tokens: TokenMetrics,
    pub performance: PerformanceMetrics,
    pub resources: ResourceMetrics,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct TokenMetrics {
    pub input_total: u64,
    pub output_total: u64,
    pub cost_usd: f64,
    pub tokens_per_second: f32,
}

#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub ttft_ms: u32,              // Time To First Token
    pub throughput_tps: f32,       // Tokens Per Second
    pub kv_cache_usage: f32,       // KV Cache使用率
    pub prefill_tps: f32,          // Prefill速度
    pub generation_tps: f32,       // Generation速度
}
```

**本地Agent性能采集**:

```rust
/// 通过nvidia-smi或rocm-smi采集GPU指标
pub struct GpuMetricsCollector;

impl GpuMetricsCollector {
    pub async fn collect(&self) -> Result<GpuMetrics> {
        #[cfg(target_os = "macos")]
        {
            // macOS使用powermetrics或自定义实现
            self.collect_apple_silicon().await
        }
        
        #[cfg(target_os = "linux")]
        {
            // Linux使用nvidia-smi或rocm-smi
            self.collect_nvidia().await
        }
    }
    
    async fn collect_nvidia(&self) -> Result<GpuMetrics> {
        let output = Command::new("nvidia-smi")
            .args(&["--query-gpu=memory.used,memory.total,utilization.gpu", "--format=csv,noheader,nounits"])
            .output()
            .await?;
            
        // 解析输出...
    }
}
```

**里程碑**:
- [ ] 指标采集系统
- [ ] Token成本计算
- [ ] GPU性能监控（NVIDIA/Apple Silicon）
- [ ] Prometheus导出
- [ ] HTTP API暴露指标

---

### Phase 5: TUI终端界面 (Week 15-17)

**目标**: 终端交互界面

```rust
// crates/ah-tui/src/lib.rs

use ratatui::{
    backend::CrosstermBackend,
    widgets::{Block, Borders, List, Paragraph},
    Terminal,
};

pub struct App {
    /// Agent列表
    agents: Vec<AgentInfo>,
    /// 消息历史
    messages: Vec<DisplayMessage>,
    /// 当前输入
    input: String,
    /// 选中标签页
    current_tab: Tab,
    /// 监控数据
    metrics: MetricsSnapshot,
}

impl App {
    pub async fn run(&mut self) -> Result<()> {
        let backend = CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;
        
        loop {
            // 绘制UI
            terminal.draw(|f| self.ui(f))?;
            
            // 处理事件
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Tab => self.next_tab(),
                    KeyCode::Enter => self.send_message().await?,
                    _ => self.handle_input(key),
                }
            }
            
            // 刷新监控数据
            self.update_metrics().await;
        }
        
        Ok(())
    }
    
    fn ui(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // 标题
                Constraint::Min(0),     // 主内容
                Constraint::Length(3),  // 输入框
            ])
            .split(frame.area());
        
        // 标题栏
        frame.render_widget(
            Paragraph::new("🌐 Agent Hub").block(Block::default().borders(Borders::ALL)),
            chunks[0]
        );
        
        // 主内容区 - 根据标签页切换
        match self.current_tab {
            Tab::Agents => self.render_agents(frame, chunks[1]),
            Tab::Chat => self.render_chat(frame, chunks[1]),
            Tab::Monitor => self.render_monitor(frame, chunks[1]),
            Tab::Memory => self.render_memory(frame, chunks[1]),
        }
        
        // 输入框
        frame.render_widget(
            Paragraph::new(self.input.clone()).block(Block::default().borders(Borders::ALL).title("输入")),
            chunks[2]
        );
    }
}
```

**TUI界面预览**:

```
┌─────────────────────────────────────────────────────────────────┐
│ 🌐 Agent Hub v0.1.0           [Agents] [Chat] [Monitor] [Memory]│
├─────────────────────────────────────────────────────────────────┤
│ ┌──────────────────┐  ┌─────────────────────────────────────┐  │
│ │ 🤖 Agents        │  │ 💬 Chat                             │  │
│ │                  │  │                                     │  │
│ │ 🟢 claude-pro    │  │ claude: 我来设计架构...             │  │
│ │ 🟢 kimi-vip      │  │ kimi: UI草图已完成                  │  │
│ │ 🟡 hermes-local  │  │ hermes: 开始编写代码...             │  │
│ │ ⚪ gemini-pro    │  │                                     │  │
│ │                  │  │                                     │  │
│ │ [r] Refresh      │  │                                     │  │
│ │ [a] Add Agent    │  │                                     │  │
│ └──────────────────┘  └─────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│ > 发送消息给所有Agent...                                        │
└─────────────────────────────────────────────────────────────────┘
```

**快捷键**:

| 按键 | 功能 |
|------|------|
| `Tab` | 切换标签页 |
| `↑/↓` | 选择Agent/消息 |
| `Enter` | 发送消息 |
| `a` | 添加Agent |
| `d` | 删除Agent |
| `m` | 查看记忆 |
| `q` | 退出 |

**里程碑**:
- [ ] 基础TUI框架
- [ ] Agent列表界面
- [ ] 聊天界面
- [ ] 监控面板
- [ ] 记忆浏览器

---

### Phase 6: GUI图形界面 (Week 18-22)

**目标**: 跨平台GUI应用

**技术栈**: [Tauri](https://tauri.app/) (Rust后端 + Web前端)

```
┌─────────────────────────────────────────────────────────────┐
│                     Tauri 架构                              │
├─────────────────────────────────────────────────────────────┤
│  Frontend (WebView)          Backend (Rust)                 │
│  ┌─────────────────┐         ┌─────────────────┐           │
│  │ React/Vue       │  IPC    │ ah-core         │           │
│  │ - Agent画布     │  ←──→   │ - 消息总线      │           │
│  │ - 工作流编辑器  │         │ - 适配器管理    │           │
│  │ - 监控仪表板    │         │ - 记忆层        │           │
│  └─────────────────┘         └─────────────────┘           │
└─────────────────────────────────────────────────────────────┘
```

**前端技术选型**:

| 组件 | 选择 | 用途 |
|------|------|------|
| 框架 | React 18 | UI组件 |
| 状态 | Zustand | 状态管理 |
| 样式 | Tailwind CSS | 原子化CSS |
| 组件 | Radix UI | 无头组件 |
| 图表 | Recharts | 监控图表 |
| 画布 | React Flow | 工作流编辑 |

**核心界面**:

1. **Agent画布** - 拖拽配置
```typescript
// 前端组件预览
interface AgentNode {
  id: string;
  type: 'claude' | 'kimi' | 'gemini' | 'hermes' | 'codebuddy';
  position: { x: number; y: number };
  data: {
    name: string;
    status: 'online' | 'offline' | 'busy';
    roles: string[];
  };
}

// React Flow节点
<ReactFlow
  nodes={agents.map(a => ({
    id: a.id,
    type: 'agent',
    position: a.position,
    data: { ...a, onConfigure: () => openConfig(a) }
  }))}
  edges={connections}
  onConnect={handleConnect}
/>
```

2. **角色混合器**
```typescript
interface RoleMixerProps {
  baseModel: string;
  roles: Role[];
  onChange: (weights: Record<string, number>) => void;
}

// 可视化权重滑块
<RoleMixer
  roles={[
    { id: 'software_engineer', name: '软件工程师', weight: 1.0 },
    { id: 'financial_analyst', name: '金融分析师', weight: 0.0 },
  ]}
/>
```

**里程碑**:
- [ ] Tauri项目搭建
- [ ] 前端框架配置
- [ ] Agent画布实现
- [ ] 角色混合器
- [ ] 监控仪表板
- [ ] 记忆管理界面

---

### Phase 7: 测试与发布 (Week 23-24)

**测试策略**:

```rust
// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_message_bus() {
        let bus = MessageBus::new();
        let msg = create_test_message();
        
        bus.publish(msg.clone()).await.unwrap();
        
        // 验证消息分发
        // ...
    }
}

// 集成测试
#[tokio::test]
async fn test_claude_adapter() {
    let adapter = ClaudeAdapter::new();
    let response = adapter.chat(vec![test_message()], None).await;
    
    assert!(response.is_ok());
}

// 端到端测试
#[test]
fn test_e2e_workflow() {
    // 启动Hub
    let hub = AgentHub::new(test_config());
    
    // 注册Agent
    hub.register_agent(claude_config()).await;
    hub.register_agent(hermes_config()).await;
    
    // 运行工作流
    let result = hub.run_workflow(test_workflow()).await;
    
    assert!(result.is_ok());
}
```

**发布流程**:

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags: ['v*']

jobs:
  build:
    strategy:
      matrix:
        target: [
          x86_64-apple-darwin,
          aarch64-apple-darwin,
          x86_64-unknown-linux-gnu,
          aarch64-unknown-linux-gnu,
          x86_64-pc-windows-msvc
        ]
    
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Rust
        uses: dtolnay/rust-action@stable
        with:
          targets: ${{ matrix.target }}
      
      - name: Build
        run: cargo build --release --target ${{ matrix.target }}
      
      - name: Package
        run: |
          mkdir -p dist
          cp target/${{ matrix.target }}/release/agenthub dist/
          tar czf agenthub-${{ matrix.target }}.tar.gz -C dist .
      
      - name: Upload Release
        uses: softprops/action-gh-release@v1
        with:
          files: agenthub-*.tar.gz
```

**里程碑**:
- [ ] 单元测试 >80%覆盖
- [ ] 集成测试完成
- [ ] 性能基准测试
- [ ] 安全审计
- [ ] v0.1.0发布

---

## 📅 时间线总览

```
Week 1-2   │ Phase 0: 项目初始化
Week 3-5   │ Phase 1: 核心引擎
Week 6-9   │ Phase 2: 适配器系统
Week 10-12 │ Phase 3: 共享记忆
Week 13-14 │ Phase 4: 监控系统
Week 15-17 │ Phase 5: TUI界面
Week 18-22 │ Phase 6: GUI界面
Week 23-24 │ Phase 7: 测试发布
           │
           └─────────────────────────────────────►
           0     6     12    18    24
           └─────┴─────┴─────┴─────┘
                 周数
```

**预计总工期**: 6个月（24周）

---

## 🛠️ 开发环境

### 必需工具

```bash
# Rust工具链
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
rustup target add aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu

# 跨平台编译工具
# macOS
brew install FiloSottile/musl-cross/musl-cross

# Linux
sudo apt-get install gcc-aarch64-linux-gnu

# 开发依赖
cargo install cargo-watch cargo-edit cargo-tarpaulin
```

### 快速开始

```bash
# 克隆仓库
git clone https://github.com/yourusername/agenthub.git
cd agenthub

# 开发模式运行
cargo watch -x run

# 运行测试
cargo test --all

# 构建发布版本
cargo build --release

# 运行TUI
cargo run --bin ah-tui

# 运行GUI
cargo run --bin ah-gui
```

---

## 📋 检查清单

### 开发前准备
- [ ] Rust环境配置
- [ ] 跨平台编译测试
- [ ] GitHub仓库创建
- [ ] CI/CD配置

### 每个Phase开始
- [ ] 设计文档Review
- [ ] 接口定义确认
- [ ] 测试计划制定

### 每个Phase结束
- [ ] 代码Review
- [ ] 测试通过
- [ ] 文档更新
- [ ] 性能基准

---

这个路线图是否符合你的预期？需要调整哪些部分？