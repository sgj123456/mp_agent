# mp_agent — 开发指南

> 描述项目贡献流程、代码规范、测试策略和扩展方式。

## 目录

1. [项目概览](./01-overview.md)
2. [架构设计](./02-architecture.md)
3. [工具系统](./03-tools.md)
4. [权限系统](./04-permission.md)
5. [MCP 集成](./05-mcp.md)
6. [技能系统](./06-skills.md)
7. [UI 组件](./07-ui.md)
8. [配置与运行](./08-config.md)
9. [开发指南](./09-development.md) ← 你在这里

---

## 1. 快速上手

```bash
# 克隆仓库
git clone <repo-url>
cd mp_agent

# 复制 .env 示例并填写
cp .env.example .env   # 如存在
# 编辑 .env 填入 OPENAI_API_KEY 等

# 调试构建
cargo build

# 运行
cargo run
```

## 2. 项目结构

```
mp_agent/
├── Cargo.toml              # 依赖与包配置
├── Cargo.lock              # 锁定依赖版本
├── README.md               # 项目简介
├── .env                    # 环境变量（不应提交）
├── .gitignore              # 忽略规则
├── docs/                   # 项目文档（本目录）
├── src/
│   ├── main.rs             # 入口：初始化 TUI、配置、事件循环
│   ├── app.rs              # 应用状态：键盘事件处理、事件消费、界面绘制、权限审批
│   ├── agent.rs            # AI Agent：流式聊天、工具调用循环、消息管理、权限请求
│   ├── config.rs           # 配置：从 .env 加载 API 密钥、模型等
│   ├── mcp.rs              # MCP 管理器：连接外部 MCP 服务器、工具映射
│   ├── permission.rs       # 权限管理：操作类型、规则匹配、路径处理
│   ├── error.rs            # 错误处理：color-eyre 钩子安装
│   ├── agent/
│   │   ├── tools.rs        # 原生工具定义与执行
│   │   └── skill.rs        # 技能加载、AGENTS.md 读取、系统提示构建
│   └── ui/
│       ├── chat.rs         # 聊天区域：消息渲染、滚动条、流式预览
│       ├── input.rs        # 输入区域：命令行编辑、历史、Tab 补全
│       ├── markdown.rs     # Markdown 渲染器：pulldown-cmark → Ratatui
│       └── layout.rs       # 布局工具
├── tests/                  # 集成测试（当前为空）
└── target/                 # 构建产物（由 .gitignore 忽略）
```

## 3. 代码规范

### 3.1 Rust 风格

- 遵循 Rust 官方风格指南（`cargo fmt` 自动格式化）
- 使用 `rustfmt` 和 `clippy` 进行代码质量检查
- 命名约定：
  - 类型：`PascalCase`（如 `ChatArea`, `PendingPermission`）
  - 变量和函数：`snake_case`（如 `streaming_buffer`, `handle_key_event`）
  - 常量：`UPPER_CASE`（如 `OPTIMIZED_PROMPT`）
  - 枚举变体：`PascalCase`（如 `AgentEvent::Token`）

### 3.2 错误处理

- 使用 `color_eyre::Result` 作为返回类型
- 使用 `?` 操作符传播错误
- 在 `main.rs` 中安装全局错误钩子
- 避免裸 `unwrap()`，使用 `?` 或自定义错误消息

### 3.3 异步编程

- 所有 I/O 操作使用 `tokio` 异步版本（`tokio::fs`, `tokio::process`）
- Agent 任务运行在独立的 tokio 任务中
- 使用通道（mpsc/oneshot）在 UI 和 Agent 间通信
- 避免在异步代码中阻塞操作

### 3.4 日志

- 使用 `tracing` 进行结构化日志记录
- 级别选择：
  - `info!`：重要的运行时事件（连接、工具执行）
  - `debug!`：调试信息（参数值、流程细节）
  - `warn!`：非致命问题（技能加载失败）
  - `error!`：严重错误
- 敏感信息（API 密钥）不应出现在日志中

### 3.5 模块组织

- 每个功能模块对应一个 `.rs` 文件
- 子模块放在子目录中（如 `agent/`, `ui/`）
- 在父模块中使用 `pub mod xxx` 声明
- 仅在必要时使用 `pub` 导出
- 内部模块使用 `mod xxx` 私有声明

## 4. 添加新功能

### 4.1 添加新工具

在 `src/agent/tools.rs` 中：

1. 添加工具定义函数（如 `fn my_tool() -> Value`）
2. 在 `native_tool_definitions()` 的向量中加入
3. 在 `native_tool_names()` 的名称列表中加入
4. 在 `execute_native_tool()` 的 match 中添加分支
5. 如果需要权限，在 `permission.rs::needs_permission` 中添加判断
6. 更新 `docs/03-tools.md` 文档

示例：

```rust
fn my_tool() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "my_tool",
            "description": "工具描述",
            "parameters": { /* JSON schema */ }
        }
    })
}
```

### 4.2 添加新 UI 组件

在 `src/ui/` 中：

1. 创建新文件 `src/ui/xxx.rs`
2. 在 `src/ui.rs` 中声明 `pub mod xxx`
3. 在 `App` 结构体中添加字段
4. 在 `App::new()` 中初始化
5. 在 `App::draw()` 中渲染
6. 在 `App::handle_key_event()` 中处理输入

### 4.3 添加新 MCP 服务器

在 `src/app.rs` 的 `App::new()` 中：

```rust
let _tools = mcp.connect(
    "server-name".to_string(),
    "command".to_string(),
    vec!["arg1".to_string(), "arg2".to_string()],
).await;
```

或使用配置文件方式（未来支持）。

### 4.4 添加新 Slash 命令

