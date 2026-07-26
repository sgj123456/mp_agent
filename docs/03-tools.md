# mp_agent — 工具系统

> 描述 Agent 可用的内置工具及其调用方式。

## 目录

1. [项目概览](./01-overview.md)
2. [架构设计](./02-architecture.md)
3. [工具系统](./03-tools.md) ← 你在这里
4. [权限系统](./04-permission.md)
5. [MCP 集成](./05-mcp.md)
6. [技能系统](./06-skills.md)
7. [UI 组件](./07-ui.md)
8. [配置与运行](./08-config.md)
9. [开发指南](./09-development.md)

---

## 1. 工具概览

mp_agent 内置 **12 个原生工具**，覆盖文件操作、命令执行、搜索、任务管理和多选择决策。所有工具通过 OpenAI 的 function-calling 机制暴露给 AI 模型。

| 工具名 | 类别 | 需要权限 | 描述 |
|---|---|---|---|
| `bash` | 执行 | ✅ Execute | 执行 bash 命令 |
| `read_file` | 文件 | — | 读取文件内容（支持分页） |
| `write_file` | 文件 | ✅ Write | 写入文件内容 |
| `edit_file` | 文件 | ✅ Write | 精确替换文本片段 |
| `glob` | 搜索 | — | 按 glob 模式查找文件 |
| `grep` | 搜索 | — | 按正则搜索文件内容 |
| `list_directory` | 文件 | — | 列出目录内容 |
| `add_todo` | 任务 | — | 添加任务 |
| `update_todo` | 任务 | — | 更新任务 |
| `list_todos` | 任务 | — | 列出任务 |
| `remove_todo` | 任务 | — | 删除任务 |
| `present_choices` | 决策 | — | 向用户呈现多选项 |

## 2. 工具定义

所有工具定义在 `src/agent/tools.rs` 中，通过 `native_tool_definitions()` 返回符合 OpenAI function-calling 格式的 JSON 向量。

### 2.1 bash

```json
{
  "type": "function",
  "function": {
    "name": "bash",
    "description": "Execute a bash command and return its stdout/stderr output.",
    "parameters": {
      "type": "object",
      "properties": {
        "command": { "type": "string", "description": "The bash command to execute" },
        "workdir": { "type": "string", "description": "Working directory (optional)" }
      },
      "required": ["command"]
    }
  }
}
```

**执行行为**：
- 通过 `bash -c` 执行命令
- 可选 `workdir` 参数设置工作目录
- 合并 stdout 和 stderr 输出
- 非零退出码时添加 `(exit code: N)` 前缀
- 无输出时返回 `(no output)`

**权限**：触发 `PermissionOp::Execute`，弹出审批提示。

### 2.2 read_file

```json
{
  "type": "function",
  "function": {
    "name": "read_file",
    "description": "Read the contents of a file.",
    "parameters": {
      "type": "object",
      "properties": {
        "path": { "type": "string", "description": "File path" },
        "offset": { "type": "integer", "description": "Start line (0-indexed, optional)" },
        "limit": { "type": "integer", "description": "Max lines (optional)" }
      },
      "required": ["path"]
    }
  }
}
```

**执行行为**：
- 默认 `offset=0`，`limit=2000`
- 返回格式：`File: {path} ({total} lines, showing {start}-{end})` + 内容
- 截断时添加 `(... truncated, showing X of Y lines)` 提示
- offset 超出范围时返回 `(offset beyond file length)`

### 2.3 write_file

```json
{
  "type": "function",
  "function": {
    "name": "write_file",
    "description": "Write content to a file.",
    "parameters": {
      "type": "object",
      "properties": {
        "path": { "type": "string", "description": "File path" },
        "content": { "type": "string", "description": "Content to write" }
      },
      "required": ["path", "content"]
    }
  }
}
```

**执行行为**：
- 如果文件存在，记录原大小并显示 delta（`+N` / `-N`）
- 如果文件不存在，创建新文件
- 返回 `Wrote {path} ({N} bytes, was {M} bytes, delta ±K)` 或 `Created {path} ({N} bytes)`

