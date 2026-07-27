# mp_agent — 架构设计

> 本文档描述 mp_agent 的整体架构、模块划分、数据流和并发模型。

## 目录

1. [项目概览](./01-overview.md)
2. [架构设计](./02-architecture.md) ← 你在这里
3. [工具系统](./03-tools.md)
4. [权限系统](./04-permission.md)
5. [MCP 集成](./05-mcp.md)
6. [技能系统](./06-skills.md)
7. [UI 组件](./07-ui.md)
8. [配置与运行](./08-config.md)
9. [开发指南](./09-development.md)

---

## 1. 总体架构

mp_agent 采用**事件驱动的 TUI 应用架构**，核心组件通过 `tokio` 多路复用器 + 通道（channel）进行通信。整体遵循"单 UI 线程 + 异步 Agent 任务"的模式。

```
┌─────────────────────────────────────────────────────────────────┐
│                         TUI 主循环                               │
│  ┌─────────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │    App 状态      │  │   事件轮询    │  │     渲染层         │  │
│  │ - 聊天历史       │◄─┤ - 键盘事件    │─►│ - Ratatui Frame   │  │
│  │ - 输入缓冲区     │  │ - Agent 事件  │  │ - ChatArea       │  │
│  │ - 权限待决       │  │   (通道接收)  │  │ - InputArea      │  │
│  │ - 选择待决       │  │              │  │ - Markdown 渲染   │  │
│  └───────┬─────────┘  └──────┬───────┘  └───────────────────┘  │
│          │                   │                                  │
│          ▼                   ▼                                  │
│  ┌─────────────────┐  ┌──────────────┐                          │
│  │  AgentCommand    │  │ AgentEvent    │                          │
│  │  (发送通道)      │  │ (接收通道)    │                          │
│  └────────┬────────┘  └──────┬───────┘                          │
│           │                  │                                   │
└───────────┼──────────────────┼───────────────────────────────────┘
            │                  │
            ▼                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Agent 异步任务                              │
│  ┌─────────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │  消息发送        │  │ 工具调用循环  │  │ 权限审批          │  │
│  │  (SSE 流式)     │◄─┤ - 原生工具    │◄─┤ - oneshot 通道    │  │
│  │                 │  │ - MCP 工具    │  │ - 内存规则缓存     │  │
│  └─────────────────┘  └──────────────┘  └───────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## 2. 核心模块

### 2.1 `main.rs` — 应用入口

- 安装 `color-eyre` 错误钩子
- 配置 `tracing` 日志（写入 `mp_agent.log` 文件）
- 从 `.env` 加载配置
- 初始化 Crossterm 终端（原始模式 + 备用屏幕）
- 创建 `App` 实例并进入主循环
- 退出时恢复终端状态

### 2.2 `app.rs` — 应用状态与事件循环

`App` 结构体持有所有运行时状态：

| 字段 | 说明 |
|---|---|
| `chat` | 聊天区域，管理消息历史和渲染 |
| `input` | 输入区域，管理文本编辑、历史、上下文建议、补全 |
| `mcp` | MCP 连接管理器 |
| `processing` | 是否正在等待 AI 响应 |
| `streaming_buffer` | 流式接收的 token 缓冲 |
| `cmd_tx` | 发送 `AgentCommand` 到 Agent 的通道 |
| `event_rx` | 接收 `AgentEvent` 的通道 |
| `pending_permission` | 待处理的权限请求 |
| `pending_choice` | 待处理的选择请求 |
| `permission_rules` | 已记住的权限规则 |
| `token_usage_total` | 会话累计 token 用量（prompt + completion） |
| `token_usage_session` | 当前对话轮次的 token 用量 |
| `pending_messages` | 等待 Agent 完成时发送的输入消息队列 |

**主循环逻辑**（在 `main.rs` 的 `run_app` 中）：

1. `app.draw(terminal)` — 渲染当前帧
2. `event::poll(Duration::from_millis(16))` — 轮询键盘事件
3. `app.handle_key_event(key)` — 处理键盘输入
4. `app.process_agent_events()` — 消费 Agent 事件通道
5. 检查 `app.running` 标志决定是否退出

### 2.3 `agent.rs` — AI Agent 核心

`Agent` 负责与 AI 模型对话、管理消息历史、执行工具调用循环。

**关键方法**：

- `new()` — 创建 Agent，初始化系统提示和消息历史
- `send_message()` — 发送用户消息并处理完整的工具调用循环（最多 `max_iterations = 100` 轮）
- `get_tools()` — 获取原生工具定义列表
- `request_permission()` — 通过 oneshot 通道向 UI 请求权限
- `execute_todo_tool()` — 执行 todo 管理工具
- `execute_choices_tool()` — 执行多选择工具
- `handle_tool_calls()` — 处理工具调用列表，构建消息并执行工具

**消息流**：

1. 用户消息加入 `messages` 向量
2. 构建请求（含系统提示 + 工具定义），发起 SSE 流式请求
3. 实时解析 token 流，发送 `AgentEvent::Token`
4. 如果 finish_reason 为 `tool_calls`，进入工具调用循环
5. 对每个工具调用：检查权限 → 执行 → 结果加入消息历史
6. 继续下一轮对话，直至无工具调用
7. 发送 `AgentEvent::MessageComplete`

### 2.4 `config.rs` — 配置管理

从环境变量加载配置：

| 变量 | 默认值 | 说明 |
|---|---|---|
| `OPENAI_API_KEY` | 必须设置 | API 密钥 |
| `OPENAI_BASE_URL` | `https://api.openai.com/v1` | API 基础 URL |
| `OPENAI_MODEL` | `gpt-4o` | 模型名称 |
| `OPENAI_MAX_TOKENS` | 无 | 最大 token 数（可选） |

