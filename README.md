# Crow Hub 🐦‍⬛

[![CI](https://github.com/zhiqing-yu/crow-hub/actions/workflows/ci.yml/badge.svg)](https://github.com/zhiqing-yu/crow-hub/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.86%2B-orange.svg)](https://www.rust-lang.org)

> A universal agent orchestration hub — 调度你的 AI Agent 团队

Crow Hub 是一个基于 Rust 的多 Agent 调度中枢。它将不同的 AI Agent（Claude、Kimi、Gemini、OpenClaw 等）通过统一的消息总线编排在一起，让它们可以在同一个频道中协作、对话、完成任务。

## ✨ 核心特性

- **🔄 多驱动适配** — 支持 API、子进程（Subprocess）、Tmux、MCP 等多种驱动方式，云端 + 本地混合调度
- **💬 Agent 间通讯** — 内置消息总线，Agent 可以互相发送消息、订阅频道
- **🧠 共享记忆** — 可插拔向量数据库，记忆可带走不锁定
- **📊 全链路监控** — Token 用量、性能指标、成本追踪
- **🎛️ 可视化配置** — TUI 终端界面 + GUI 图形界面
- **⚡ 高性能** — Rust 编写，安全、快速、跨平台

## 🚀 快速开始

### First run (clean clone → working TUI in 3 commands)

```bash
git clone https://github.com/zhiqing-yu/crow-hub.git
cd crow-hub
cargo build --release

# 1. Scan YOUR machine for installed agent CLIs and write per-machine manifests.
#    Probes native PATH, any WSL distros, and SSH hosts configured in ~/.ssh/config.
#    Supports: nvm / fnm / volta / asdf / mise / Homebrew / npm-global / custom paths.
./target/release/crow setup

# 2. Launch the TUI.  Agents the setup wizard found appear in the sidebar.
./target/release/crow
```

That's it.  No manual configuration — `crow setup` writes per-agent manifests to
`plugins/agents/` based on what's actually installed on your machine.

> **Note**: `plugins/agents/` is **gitignored**.  Each user's manifests are
> per-machine (binary paths, SSH hosts, WSL distros, etc.) and never shared
> via git.  See [`examples/agents/`](examples/agents/) for templates if you
> need to add an agent the scanner missed.

### Agent Manifest 示例

每个 Agent 是一个独立的 TOML 配置文件。完整模板见 [`examples/agents/`](examples/agents/) — 复制对应模板到
`plugins/agents/<your-agent-name>/agent.toml` 并修改填充值即可。三种常见形态：

```toml
# Native — agent binary installed on the same machine running crow-hub
[subprocess]
command = "/opt/homebrew/bin/claude"   # macOS example
shell = "native"
```

```toml
# WSL — agent binary installed inside a WSL distro
[subprocess]
command = "/home/user/.local/bin/claude"
shell = "wsl"
wsl_distro = "Ubuntu"
```

```toml
# SSH — agent binary on a remote host (e.g. a beefy GPU server)
[subprocess]
command = "/home/user/.local/bin/claude"
shell = "ssh"
ssh_host = "192.168.50.1"
ssh_user = "user"
```

### Diagnosing an agent

If an agent shows red ✗ in the TUI, the fastest feedback loop is:

```bash
./target/release/crow doctor <agent-name>
```

Sends a one-shot test prompt directly through the driver (bypassing the bus
and TUI) and prints the raw response or full error detail — including the
host-env probe time, captured PATH var count, and exact invocation.

### Refreshing the host env cache

`crow setup` and the runtime probe each host once for its interactive `$PATH`
(captures nvm/fnm/volta/asdf/mise binaries) and cache the result to
`~/.crow-hub/env-cache/`.  After installing a new tool or changing your
shell config:

```bash
./target/release/crow refresh-env                  # refresh all hosts
./target/release/crow refresh-env wsl:Ubuntu       # refresh one WSL distro
./target/release/crow refresh-env ssh:user@host    # refresh one SSH host
```

### 启动

```bash
# 启动 TUI 界面（默认）
crow

# 启动后台服务
crow server

# 查看状态
crow status

# 列出所有 Agent
crow agent list

# 运行工作流
crow run examples/simple-workflow.yaml
```

## 📖 使用示例

### 命令行使用

```bash
# 查看状态
crow status

# 列出所有 Agent
crow agent list

# 添加 Agent（提示你创建 manifest）
crow agent add --name "my-claude" --adapter claude

# 发送消息
crow send --to "claude-wsl-ubuntu" --message "Hello!"

# 运行工作流
crow run examples/simple-workflow.yaml
```

### 工作流定义

```yaml
workflow:
  name: "代码审查工作流"

  agents:
    - id: "coder"
      adapter: "claude"
      role: "developer"

    - id: "reviewer"
      adapter: "kimi"
      role: "code_reviewer"

  steps:
    - id: "write-code"
      agent: "coder"
      action: "generate_code"
      inputs:
        requirement: "实现一个 HTTP 客户端"

    - id: "review-code"
      agent: "reviewer"
      action: "review"
      inputs:
        code: "{{write-code.output}}"
      depends_on:
        - "write-code"
```

## 🏗️ 架构

```
┌─────────────────────────────────────────────────────────┐
│                      User Interface                      │
│              (TUI: crow / GUI: crow-gui)                │
├─────────────────────────────────────────────────────────┤
│                      Core Engine                         │
│  ┌─────────┬─────────┬─────────┬─────────────────────┐ │
│  │  Bus    │Registry │ Session │   Orchestrator      │ │
│  │(Message)│ (Agent) │ (Multi) │    (Workflow)       │ │
│  └────┬────┴────┬────┴────┬────┴──────────┬──────────┘ │
│       │         │         │               │            │
│  ┌────┴─────────┴─────────┴───────────────┴─────────┐  │
│  │              Agent Runtime (ch-agent)             │  │
│  │   ┌──────┬───────────┬────────┬─────────┐         │  │
│  │   │ API  │ Subprocess│  Tmux  │   MCP   │         │  │
│  │   └──────┴───────────┴────────┴─────────┘         │  │
│  └───────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────┤
│              Adapter Abstraction Layer                   │
│       (Claude / Kimi / Gemini / OpenClaw / ...)         │
└─────────────────────────────────────────────────────────┘
```

## 🛠️ 支持的 Agent

### 云端 Agent（API 驱动）
- [x] Claude (Anthropic)
- [x] Kimi (Moonshot)
- [x] Gemini (Google)
- [x] GPT-4 / Codex (OpenAI)

### 本地 Agent（子进程 / Tmux 驱动）
- [x] Claude Code CLI
- [x] Kimi CLI
- [x] Gemini CLI
- [x] OpenClaw
- [x] OpenCode
- [x] Ollama
- [x] vLLM

## 📦 项目结构

```
crow-hub/
├── crates/
│   ├── ch-protocol/    # 通讯协议与核心消息类型
│   ├── ch-model/       # 模型路由、注册表与发现
│   ├── ch-core/        # 消息总线、Agent 注册表、编排引擎
│   ├── ch-agent/       # Agent 运行时与驱动实现（API / Subprocess / Tmux / MCP）
│   ├── ch-adapter/     # 适配器抽象层
│   ├── ch-memory/      # 共享记忆与向量存储
│   ├── ch-monitor/     # 监控与指标系统
│   ├── ch-tui/         # TUI 终端界面（binary: crow）
│   └── ch-gui/         # GUI 图形界面（binary: crow-gui）
├── adapters/           # 适配器实现
├── plugins/
│   └── agents/         # Per-user agent manifests (gitignored — generated by `crow setup`)
├── examples/
│   ├── agents/         # Template manifests to copy + customize for unusual setups
│   └── *.toml          # Other config examples
└── docs/               # 文档
```

## 🔧 驱动说明

Crow Hub 的 `ch-agent` 运行时支持四种驱动：

| 驱动 | 说明 | 适用场景 |
|------|------|----------|
| `api` | 直接调用模型 API | 云端大模型 |
| `subprocess` | 通过 stdin/stdout 或 argv 调用本地 CLI | Claude Code、Kimi、Gemini、OpenClaw |
| `tmux` | 在 tmux 会话中运行并发送按键指令 | 需要持久会话的 TUI 型 Agent |
| `mcp` | Model Context Protocol（预留） | 未来扩展 |

## 🤝 贡献

欢迎贡献！请查看 [CONTRIBUTING.md](CONTRIBUTING.md) 了解如何参与。

## 📄 许可证

Apache License 2.0 — 查看 [LICENSE](LICENSE) 文件了解详情。

## 🙏 致谢

- 感谢所有开源 Agent 项目的开发者
- 感谢 Rust 社区提供的优秀工具链

---

<p align="center">
  Made with ❤️ by Crow Hub Contributors
</p>
