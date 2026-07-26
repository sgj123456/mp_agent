# mp_agent — 权限系统

> 描述对敏感操作的权限审批机制，确保 AI 工具调用安全可控。

## 目录

1. [项目概览](./01-overview.md)
2. [架构设计](./02-architecture.md)
3. [工具系统](./03-tools.md)
4. [权限系统](./04-permission.md) ← 你在这里
5. [MCP 集成](./05-mcp.md)
6. [技能系统](./06-skills.md)
7. [UI 组件](./07-ui.md)
8. [配置与运行](./08-config.md)
9. [开发指南](./09-development.md)

---

## 1. 设计目标

mp_agent 让 AI 可以调用工具完成实际工作，但某些操作（写入文件、执行命令）具有潜在风险。权限系统的目标是：

- **安全可控**：敏感操作必须获得用户显式确认
- **减少干扰**：对可信路径支持"记住选择"，避免重复确认
- **信息充分**：审批提示包含操作类型、目标路径、操作描述
- **用户主导**：用户可随时拒绝，AI 需提供替代方案

## 2. 权限操作类型

| 操作 | 触发工具 | 说明 |
|---|---|---|
| `Write` | `write_file`, `edit_file` | 写入或修改文件内容 |
| `Execute` | `bash` | 执行 shell 命令 |

读取文件、搜索、列目录、todo 管理等只读操作**不需要**权限。

## 3. 权限决策

| 决策 | 按键 | 说明 |
|---|---|---|
| Allow（允许） | `y` | 允许本次操作 |
| Always（总是允许） | `a` | 允许本次操作，并保存规则到内存 |
| Deny（拒绝） | `n` | 拒绝本次操作 |
| Deny Always（总是拒绝） | `d` | 拒绝本次操作，并保存规则到内存 |
| 取消 | `Esc` | 拒绝本次操作（不保存规则） |

## 4. 规则匹配

权限规则在内存中维护，结构如下：

```rust
pub struct PermissionRule {
    pub op: PermissionOp,      // Write / Execute
    pub path_prefix: String,   // 路径前缀
    pub decision: PermissionDecision,  // Allow / Deny
}
```

- 规则按添加顺序匹配，首次匹配即返回决策
- 路径匹配使用前缀匹配（`path.starts_with(&rule.path_prefix)`）
- 保存"总是"规则时，使用路径的**目录名**作为前缀（`dirname`），而非完整文件名
- 规则仅在当前会话有效，退出后丢失

### 匹配流程

```
Agent 检测到工具调用需要权限
    ↓
检查内存中已有规则是否匹配 (match_rule)
    ├── 匹配 → 直接执行决策（Allow/Deny），不弹窗
    └── 不匹配 → 发送 PermissionRequired 事件，弹窗等待用户
```

## 5. 权限请求流程

```
AI 响应包含工具调用（如 write_file）
    ↓
Agent::send_message 解析 tool_call
    ↓
needs_permission(tool_name, args) → 返回 (Op, path)
    ↓
request_permission(op, path, tool_name)
    ├── 创建 oneshot 通道
    ├── 发送 AgentEvent::PermissionRequired（含 request + respond sender）
    └── 等待 oneshot 响应（阻塞 Agent 任务）

App::process_agent_events 接收 PermissionRequired
    ├── 检查 permission_rules 是否已有匹配规则
    │   ├── 有 → 直接 respond.send(decision)，Agent 继续
    │   └── 无 → 暂存 pending_permission，UI 渲染提示栏
    ↓
用户按键 (y/n/a/d/Esc)
    ↓
App::handle_key_event 处理决策
    ├── 如果 a/d，添加规则到 permission_rules
    ├── 通过 oneshot::send(decision) 通知 Agent
    ↓
Agent 收到决策
    ├── Allow → 执行工具
    └── Deny → 返回 "Permission denied" 消息
```

## 6. UI 渲染

