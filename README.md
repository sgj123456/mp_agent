<div align="center">

# mp_agent

> 一个基于终端的 AI 编程助手，使用 Rust + OpenAI 兼容 API + MCP 协议构建

[![Rust Edition](https://img.shields.io/badge/Rust-edition%202024-E34F26?logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](#许可证)
[![MCP](https://img.shields.io/badge/MCP-supported-brightgreen)](#mcp-集成)
[![TUI](https://img.shields.io/badge/TUI-ratatui-527CB7)](https://ratatui.rs/)

[快速开始](#快速开始) · [特性](#特性) · [架构](#架构) · [内置工具](#内置工具) · [MCP 集成](#mcp-集成) · [权限系统](#权限系统) · [路线图](#roadmap) · [许可证](#许可证)

</div>

---

## 特性

- **TUI 界面**：基于 Ratatui + Crossterm 的全终端界面，聊天、输入、状态栏一目了然
- **流式输出**：支持 SSE 流式响应，实时显示 AI 生成的 token
- **工具调用**：内置多种原生工具，AI 可自动调用完成文件操作、命令执行、任务管理等
- **MCP 协议支持**：可连接外部 MCP 服务器，扩展工具生态
- **技能系统**：支持从 `.opencode/skills/` 目录加载自定义技能和 Agent 上下文
- **Markdown 渲染**：支持代码块、表格、引用、列表等 Markdown 格式的终端渲染，使用 `tui-markdown` 库进行语法高亮
- **Slash 命令**：`/help`、`/clear`、`/tools` 等便捷命令 + Tab 自动补全
- **类 Emacs 快捷键**：Ctrl+A/E/U/K/L 等，历史上下翻，滚动查看
- **权限审批**：对写入文件、执行命令等敏感操作实时请求用户确认，支持记住选择（always/deny）
- **任务管理（Todo）**：内置 todo 列表，支持增删改查和优先级管理
- **决策呈现**：当有多种方案时，AI 可以呈现选项供用户选择
- **上下文建议**：根据聊天历史自动提取文件路径、命令、工具名等，输入时 Tab 补全；空输入时显示建议提示，非空输入时 ghost text 自动补全剩余部分
- **消息队列**：等待 AI 响应时可继续输入，消息自动排队发送
- **Token 统计**：实时显示当前会话和本次响应的 token 消耗
- **结果折叠**：长工具调用结果自动折叠，按 `z` 展开/收起

## 架构

```
src/
├── main.rs          # 入口：初始化 TUI、配置、事件循环
├── app.rs           # 应用状态：键盘/鼠标事件处理、界面绘制、权限审批、Agent 事件消费、上下文建议提取
├── agent.rs         # AI Agent：流式聊天、工具调用循环、消息管理、权限/选择请求、todo 管理
├── config.rs        # 配置：从 .env 加载 API 密钥、模型等
├── mcp.rs           # MCP 管理器：连接外部 MCP 服务器、工具映射
├── permission.rs    # 权限管理：操作类型、规则匹配、路径处理、记忆决策
├── error.rs         # 错误处理：color-eyre 钩子安装
├── agent/
│   ├── request.rs   # SSE 流式请求解析：token 流解析、usage 统计、事件推送
│   ├── tools.rs     # 原生工具定义与执行（bash、文件操作、搜索、todo 管理、present_choices）
│   └── skill.rs     # 技能加载、AGENTS.md 读取、系统提示构建
└── ui/
    ├── chat.rs      # 聊天区域：消息渲染、滚动条、流式预览、工具结果折叠
    ├── input.rs     # 输入区域：命令行编辑、历史、Tab 补全、Slash 命令、上下文建议
    ├── markdown.rs  # Markdown 渲染器：pulldown-cmark → Ratatui（含语法高亮）
    └── layout.rs    # 布局工具
```

## 内置工具

| 工具名 | 描述 |
|---|---|
| `bash` | 执行 bash 命令并返回输出（支持 workdir） |
| `read_file` | 读取文件内容（支持 offset/limit 分页） |
| `write_file` | 写入文件内容（创建或覆盖） |
| `edit_file` | 按精确匹配替换文件中的文本片段（类似 Anthropic 的 edit 工具，优先使用） |
| `glob` | 按 glob 模式查找文件 |
| `grep` | 按正则搜索文件内容（支持 include 文件过滤） |
| `list_directory` | 列出目录内容，标注文件/目录 |
| `add_todo` | 添加任务到 todo 列表（支持优先级 low/medium/high） |
| `update_todo` | 更新 todo 的状态、描述或优先级 |
| `list_todos` | 列出所有 todo 及其状态和优先级 |
| `remove_todo` | 从列表中删除一个 todo |
| `present_choices` | 当有多种方案时呈现选项，让用户选择 |

### MCP 工具

如果 MCP 服务器已配置，额外的工具会以服务器名称为前缀（例如 `git_status`、`db_query`），与内置工具一起出现在 Agent 的工具列表中。Agent 可以根据任务需要调用这些外部工具。MCP 工具的错误被视为暂时性的——Agent 会检查错误消息并在必要时用修正后的参数重试。

### Agent 工具使用指南

mp_agent 附带了一套完整的工具使用指南，通过系统提示注入，引导 AI 遵循最佳实践：

- **文件编辑**：优先使用 `edit_file` 而非 `write_file`；编辑前先读取文件；提供足够的上下文使 old_string 唯一
- **Bash 命令**：使用相对路径；检查退出码；合理链式命令
- **搜索导航**：先用 `glob` 找文件，再用 `grep` 搜内容；用 `list_directory` 探索结构
- **最佳实践**：读写前后验证、小步迭代、测试确认、语法检查

## MCP 集成

通过 `McpManager` 可以连接任意 MCP 子进程服务器，自动拉取工具列表并映射为 OpenAI 工具格式，使 Agent 能够使用外部 MCP 工具。

## 权限系统

当 AI 触发敏感操作时（写入文件、执行命令），会弹出权限请求提示：

```
【Permission】WRITE /path/to/file | write_file  [y]es [a]lways [n]o [d]eny [Esc]
```

- **y / n**：本次允许 / 拒绝
- **a / d**：允许 / 拒绝，并记住规则（基于路径前缀），后续同类操作自动决策
- **Esc**：默认拒绝

权限规则保存在内存中，会话结束后自动清除。

---

<div align="center">

## 快速开始

</div>

### 环境要求

- Rust 1.76+（edition 2024）
- 一个 OpenAI 或兼容 OpenAI API 的模型 endpoint

### 安装

```bash
git clone <repo-url>
cd mp_agent
cargo build --release
```

### 配置

编辑项目根目录的 `.env` 文件，填写你的 API 信息：

```env
OPENAI_API_KEY=sk-your-api-key
OPENAI_BASE_URL=https://api.openai.com/v1
OPENAI_MODEL=gpt-4o
OPENAI_MAX_TOKENS=5000
```

> 💡 如果你使用的是兼容 OpenAI API 的服务（如 InternLM、DeepSeek 等），修改 `OPENAI_BASE_URL` 和 `OPENAI_MODEL` 即可。

### 运行

```bash
cargo run
```

或者使用 release 版本：

```bash
./target/release/mp_agent
```

## 使用说明

### 基本交互

- 直接输入消息，按 **Enter** 发送
- AI 回复时按 **Esc** 可取消当前生成
- 再次按 **Esc** 退出程序

### Slash 命令

| 命令 | 说明 |
|---|---|
| `/help` | 显示帮助 |
| `/clear` | 清除聊天记录 |
| `/model` | 显示或切换当前模型 |
| `/tools` | 列出可用工具 |
| `/exit` | 退出程序 |
| `/history` | 查看输入历史提示 |

输入 `/` 时会自动显示命令建议，按 **Tab** 补全，**↑/↓** 切换建议。

### 快捷键

| 快捷键 | 功能 |
|---|---|
| `Ctrl+C` | 强制退出 |
| `Ctrl+A` | 光标移至行首 |
| `Ctrl+E` | 光标移至行尾 |
| `Ctrl+U` | 清空输入（删除光标前所有字符） |
| `Ctrl+K` | 删除光标后所有字符 |
| `Ctrl+L` | 清空聊天 |
| `Ctrl+D` | 显示系统提示 |
| `←/→` | 光标左右移动 |
| `↑/↓` | 输入历史 / 滚动聊天 |
| `PageUp/PageDown` | 翻页滚动 |
| `Tab` | 命令/上下文建议补全 |
| `Esc` | 取消生成 / 退出程序 |

### 上下文建议

输入时会根据当前聊天历史自动提供上下文建议，按 **Tab** 补全：

- **文件路径**：消息中出现的含 `/` 或 `.` 的路径字符串
- **命令前缀**：以 `cargo`、`git`、`ls`、`cat` 开头的命令
- **工具名**：已调用的工具名称（如 `/tools bash`）
- **引号内容**：消息中的引号内文本

建议通过以下两种方式呈现：

1. **Ghost text（幽灵文字）**：当输入与某条建议部分匹配时，输入框内自动以灰色文字显示剩余部分，按 Tab 可直接接受完整建议。
2. **Tab 补全面板**：输入 `/` 或空白时显示 slash 命令列表；输入非 slash 内容时匹配上下文建议。按 **↑/↓** 选择，**Tab** 接受。

建议最多显示 20 条。

### 消息队列

等待 AI 响应时仍可继续输入，不会阻塞：

- 按 **Enter** 将当前输入加入排队队列
- 状态栏显示 `Processing... (N queued)` 提示排队数量
- 当前轮次完成后自动发送下一条排队消息
- 按 **Esc** 取消处理并清空所有排队消息

### 技能系统

Agent 会自动从以下路径加载技能文件（`.md` / `.txt` / `.skill`）：

- `./.opencode/skills/`
- `$HOME/.config/opencode/skills/`

技能文件格式：

```
技能名称
技能描述
技能正文内容...
```

同时会读取当前目录下的 `AGENTS.md` 作为项目上下文注入系统提示。

## 工作流程

1. 用户输入消息 → `App` 通过 channel 发送 `AgentCommand::SendMessage`
   - 如果 Agent 正在处理，输入加入 `pending_messages` 队列
2. `Agent` 构建请求（系统提示 + AGENTS.md 上下文 + 技能 + 工具定义），发起流式 SSE 请求
3. 响应流解析为 token → `AgentEvent::Token` + `AgentEvent::TokenUsage` → UI 实时渲染 + token 统计
4. 如果 AI 请求工具调用 → 检查是否需要权限 → 如需则弹出审批 → 执行对应工具 → 结果返回给 Agent → 继续对话
5. 如果 AI 需要用户做选择（`present_choices`）→ 弹出选项列表 → 用户选择 → 结果返回给 Agent
6. 完成后发送 `AgentEvent::MessageComplete` → 消息加入聊天历史
7. `update_context_suggestions()` 提取上下文建议 → 更新输入建议区
8. `drain_pending_messages()` 排空排队消息 → 发送下一条

## 开发

```bash
# 开发构建
cargo build

# 运行测试
cargo test

# 检查代码
cargo clippy

# 格式化
cargo fmt
```

---

## Roadmap

以下是 mp_agent 的阶段性规划，按优先级排列：

### v0.1 — 基础可用（当前阶段 ✅）

- [x] TUI 界面（Ratatui + Crossterm）
- [x] 流式 SSE 聊天 + OpenAI 兼容 API
- [x] 内置工具集（bash / 文件操作 / 搜索 / todo / present_choices）
- [x] 权限审批系统（y/n/always/deny + 记忆）
- [x] Markdown 渲染（代码块 / 表格 / 引用 / 列表）
- [x] Slash 命令 + Tab 补全
- [x] 类 Emacs 快捷键
- [x] 技能系统（从 `.opencode/skills/` 和 `AGENTS.md` 加载上下文）
- [x] MCP 客户端集成（连接外部 MCP 服务器）

### v0.2 — 体验增强

- [ ] **对话历史持久化**：将聊天记录保存到本地文件，支持恢复会话
- [ ] **多会话管理**：在同一实例中切换多个聊天对话
- [ ] **配置 UI**：在 TUI 内直接编辑 `.env` 配置（API Key、模型等）
- [ ] **主题系统**：支持亮色 / 暗色 / 自定义配色方案
- [ ] **系统托盘 / 后台模式**：最小化到托盘，CLI 方式唤醒
- [ ] **通知系统**：长时间任务完成时终端通知
- [ ] **工具调用历史可视化**：以树状图展示工具调用链

### v0.3 — 能力扩展

- [ ] **多模型支持**：同时连接多个模型 endpoint，按需切换或自动回退
- [ ] **函数调用流式中间件**：支持流式解析工具调用参数
- [ ] **自定义工具注册**：允许用户通过 YAML/JSON 声明自定义工具
- [ ] **MCP 服务器模式**：将 mp_agent 本身作为 MCP Server 暴露给其他客户端
- [ ] **远程 Agent 模式**：通过 WebSocket / SSH 连接远程 Agent 实例
- [ ] **插件系统**：支持加载 Rust 编译的插件或 WASM 模块

### v0.4 — 智能化与工程化

- [ ] **Agent 记忆系统**：长期记忆 + 短期工作记忆，支持 RAG 检索
- [ ] **自动测试与验证**：AI 执行命令后自动运行相关测试并解释结果
- [ ] **代码审查模式**：作为代码审查 Agent 集成到 CI/CD 流程
- [ ] **项目脚手架**：根据自然语言描述生成项目骨架
- [ ] **多 Agent 协作**：多个 Agent 角色（Coder / Reviewer / Tester）协同工作
- [ ] **性能分析面板**：Token 消耗统计、延迟监控、工具调用耗时热力图

### v0.5 — 生态与社区

- [ ] **插件市场**：社区共享技能 / 工具 / 配置模板
- [ ] **跨平台二进制分发**：Homebrew / Scoop / AUR 包
- [ ] **Web UI 备选**：基于 Tauri / Yew 的图形界面
- [ ] **VS Code / JetBrains 插件**：在 IDE 内直接调用 mp_agent
- [ ] **文档站点**：托管完整的 API 文档和教程
- [ ] **Benchmark 套件**：与其他同类 Agent 工具对比评测

---

<div align="center">

## 许可证

MIT · [回到顶部](#mp_agent)

</div>
