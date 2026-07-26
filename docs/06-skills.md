# mp_agent — 技能系统

> 描述如何自定义技能、配置 Agent 上下文和注入项目级知识。

## 目录

1. [项目概览](./01-overview.md)
2. [架构设计](./02-architecture.md)
3. [工具系统](./03-tools.md)
4. [权限系统](./04-permission.md)
5. [MCP 集成](./05-mcp.md)
6. [技能系统](./06-skills.md) ← 你在这里
7. [UI 组件](./07-ui.md)
8. [配置与运行](./08-config.md)
9. [开发指南](./09-development.md)

---

## 1. 什么是技能

技能是 mp_agent 的**自定义知识注入机制**，允许你将项目规范、编码惯例、最佳实践等文档化内容以文件形式存放在特定目录，由 Agent 在启动时自动加载并注入系统提示，使 AI 在对话中始终遵循这些约束。

典型用途：

- 项目编码规范（命名约定、目录结构、代码风格）
- API 设计指南
- 安全审查清单
- 部署流程文档
- 团队约定（Git 分支策略、PR 模板等）

## 2. 技能文件位置

mp_agent 按以下顺序加载技能文件：

| 优先级 | 路径 | 说明 |
|---|---|---|
| 1 | `./.opencode/skills/` | 当前项目下的技能目录（项目级） |
| 2 | `$HOME/.config/opencode/skills/` | 用户全局技能目录（全局级） |

项目级技能优先于全局技能加载。如果两个位置存在同名技能，两者都会被加载，项目级技能出现在系统提示的后部（因此优先级更高）。

## 3. 技能文件格式

技能文件可以是 `.md`、`.txt` 或 `.skill` 扩展名。文件格式约定：

```
第一行：技能名称
第二行：技能描述
第三行及以后：技能正文内容...
```

示例（`.opencode/skills/rust-conventions.skill`）：

```
Rust 编码约定
本项目 Rust 代码的风格与安全指南

1. 所有公共 API 必须使用 `pub` 显式导出
2. 错误处理使用 `color_eyre::Result`，避免裸 `unwrap()`
3. 文件修改优先使用 `edit_file` 而非 `write_file`
4. 每个新模块必须包含 `#[cfg(test)]` 测试
5. 函数命名：蛇形命名法（snake_case），类型使用大驼峰（PascalCase）
```

**说明**：

- 名称和描述会被解析为技能的元数据，在系统提示中作为小标题显示
- 正文内容直接作为 Markdown 注入，支持所有 Markdown 语法（代码块、表格、列表等）
- 如果文件只有单行，则该行同时作为名称和描述，正文为空

## 4. AGENTS.md 项目上下文

除了技能文件，mp_agent 还会自动读取当前工作目录下的 `AGENTS.md` 文件（如果存在），将其内容作为项目上下文注入系统提示。

`AGENTS.md` 是一个社区约定文件名，用于描述项目的整体信息、技术栈、开发流程等。推荐包含以下内容：

```markdown
# 项目名称

项目简介

## 技术栈

- Rust (edition 2024)
- Ratatui + Crossterm (TUI)
- async-openai (AI API)

## 构建与运行

```bash
cargo build --release
cargo run
```

## 目录结构

```
src/
├── main.rs      # 入口
├── app.rs       # 应用状态
└── agent.rs     # AI Agent
```

## 代码约定

- 使用 `edit_file` 进行精确修改
- 修改后运行 `cargo check` 验证
```

AGENTS.md 的内容在系统提示中出现在技能之前，作为项目级上下文的基础层。

## 5. 系统提示构建流程

启动时，`App::new()` 调用 `skill::build_system_prompt()` 按以下顺序组装系统提示：

```
1. 优化提示词（OPTIMIZED_PROMPT）— 工具列表 + 使用指南 + 最佳实践
2. AGENTS.md 内容（如果存在）— 项目上下文
3. 所有加载的技能 — 按加载顺序追加
```

最终形成的系统提示结构：

```
You are mp_agent, an AI-powered coding assistant...

## Available Tools
- read_file, write_file, edit_file, ...

## Tool Guidelines
- Read before write, prefer edit_file, ...

## Best Practices
- Read before write, iterate, test, ...

## Project Context (from AGENTS.md)
<AGENTS.md 内容>

## Available Skills
### 技能名称
技能描述
技能正文内容...

### 另一个技能
...
```

## 6. 技能加载行为

- 启动时一次性加载，运行时不重新加载（除非重启）
- 加载失败的技能会被跳过并记录警告日志
- 空目录或不存在目录静默处理
- 技能内容完整读入内存，作为系统提示的一部分

## 7. 创建技能的步骤

1. 在项目根目录创建 `.opencode/skills/` 目录（如不存在）
2. 在目录下创建技能文件（推荐使用 `.skill` 扩展名）
3. 第一行写技能名称，第二行写描述，后续写正文
4. 重启 mp_agent 使新技能生效
5. 在聊天中输入 `/tools` 确认技能已加载（系统提示中会包含）

## 8. 技能使用建议

### 给开发者

- **保持简洁**：技能内容应精炼，避免过长导致系统提示超出 token 限制
- **分层组织**：通用规范放全局技能目录，项目特定规范放项目级技能
- **及时更新**：技能文件变更需要重启 Agent 才能生效
- **避免冲突**：如果多个技能对同一事项有不同约束，靠后的技能会覆盖前面的（但 AI 可能产生混淆）

### 给 AI

- 系统提示中包含了所有加载的技能和项目上下文
- 在生成代码或建议时，优先遵循技能中的约束
- 如果技能间有冲突，以项目级技能为准
- 使用 `edit_file` 进行小范围修改，符合技能中"精准替换"的约定

## 9. 当前限制

- 技能仅在启动时加载，运行时无法动态更新
- 技能内容无 token 计数检查，过长可能导致截断
- 无技能版本管理，文件修改后需重启
- 技能文件解析简单（按行分割），不支持 YAML frontmatter 等元数据格式

## 10. 未来扩展

- 支持技能热重载（文件变更自动重新加载）
- 支持 YAML frontmatter 元数据格式
- 按技能类别/标签分组加载
- 技能 token 计数与截断策略
- 通过 slash 命令手动重新加载技能
- 技能市场：共享和下载社区技能