**权限**：触发 `PermissionOp::Write`，弹出审批提示。

### 2.4 edit_file（推荐的文件修改方式）

```json
{
  "type": "function",
  "function": {
    "name": "edit_file",
    "description": "Replace text in a file by matching an exact substring and replacing it with new text. Use this for targeted edits instead of rewriting the whole file.",
    "parameters": {
      "type": "object",
      "properties": {
        "path": { "type": "string", "description": "File path" },
        "old_string": { "type": "string", "description": "Text to find (exact match, must be unique)" },
        "new_string": { "type": "string", "description": "Replacement text" }
      },
      "required": ["path", "old_string", "new_string"]
    }
  }
}
```

**执行行为**：
1. 读取文件完整内容
2. 精确匹配 `old_string`
3. 如果未找到，报错
4. 如果出现多次，报错（要求更唯一的匹配）
5. 使用 `replacen` 替换第一次匹配
6. 回写文件
7. 返回 diff 格式的修改摘要

**权限**：触发 `PermissionOp::Write`，弹出审批提示。

**设计哲学**：类似于 Anthropic 的 edit 工具，优先使用精确替换而非全文重写，减少权限确认的噪声。

### 2.5 glob

```json
{
  "type": "function",
  "function": {
    "name": "glob",
    "description": "Find files matching a glob pattern.",
    "parameters": {
      "type": "object",
      "properties": {
        "pattern": { "type": "string", "description": "Glob pattern (e.g. '**/*.rs')" }
      },
      "required": ["pattern"]
    }
  }
}
```

**执行行为**：
- 使用 `glob` crate 匹配模式
- 返回匹配路径的换行分隔列表
- 无匹配返回 `No files matched the pattern`

### 2.6 grep

```json
{
  "type": "function",
  "function": {
    "name": "grep",
    "description": "Search file contents using regex.",
    "parameters": {
      "type": "object",
      "properties": {
        "pattern": { "type": "string", "description": "Regex pattern" },
        "include": { "type": "string", "description": "File pattern to include (e.g. '*.rs')" },
        "path": { "type": "string", "description": "Directory to search" }
      },
      "required": ["pattern"]
    }
  }
}
```

**执行行为**：
- 使用 `regex` crate 编译模式
- 默认 `include=*`（所有文件），`path=.`（当前目录）
- 通过 `glob` 递归查找匹配 `include` 的文件
- 对每个文件逐行匹配正则
- 返回格式：`{path}:{line_num}: {line}`
- 无匹配返回 `No matches found`

### 2.7 list_directory

```json
{
  "type": "function",
  "function": {
    "name": "list_directory",
    "description": "List directory contents.",
    "parameters": {
      "type": "object",
      "properties": {
        "path": { "type": "string", "description": "Directory path" }
      },
      "required": ["path"]
    }
  }
}
```

**执行行为**：
- 使用 `tokio::fs::read_dir` 异步读取
- 目录条目标记 `[DIR]` 或 `[FILE]`
- 按字母序排序
- 空目录返回 `Directory is empty`

## 3. 任务管理工具

这是一组特殊的工具，由 Agent 内部的 `TodoStore` 管理，不触发权限检查。

### 3.1 add_todo

```json
{
  "type": "function",
  "function": {
    "name": "add_todo",
    "description": "Add a new task to the todo list.",
    "parameters": {
      "type": "object",
      "properties": {
        "description": { "type": "string", "description": "Task description" },
        "priority": { "type": "string", "enum": ["low", "medium", "high"], "description": "Task priority (default: medium)" }
      },
      "required": ["description"]
    }
  }
}
```

### 3.2 update_todo

