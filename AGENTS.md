# AGENTS.md — mp_agent 项目上下文

> 本文件由 mp_agent 在会话启动时自动读取，作为项目上下文注入系统提示。

## 项目简介

**mp_agent** 是一个基于终端的 AI 编程助手，使用 Rust + OpenAI 兼容 API + MCP 协议构建。

- **语言**：Rust edition 2024
- **UI 框架**：Ratatui + Crossterm（TUI）
- **AI 后端**：OpenAI 兼容 API（SSE 流式响应）
- **工具协议**：MCP（Model Context Protocol）
- **包管理**：Cargo

## 项目结构

```
.
├── Cargo.toml          # 项目依赖与元数据
├── .env                # API 配置（OPENAI_API_KEY、OPENAI_BASE_URL 等）
├── README.md           # 项目总文档
├── AGENTS.md           # 本文件：Agent 项目上下文
├── src/
│   ├── main.rs         # 入口：初始化 TUI、配置、事件循环
│   ├── app.rs          # 应用状态：键盘/鼠标事件处理、界面绘制、权限审批、Agent 事件消费、上下文建议提取
│   ├── agent.rs        # AI Agent：流式聊天、工具调用循环、消息管理、权限/选择请求、todo 管理
│   ├── config.rs       # 配置：从 .env 加载 API 密钥、模型等
│   ├── mcp.rs          # MCP 管理器：连接外部 MCP 服务器、工具映射
│   ├── permission.rs   # 权限管理：操作类型、规则匹配、路径处理、记忆决策
│   ├── error.rs        # 错误处理：color-eyre 钩子安装
│   ├── agent/
│   │   ├── request.rs  # SSE 流式请求解析：token 流解析、usage 统计、事件推送
│   │   ├── tools.rs    # 原生工具定义与执行（bash、文件操作、搜索、todo、present_choices）
│   │   └── skill.rs    # 技能加载、AGENTS.md 读取、系统提示构建
│   └── ui/
│       ├── chat.rs     # 聊天区域：消息渲染、滚动条、流式预览、工具结果折叠
│       ├── input.rs    # 输入区域：命令行编辑、历史、Tab 补全、Slash 命令、上下文建议
│       ├── markdown.rs # Markdown 渲染器：pulldown-cmark → Ratatui（含语法高亮）
│       └── layout.rs   # 布局工具
├── tests/              # 集成测试
├── docs/               # 额外文档
└── .mp_agent/skills/   # 自定义技能目录（可选）
```

## 开发工作流

### 构建与运行

```bash
# 开发构建
cargo build

# 运行
cargo run

# Release 构建
cargo build --release
./target/release/mp_agent
```

### 测试与检查

```bash
# 运行测试
cargo test

# Clippy 检查
cargo clippy

# 格式化
cargo fmt
```

### 配置

编辑 `.env` 文件设置 API 信息：

```env
OPENAI_API_KEY=sk-your-api-key
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_MODEL=gpt-4o
OPENAI_MAX_TOKENS=5000
```

## 工具使用规范

mp_agent 提供了一套内置工具，Agent 应遵循以下最佳实践：

### 文件编辑

- **优先使用 `edit_file`** 而非 `write_file`，除非是创建新文件或大规模重写（>50% 变更）
- 编辑前先读取目标文件，理解上下文
- `edit_file` 的 `old_string` 必须唯一匹配，提供前后 1-2 行上下文
- 编辑后读取文件验证变更

### Bash 命令

- 优先使用相对路径
- 长任务应增量输出，避免长时间阻塞
- 始终检查退出码，失败时分析错误并修复
- 调试时使用 `--no-cache` 或等效标志
- 用 `&&` 链接命令，但保持合理长度

### 搜索与导航

- 用 `glob` 按模式查找文件
- 用 `grep` 按正则搜索内容
- 用 `list_directory` 探索目录结构

### 通用最佳实践

