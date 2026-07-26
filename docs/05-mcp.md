# mp_agent — MCP 集成

> 描述如何通过 MCP (Model Context Protocol) 协议连接外部工具服务器，扩展 Agent 能力。

## 目录

1. [项目概览](./01-overview.md)
2. [架构设计](./02-architecture.md)
3. [工具系统](./03-tools.md)
4. [权限系统](./04-permission.md)
5. [MCP 集成](./05-mcp.md) ← 你在这里
6. [技能系统](./06-skills.md)
7. [UI 组件](./07-ui.md)
8. [配置与运行](./08-config.md)
9. [开发指南](./09-development.md)

---

## 1. 什么是 MCP

MCP (Model Context Protocol) 是由 Anthropic 开发的开放协议，用于让 AI 模型与安全地连接外部工具和数据源。mp_agent 通过 `rmcp` 库作为 MCP 客户端，可以连接运行在子进程中的 MCP 服务器，自动发现并调用其暴露的工具。

## 2. 架构

```
mp_agent (MCP Client)
    │
    │  stdio transport (TokioChildProcess)
    ▼
MCP Server (子进程)
    │
    │  工具实现
    ▼
外部服务 (文件系统、数据库、Git、API 等...)
```

- MCP 服务器作为 mp_agent 的子进程启动，通过 stdio 传输 JSON-RPC 消息
- 使用 `rmcp` 库处理协议层（初始化、工具列表、工具调用）
- MCP 工具自动映射为 OpenAI function-calling 格式，前缀 `mcp_`
- 所有 MCP 工具在 Agent 的工具调用循环中统一处理

## 3. McpManager

`McpManager`（`src/mcp.rs`）管理多个 MCP 服务器连接：

### 3.1 主要方法

| 方法 | 说明 |
|---|---|
| `new()` | 创建空的 MCP 管理器 |
| `connect(name, command, args)` | 启动 MCP 子进程并连接，返回工具列表 |
| `all_mcp_tools()` | 获取所有已连接 MCP 服务器的工具引用 |
| `call_tool(name, args)` | 调用指定 MCP 工具，返回结果字符串 |
| `disconnect_all()` | 断开所有 MCP 连接 |

### 3.2 连接流程

```rust
let mut mcp = McpManager::new();
let tools = mcp.connect(
    "filesystem".to_string(),
    "npx".to_string(),
    vec!["@anthropic/mcp-server-filesystem".to_string(), "/tmp".to_string()]
).await?;
// tools 包含该服务器暴露的所有工具
```

### 3.3 工具映射

MCP 工具定义通过 `mcp_tools_to_openai` 函数转换为 OpenAI function-calling 格式：

```rust
pub fn mcp_tools_to_openai(mcp_tools: &[McpTool]) -> Vec<Value> {
    mcp_tools
        .iter()
        .map(|tool| {
            let input_schema = tool.input_schema.as_ref();
            json!({
                "type": "function",
                "function": {
                    "name": format!("mcp_{}", tool.name),
                    "description": tool.description.as_deref().unwrap_or("MCP tool"),
                    "parameters": input_schema
                }
            })
        })
        .collect()
}
```

- 工具名前缀 `mcp_` 以避免与原生工具名冲突
- 描述和参数 schema 直接从 MCP 工具定义透传

## 4. 工具调用

当 AI 请求调用 MCP 工具时：

1. Agent 在 `get_tools()` 中获取原生工具 + MCP 工具
2. AI 响应包含 `mcp_tool_name` 的 tool_call
3. `needs_permission` 返回 `None`（MCP 工具当前不触发权限检查）
4. `execute_native_tool` 中没有匹配分支，当前会返回 `"Unknown tool"`
5. **注意**：MCP 工具调用需要在 `execute_native_tool` 中添加分支，通过 `McpManager::call_tool` 转发

### 当前限制

MCP 工具的调用尚未在 `execute_native_tool` 中路由。完整集成需要在 `agent.rs` 的工具执行分支中添加：

```rust
if tc.name.starts_with("mcp_") {
    let mcp_tool_name = tc.name.strip_prefix("mcp_").unwrap();
    self.mcp.call_tool(mcp_tool_name, args).await
} else {
    // 原生工具执行
}
```

## 5. 配置 MCP 服务器

当前 MCP 连接需要在代码中硬配置（`App::new` 中）。未来版本将通过配置文件支持动态 MCP 服务器注册。

示例配置（伪代码）：

```toml
# 未来可能的 config.toml
[[mcp_servers]]
name = "filesystem"
command = "npx"
args = ["@anthropic/mcp-server-filesystem", "/home/user/projects"]

[[mcp_servers]]
name = "git"
command = "mcp-server-git"
args = []
```

## 6. 可用的 MCP 服务器

以下是一些流行的 MCP 服务器，可以与 mp_agent 配合使用：

| 服务器 | 用途 | 启动命令 |
|---|---|---|
| `@anthropic/mcp-server-filesystem` | 文件系统操作 | `npx @anthropic/mcp-server-filesystem <path>` |
| `mcp-server-git` | Git 操作 | `mcp-server-git` |
| `mcp-server-postgres` | PostgreSQL 查询 | `npx mcp-server-postgres <connection-string>` |
| `mcp-server-slack` | Slack 交互 | `npx mcp-server-slack` |
| `mcp-server-github` | GitHub API | `npx mcp-server-github` |

## 7. 协议细节

MCP 使用 JSON-RPC 2.0  over stdio：

1. **初始化**：客户端发送 `initialize` 消息，服务器响应能力和工具列表
2. **工具调用**：客户端发送 `tools/call` 请求，服务器执行并返回结果
3. **通知**：服务器可发送异步通知（如资源变更）
4. **ping/健康检查**：保持连接活跃

`rmcp` 库封装了这些协议细节，提供高层 API：

```rust
let service = ().serve(transport).await?;
let tools = service.list_tools(None).await?;
let result = service.call_tool(params).await?;
```

## 8. 错误处理

MCP 连接和调用中的错误通过 `Result<String, String>` 返回：

- 连接失败：记录日志，返回错误描述
- 工具调用失败：包装为错误字符串，作为工具结果返回给 AI
- 连接断开：`disconnect_all` 优雅关闭所有服务

## 9. 与原生工具的对比

| 维度 | 原生工具 | MCP 工具 |
|---|---|---|
| 实现 | Rust 内置 | 外部进程（Node/Python/Go 等） |
| 性能 | 直接调用，低延迟 | 进程间通信，略高延迟 |
| 安全 | 权限审批 | 由 MCP 服务器自身约束 |
| 生态 | 固定 12 个工具 | 丰富的 MCP 服务器生态 |
| 集成 | 代码级集成 | 协议级集成，自动发现 |
| 权限 | 需要审批 | 当前跳过审批 |

## 10. 未来扩展

- 配置文件驱动 MCP 服务器注册
- MCP 工具纳入统一权限审批框架
- 支持 SSE 传输的 MCP 服务器连接
- MCP 资源（resource）读取和订阅
- MCP 提示模板（prompt）集成