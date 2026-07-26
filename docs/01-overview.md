# mp_agent — 项目文档

> 一个基于终端的 AI 编程助手，使用 Rust + OpenAI 兼容 API + MCP 协议构建，支持流式输出、工具调用、Markdown 渲染、技能系统和权限审批。

## 目录

1. [项目概览](./01-overview.md) ← 你在这里
2. [架构设计](./02-architecture.md)
3. [工具系统](./03-tools.md)
4. [权限系统](./04-permission.md)
5. [MCP 集成](./05-mcp.md)
6. [技能系统](./06-skills.md)
7. [UI 组件](./07-ui.md)
8. [配置与运行](./08-config.md)
9. [开发指南](./09-development.md)

---

## 1. 产品定位

mp_agent 是一个**运行在终端内的 AI 编程助手**，核心目标是在不离开终端的前提下，让开发者能够：

- 与 AI 模型对话，获取编码建议
- 让 AI 调用工具（读/写/编辑文件、执行命令、搜索等）完成实际工作
- 连接外部 MCP 服务器扩展工具生态
- 对敏感操作进行权限审批，确保安全可控

适用场景：日常编码、代码审查、项目脚手架生成、自动化脚本编写、DevOps 任务等。

## 2. 核心特性

| 特性 | 说明 |
|---|---|
| **TUI 界面** | 基于 Ratatui + Crossterm 的全终端界面，聊天、输入、状态栏一目了然 |
| **流式输出** | 支持 SSE 流式响应，实时显示 AI 生成的 token |
| **工具调用** | 内置多种原生工具，AI 可自动调用完成文件操作、命令执行等任务 |
| **MCP 协议支持** | 可连接外部 MCP 服务器，扩展工具生态 |
| **技能系统** | 支持从 `.opencode/skills/` 目录加载自定义技能和 Agent 上下文 |
| **Markdown 渲染** | 支持代码块、表格、引用、列表等 Markdown 格式的终端渲染，带语法高亮 |
| **Slash 命令** | `/help`、`/clear`、`/tools` 等便捷命令 + Tab 自动补全 |
| **类 Emacs 快捷键** | Ctrl+A/E/U/L 等，历史上下翻，滚动查看 |
| **权限审批** | 对写入文件、执行命令等敏感操作实时请求用户确认，支持记住选择（always/deny） |
| **任务管理** | 内置 todo 工具，可添加/更新/列出/删除任务 |
| **多选择决策** | AI 可通过 `present_choices` 工具让用户在多个方案中选择 |

## 3. 技术栈

| 层级 | 技术 |
|---|---|
| 语言 | Rust (edition 2024) |
| TUI 框架 | Ratatui + Crossterm |
| AI API | async-openai (OpenAI 兼容 SSE API) |
| MCP 客户端 | rmcp (Model Context Protocol) |
| HTTP 客户端 | reqwest + eventsource-stream |
| Markdown | pulldown-cmark (自定义渲染为 Ratatui) |
| 配置 | dotenvy (.env 文件) |
| 日志 | tracing + tracing-subscriber |
| 错误处理 | color-eyre |

## 4. 快速开始

```bash
# 克隆项目
git clone <repo-url>
cd mp_agent

# 构建
cargo build --release

# 配置 API 密钥
cp .env.example .env   # 如存在，或直接编辑 .env
# 编辑 .env 填写 OPENAI_API_KEY 等

# 运行
cargo run
# 或
./target/release/mp_agent
```

详细配置说明见 [配置与运行](./08-config.md)。

## 5. 项目结构

```
mp_agent/
├── Cargo.toml              # 依赖与包配置
├── README.md               # 项目简介
├── .env                    # 环境变量（API 密钥等，不应提交）
├── .gitignore
├── docs/                   # 项目文档（本目录）
├── src/
│   ├── main.rs             # 入口：初始化 TUI、配置、事件循环
│   ├── app.rs              # 应用状态：键盘事件处理、事件消费、界面绘制、权限审批
│   ├── agent.rs            # AI Agent：流式聊天、工具调用循环、消息管理、权限请求
│   ├── config.rs           # 配置：从 .env 加载 API 密钥、模型等
│   ├── mcp.rs              # MCP 管理器：连接外部 MCP 服务器、工具映射
│   ├── permission.rs       # 权限管理：操作类型、规则匹配、路径处理
│   ├── error.rs            # 错误处理：color-eyre 钩子安装
│   └── agent/
│       ├── tools.rs        # 原生工具定义与执行
│       └── skill.rs        # 技能加载、AGENTS.md 读取、系统提示构建
│   └── ui/
│       ├── chat.rs         # 聊天区域：消息渲染、滚动条、流式预览
│       ├── input.rs        # 输入区域：命令行编辑、历史、Tab 补全
│       ├── markdown.rs     # Markdown 渲染器：pulldown-cmark → Ratatui
│       └── layout.rs       # 布局工具
├── tests/                  # 测试
└── target/                 # 构建产物
```

## 6. 数据流概览

```
用户输入 → App (handle_key_event) → AgentCommand::SendMessage → Agent
                                                              ↓
                                              构建请求 + 系统提示 + 工具定义
                                                              ↓
                                              SSE 流式请求 → OpenAI 兼容 API
                                                              ↓
                                              解析 token → AgentEvent::Token → UI 实时渲染
                                                              ↓
                                              如果 AI 请求工具调用 → 检查权限 → 执行工具 → 结果返回
                                                              ↓
                                              AgentEvent::MessageComplete → 消息加入历史
```

详细架构见 [架构设计](./02-architecture.md)。

## 7. 许可证

MIT