1. **先读后写** — 修改文件前务必先读取
2. **小步迭代** — 每次只做专注的小改动，逐步验证
3. **测试验证** — 改动后运行相关测试或构建
4. **语法检查** — 编辑后确认代码能编译/运行
5. **善用工具** — 不要猜测文件内容或结构，用工具探索
6. **简洁清晰** — 直接给出答案，使用 markdown 格式化
7. **错误处理** — 工具失败时读取错误信息，修复后重试
8. **权限感知** — 文件写入和命令执行需要用户审批，被拒绝时解释原因并建议替代方案

## 权限系统

敏感操作（写入文件、执行命令）会触发权限请求：

```
【Permission】WRITE /path/to/file | write_file  [y]es [a]lways [n]o [d]eny [Esc]
```

- **y / n**：本次允许 / 拒绝
- **a / d**：允许 / 拒绝并记住规则（基于路径前缀）
- **Esc**：默认拒绝

规则保存在内存中，会话结束自动清除。

## 技能系统

Agent 会自动从以下目录加载技能文件（`.md` / `.txt` / `.skill`）：

- `./.mp_agent/skills/`
- `$HOME/.config/mp_agent/skills/`

技能文件格式：
```
第一行：技能名称
第二行：技能描述
其余行：技能正文内容
```

## MCP 工具

如果 MCP 服务器已配置，额外的工具会以服务器名称为前缀（例如 `git_status`、`db_query`），与上述原生工具一起可用。当它们的能力匹配任务时使用它们。将 MCP 工具错误视为暂时性的——检查错误消息并在必要时用修正后的参数重试。

## 决策呈现

当存在多种可行方案时，Agent 应调用 `present_choices` 呈现选项，让用户选择。

## 上下文建议

`InputArea` 支持从当前聊天历史中自动提取上下文建议，在用户输入时通过 Tab 补全提供：

- **提取来源**：`extract_context_suggestions()` 扫描所有消息类型（User / Assistant / ToolCall / ToolResult / System / Error）
- **候选提取**：`extract_candidates()` 提取文件路径类字符串（含 `/` 或 `.`）、命令前缀（`cargo`、`git`、`ls`、`cat`）和引号内文本
- **JSON 字符串**：`extract_json_strings()` 从工具调用的 JSON 参数中提取所有字符串叶子
- **去重与截断**：使用 `HashSet` 去重，最多保留 20 条建议
- **显示时机**：输入为空或按 `/` 时同时显示 slash 命令和上下文建议；输入非 slash 时仅匹配上下文建议

## 消息队列

等待 AI 响应时用户仍可继续输入，消息自动排队发送：

- `processing` 为 true 时，Enter 将输入加入 `pending_messages` 队列
- 状态栏显示 `Processing... (N queued)` 提示排队数量
- 当前轮次完成后 `drain_pending_messages()` 依次发送队列中的消息
- 按 Esc 取消处理并清空所有排队消息

## 输入与聊天行为

- 等待 AI 响应时用户仍可继续输入，消息自动排队发送。状态栏显示 `Processing... (N queued)` 提示排队数量。
- 在输入区域按 **Alt+Enter** 插入字面换行符，而非提交消息。Shift+Enter 需要 Kitty 键盘协议，多数终端不支持。
- Tab 补全从当前聊天历史中提取上下文建议（文件路径、命令前缀、引号内文本及工具调用 JSON 中的字符串）。

## 代码风格

- 使用 Rust 官方格式（`cargo fmt`）
- 遵循 Clippy 建议
- 函数和类型添加必要的文档注释
- 错误处理使用 `color_eyre::Result` 或 `?` 运算符
- 异步代码使用 `tokio` 运行时
- 日志使用 `tracing` 宏（`tracing::info!`、`tracing::error!` 等）

## 测试策略

- 单元测试：纯逻辑函数使用 `#[test]` 标注
- 集成测试：`tests/` 目录下放置端到端测试
- 使用 `tempfile` 创建临时文件进行测试
- 测试命名使用 `snake_case`，描述行为