支持任意 OpenAI 兼容的 API endpoint。

### 2.5 `mcp.rs` — MCP 集成管理器

`McpManager` 管理多个 MCP 服务器连接：

- `connect(name, command, args)` — 启动 MCP 子进程并连接，返回工具列表
- `all_mcp_tools()` — 获取所有 MCP 工具
- `call_tool(name, args)` — 调用指定 MCP 工具
- `disconnect_all()` — 断开所有连接

使用 `rmcp` 库作为 MCP 客户端，通过 `TokioChildProcess` 传输。

### 2.6 `permission.rs` — 权限管理

定义权限操作类型（`Write` / `Execute`）、决策（`Allow` / `Deny`）、规则匹配。

- `needs_permission(tool_name, args)` — 判断工具调用是否需要权限
- `match_rule(rules, op, path)` — 匹配已记住的规则
- `abspath(path)` — 转换为绝对路径
- `truncate(s, max)` — 截断显示用字符串

权限规则基于路径前缀匹配，在内存中持久化（会话级）。

### 2.7 `error.rs` — 错误处理

安装 `color-eyre` 全局钩子，提供带颜色堆栈跟踪的错误报告。

## 3. 并发模型

mp_agent 使用 **tokio 运行时**，采用以下并发策略：

### 3.1 事件通道

- `mpsc::unbounded_channel::<AgentCommand>()` — App → Agent 的单向命令通道
- `mpsc::unbounded_channel::<AgentEvent>()` → Agent → App 的事件通道
- `oneshot::channel::<PermissionDecision>()` — 单次权限请求/响应
- `oneshot::channel::<ChoiceResult>()` — 单次选择请求/响应

### 3.2 Agent 任务

Agent 运行在独立的 tokio 任务中（`tokio::spawn(run_agent_task(...))`），与 UI 主循环解耦。

```
UI 主线程                    Agent 任务
    │                          │
    ├─ SendMessage ──────────►│ 接收命令
    │                          │ 处理请求
    │◄─ Token ────────────────┤ 流式推送
    │◄─ PermissionRequired ──┤ 请求权限
    │─ Decision ─────────────►│ 响应（oneshot）
    │◄─ MessageComplete ─────┤ 消息完成
```