在 `src/ui/input.rs` 的 `SLASH_COMMANDS` 常量中添加：

```rust
pub const SLASH_COMMANDS: &[(&str, &str)] = &[
    // ...
    ("/newcmd", "New command description"),
];
```

然后在 `app.rs::handle_slash_command()` 中添加处理逻辑。

## 5. 测试

### 5.1 单元测试

当前项目在 `tests/` 目录为空，但框架已就绪。推荐测试策略：

- `src/agent/tools.rs`：工具执行函数可以使用 mock 参数测试
- `src/permission.rs`：权限匹配逻辑（纯函数，易于测试）
- `src/ui/markdown.rs::highlight_row`：词法分析器已有测试
- `src/config.rs`：环境变量加载测试

### 5.2 运行测试

```bash
cargo test
```

### 5.3 集成测试（未来）

使用 `assert_cmd` 或类似库进行 CLI 集成测试：

```rust
use assert_cmd::Command;

#[test]
fn test_help_command() {
    let mut cmd = Command::cargo_bin("mp_agent").unwrap();
    // 需要支持非 TUI 模式测试
}
```

**注意**：由于 TUI 应用的特性，集成测试需要特殊的终端模拟环境。

## 6. 性能优化

### 6.1 流式响应

- 使用 `reqwest` 的字节流 + `eventsource-stream` 解析 SSE
- 使用 `tokio::time::timeout` 检测空闲超时
- 初始超时 30 秒，有数据后缩短到 8 秒

### 6.2 工具调用循环

- 最大迭代次数 20 次，防止无限循环
- 每次工具调用记录耗时
- 超过 1 秒的工具调用在结果中标注

### 6.3 渲染性能

- 每帧 16ms 间隔（约 60 FPS）
- 聊天消息使用即时渲染，不缓存
- 长工具结果自动折叠减少渲染量

### 6.4 内存

- 消息历史保存在内存中，无持久化
- 权限规则在内存中，会话级
- Todo 存储在内存中，会话级

## 7. 调试技巧

### 7.1 日志级别

```bash
# 详细日志
RUST_LOG=debug cargo run

# 仅 Agent 模块
RUST_LOG=mp_agent=debug cargo run

# 仅 MCP 模块
RUST_LOG=mcp=trace cargo run
```

### 7.2 查看日志

```bash
tail -f mp_agent.log
tail -n 200 mp_agent.log | grep -i error
```

### 7.3 单步调试

```bash
cargo build
lldb target/debug/mp_agent
(lldb) break set -n main
(lldb) run
```

### 7.4 网络调试

API 请求可通过 `RUST_LOG=reqwest=trace` 查看 HTTP 请求详情。

### 7.5 TUI 调试

如果 TUI 界面异常：

- 确认终端支持 UTF-8 和真彩色
- 确认终端宽度至少 80 列
- 重启终端模拟器
- 使用 `reset` 命令重置终端状态

## 8. 贡献流程

1. Fork 仓库
2. 创建功能分支（`git checkout -b feature/xxx`）
3. 提交更改（`git commit -m "feat: 添加 xxx 功能"`）
4. 推送到分支（`git push origin feature/xxx`）
5. 创建 Pull Request

### 提交信息格式

遵循约定式提交（Conventional Commits）：

- `feat: 添加新功能`
- `fix: 修复 bug`
- `docs: 文档更新`
- `refactor: 代码重构`
- `chore: 杂项（依赖更新、CI 等）`
- `perf: 性能优化`
- `test: 添加或修改测试`

## 9. 许可证

MIT — 详见 LICENSE 文件。

## 10. 已知限制与 TODO

| 项目 | 状态 | 说明 |
|---|---|---|
| MCP 工具路由 | 未完成 | `execute_native_tool` 中未路由 `mcp_` 前缀工具 |
| MCP 权限审批 | 未完成 | MCP 工具跳过权限检查 |
| MCP 配置化 | 未完成 | 需代码硬配置 MCP 服务器 |
| 消息历史持久化 | 未实现 | 会话结束后聊天记录丢失 |
| Todo 持久化 | 未实现 | 会话结束后 todo 丢失 |
| 权限规则持久化 | 未实现 | 仅内存保存 |
| 配置文件支持 | 未实现 | 仅 .env 环境变量 |
| 主题自定义 | 未实现 | 硬编码颜色 |
| 测试覆盖 | 中 | markdown 和 tools 有单元测试，app/agent 集成测试待补充 |
| 技能热重载 | 未实现 | 需重启加载新技能 |
| 多模型切换 | 未实现 | 运行时不可切换模型 |
| Token 显示 | 部分实现 | 流式计数已实现，详细 prompt/completion 分类在 UI 中未单独展示 |

## 11. 路线图

### 短期（v0.2）

- ✅ 技能系统文档
- ✅ UI 组件文档
- ✅ 配置与运行文档
- ✅ 开发指南文档
- ✅ Token 用量统计（SSE 流式解析 + UI 显示）
- ✅ 上下文建议输入补全
- ✅ 消息队列（处理中继续输入）
- MCP 工具调用路由修复
- MCP 权限审批集成
- 配置文件支持 MCP 服务器注册

### 中期（v0.3）

- 消息历史持久化（SQLite 或文件）
- Todo 持久化
- 权限规则持久化
- 主题自定义（暗色/亮色）
- 更多内置工具（git 操作、HTTP 请求等）
- 更好的测试覆盖

### 长期（v0.4+）

- 多模型切换（运行时）
- 技能热重载
- MCP SSE 传输支持
- MCP 资源读取和订阅
- MCP 提示模板集成
- 插件系统
- 远程 Agent 模式（客户端-服务器架构）