当有待处理权限请求时，状态栏显示高亮提示：

```
【Permission】WRITE /path/to/file | write_file [y]es [a]lways [n]o [d]eny [Esc]
```

- 黄色背景 + 粗体文字，确保醒目
- 路径被截断显示（最多 60 字符）
- 操作类型标签：`WRITE` / `EXEC`

## 7. 关键代码

### 7.1 判断是否需要权限（`permission.rs`）

```rust
pub fn needs_permission(tool_name: &str, args: &serde_json::Value) -> Option<(PermissionOp, String)> {
    match tool_name {
        "write_file" | "edit_file" => {
            let path = args.get("path").and_then(|v| v.as_str())?;
            Some((PermissionOp::Write, path.to_string()))
        }
        "bash" => {
            let cmd = args.get("command").and_then(|v| v.as_str())?;
            Some((PermissionOp::Execute, format!("bash: {}", cmd)))
        }
        _ => None,
    }
}
```

### 7.2 请求权限（`agent.rs`）

```rust
async fn request_permission(
    &self,
    op: PermissionOp,
    path: &str,
    desc: &str,
) -> PermissionDecision {
    let (tx, rx) = oneshot::channel();
    let _ = self.event_tx.send(AgentEvent::PermissionRequired {
        request: PermissionRequest {
            op,
            path: crate::permission::abspath(path),
            description: desc.to_string(),
        },
        respond: tx,
    });
    rx.await.unwrap_or(PermissionDecision::Deny)
}
```

### 7.3 处理权限输入（`app.rs`）

```rust
if self.pending_permission.is_some() {
    let consume = matches!(key.code,
        KeyCode::Char('y') | KeyCode::Char('Y') |
        KeyCode::Char('n') | KeyCode::Char('N') |
        KeyCode::Char('a') | KeyCode::Char('A') |
        KeyCode::Char('d') | KeyCode::Char('D') |
        KeyCode::Esc
    );
    if consume {
        let pending = self.pending_permission.take().unwrap();
        let (decision, add_rule) = match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => (PermissionDecision::Allow, false),
            KeyCode::Char('a') | KeyCode::Char('A') => (PermissionDecision::Allow, true),
            KeyCode::Char('d') | KeyCode::Char('D') => (PermissionDecision::Deny, true),
            _ => (PermissionDecision::Deny, false),
        };
        if add_rule {
            self.permission_rules.push(PermissionRule {
                op: pending.request.op.clone(),
                path_prefix: dirname(&pending.request.path),
                decision: decision.clone(),
            });
        }
        let _ = pending.respond.send(decision);
        return;
    }
}
```

## 8. 权限系统设计考量

### 8.1 为什么只在写入和执行时请求权限？

- 读取文件、搜索、列目录是只读操作，不会改变系统状态
- todo 管理是 Agent 内部状态，不触及外部系统
- 这遵循最小权限原则，减少确认噪音

### 8.2 为什么规则基于路径前缀匹配？

- 前缀匹配简单高效，适合文件系统路径的层级结构
- 使用 `dirname` 而非完整路径，让同一目录下的操作自动放行
- 未来可扩展为支持 glob 或正则匹配

### 8.3 为什么规则只在内存中持久化？

- 安全考量：避免永久保存可能过期的权限规则
- 每次会话重新评估，降低长期风险
- 简化实现，无需处理规则文件的读写和同步

### 8.4 MCP 工具的权限

当前 MCP 工具调用**不触发**权限检查（由 MCP 服务器自身的安全机制约束）。未来版本可能将 MCP 工具纳入统一权限框架。

## 9. 安全建议

1. **谨慎使用 "always"**：仅对可信项目目录使用自动允许
2. **审查命令**：对 bash 命令始终确认，特别是涉及 `rm`、`git push` 等
3. **使用 edit_file 而非 write_file**：精确替换减少意外覆盖风险
4. **注意路径**：权限规则基于绝对路径，相对路径会被转换