```json
{
  "type": "function",
  "function": {
    "name": "update_todo",
    "description": "Update an existing todo's status, description, or priority.",
    "parameters": {
      "type": "object",
      "properties": {
        "id": { "type": "integer", "description": "Todo ID" },
        "status": { "type": "string", "enum": ["pending", "done"], "description": "New status" },
        "description": { "type": "string", "description": "New description" },
        "priority": { "type": "string", "enum": ["low", "medium", "high"], "description": "New priority" }
      },
      "required": ["id"]
    }
  }
}
```

### 3.3 list_todos

```json
{
  "type": "function",
  "function": {
    "name": "list_todos",
    "description": "List all current todos with their status and priority.",
    "parameters": {
      "type": "object",
      "properties": {}
    }
  }
}
```

### 3.4 remove_todo

```json
{
  "type": "function",
  "function": {
    "name": "remove_todo",
    "description": "Remove a todo from the list.",
    "parameters": {
      "type": "object",
      "properties": {
        "id": { "type": "integer", "description": "Todo ID to remove" }
      },
      "required": ["id"]
    }
  }
}
```

**TodoStore 实现**：
- 内存存储，会话级持久化
- 自增 ID 从 1 开始
- 支持按 ID 更新/删除
- 列表返回时带 emoji 标记（✅/⬜）和优先级图标（🔴/🟡/🟢）

## 4. 多选择工具

### present_choices

```json
{
  "type": "function",
  "function": {
    "name": "present_choices",
    "description": "When multiple approaches or solutions are possible, present options to the user and let them choose. Call this whenever you're uncertain about which direction to take.",
    "parameters": {
      "type": "object",
      "properties": {
        "choices": {
          "type": "array",
          "items": { "type": "string" },
          "description": "Array of approach descriptions (at least 2, at most 9)"
        }
      },
      "required": ["choices"]
    }
  }
}
```

**交互流程**：
1. AI 调用 `present_choices` 工具，传入选项数组
2. Agent 发送 `AgentEvent::ChoiceRequired`
3. App 在状态栏渲染选项列表（1-N 编号）
4. 用户输入数字选择，或输入自定义文本后按 `c` 确认
5. 按 `Esc` 取消选择
6. 通过 oneshot 通道返回 `ChoiceResult { selected_index, custom_text }`

## 5. 工具执行流程

```
AI 响应包含 tool_calls
    ↓
Agent::send_message 解析工具调用
    ↓
遍历每个 tool_call
    ├── 判断是否需要权限 (needs_permission)
    │   ├── 需要 → request_permission → oneshot 等待决策
    │   │   ├── Allow → 执行工具
    │   │   └── Deny → 返回 "Permission denied"
    │   └── 不需要 → 直接执行
    │
    ├── todo 工具 → execute_todo_tool (Agent 内部)
    ├── present_choices → execute_choices_tool (Agent 内部)
    └── 其他原生工具 → execute_native_tool
    │
    ↓
发送 AgentEvent::ToolCallStart
执行工具 (异步)
发送 AgentEvent::ToolCallResult (带执行耗时)
将结果作为 Tool 消息加入 messages
继续下一轮对话
```

## 6. 工具使用建议（给 AI）

系统提示中包含以下工具使用指南，引导 AI 正确使用工具：

1. **读前先写**：编辑文件前先读取理解上下文
2. **优先 edit_file**：小修改用精确替换，不用全文重写
3. **限定范围**：搜索时从宽到窄，使用 `include` 过滤
4. **检查退出码**：bash 命令失败时分析错误并修复
5. **权限感知**：文件修改和命令执行需要用户确认

## 7. 扩展工具

### 7.1 新增原生工具

在 `src/agent/tools.rs` 中：

1. 添加工具定义函数（如 `fn my_tool() -> Value`）
2. 在 `native_tool_definitions()` 中加入向量
3. 在 `native_tool_names()` 中加入名称
4. 在 `execute_native_tool()` 的 match 中添加分支
5. 如需权限，在 `permission.rs::needs_permission` 中添加判断

### 7.2 MCP 工具

通过 `McpManager` 连接的 MCP 服务器工具自动映射为 OpenAI function 格式，前缀 `mcp_`。详见 [MCP 集成](./05-mcp.md)。