### 3.3 流式响应

使用 `reqwest` + `eventsource-stream` 解析 SSE 流：

- 响应流被映射为字节流 → 事件流 → JSON 对象
- 每个 token 块立即通过通道发送到 UI
- 使用 `tokio::time::timeout` 检测流空闲超时

### 3.4 工具调用循环

AI 可以连续进行多轮工具调用（最多 `max_iterations = 100` 轮）。每轮：

1. 发送当前消息 + 工具定义 → 获取流式响应
2. 解析工具调用 → 执行工具 → 结果作为工具消息返回
3. 重复直到 finish_reason 不再是 `tool_calls`

## 4. 数据流

### 4.1 用户消息流

```
用户输入 (Enter)
    ↓
App::handle_key_event
    ↓
    ├── processing 中? → 加入 pending_messages 队列
    └── 正常状态 → AgentCommand::SendMessage (通过 mpsc 通道)
                        ↓
                Agent::send_message
    ├── 构建系统提示（技能 + AGENTS.md）
    ├── 附加工具定义
    ├── SSE 流式请求 → OpenAI 兼容 API
    ├── 实时推送 AgentEvent::Token + AgentEvent::TokenUsage
    └── 工具调用？ → permission? → execute → result
            ↓
AgentEvent::MessageComplete
    ↓
App::process_agent_events → ChatArea::add_message
    ↓
update_context_suggestions() → 提取上下文建议
drain_pending_messages() → 发送下一条排队消息
```

### 4.2 权限请求流

```
Agent 检测到需要权限的工具调用
    ↓
Agent::request_permission (oneshot 发送)
    ↓
AgentEvent::PermissionRequired
    ↓
App::process_agent_events → 暂存 pending_permission
    ↓
App::draw → 渲染权限提示栏
    ↓
用户按键 (y/n/a/d/Esc)
    ↓
App::handle_key_event → 决策 + 可选保存规则
    ↓
oneshot::send(decision) → Agent 继续执行
```

### 4.3 多选择流

```
Agent 调用 present_choices 工具
    ↓
AgentEvent::ChoiceRequired
    ↓
App::process_agent_events → 暂存 pending_choice
    ↓
App::draw → 渲染选择列表
    ↓
用户输入数字或自定义文本
    ↓
App::handle_choice_key → 发送 ChoiceResult
    ↓
oneshot::send(result) → Agent 继续执行
```

## 5. 状态管理

### 5.1 聊天状态

`ChatArea` 管理消息列表，每条消息附带时间戳用于动画。支持：

- 自动滚动（用户未手动滚动时）
- 手动滚动（↑/↓、PageUp/PageDown）
- 流式预览渲染（`render_with_preview`）

### 5.2 输入状态

`InputArea` 管理文本缓冲区、光标位置、输入历史、Tab 补全。

### 5.3 权限规则

`Vec<PermissionRule>` 在内存中维护，用户选择 "always/deny" 时添加新规则。规则基于 `(op, path_prefix)` 匹配。

## 6. 渲染管线

```
App::draw(terminal)
    └── terminal.draw(|frame|
        ├── 布局计算 (Layout::Vertical)
        │   ├── 聊天区域 (Constraint::Min(5))
        │   ├── 建议区域 (Constraint::Length, 仅 slash 命令时)
        │   ├── 输入区域 (Constraint::Length(3))
        │   └── 状态栏 (Constraint::Length(1))
        ├── ChatArea::render / render_with_preview
        ├── InputArea::render_suggestions (如果需要)
        ├── InputArea::render
        └── 状态栏渲染 (权限/选择/普通状态)
    )
```

## 7. 扩展性设计

- **工具系统**：新增工具只需在 `tools.rs` 中添加 `native_tool_definitions` 条目和 `execute_native_tool` 分支
- **MCP 集成**：通过 `McpManager` 动态连接外部服务器，工具自动映射
- **技能系统**：从文件系统加载，无需修改代码
- **权限系统**：规则匹配引擎可扩展为持久化存储