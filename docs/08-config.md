# mp_agent — 配置与运行

> 描述如何配置 API 密钥、环境变量，以及构建、运行和调试项目。

## 目录

1. [项目概览](./01-overview.md)
2. [架构设计](./02-architecture.md)
3. [工具系统](./03-tools.md)
4. [权限系统](./04-permission.md)
5. [MCP 集成](./05-mcp.md)
6. [技能系统](./06-skills.md)
7. [UI 组件](./07-ui.md)
8. [配置与运行](./08-config.md) ← 你在这里
9. [开发指南](./09-development.md)

---

## 1. 环境要求

| 组件 | 版本 | 说明 |
|---|---|---|
| Rust | 1.76+ | 使用 edition 2024，需要较新的工具链 |
| Cargo | 随 Rust 附带 | 包构建与依赖管理 |
| 终端 | 支持 UTF-8 | 推荐使用现代终端（iTerm2、kitty、alacritty、Windows Terminal 等） |
| API | OpenAI 兼容 endpoint | OpenAI API 或其他兼容 SSE 流的 endpoint |

## 2. 配置文件

mp_agent 使用 **TOML 格式**的统一配置文件。所有配置（API 密钥、模型参数、MCP 服务器）集中在一个文件中。

### 2.1 查找顺序

首次启动时，按以下优先级查找配置文件。如所有位置都不存在，则在全局目录自动生成默认配置。

| 优先级 | 路径 | 说明 |
|--------|------|------|
| 1 | `./.mp_agent/config.toml` | 项目级配置（覆盖全局） |
| 2 | `~/.config/mp_agent/config.toml` (Linux) / `%APPDATA%\mp_agent\config.toml` (Windows) | 全局用户级配置 |

### 2.2 文件格式

```toml
[api]
api_key = "sk-your-api-key"
base_url = "https://api.openai.com/v1"
model = "gpt-4o"
max_tokens = 5000

[mcp.servers]
[mcp.servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
enabled = true
```

### 2.3 配置项说明

#### `[api]` 节

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `api_key` | ✅ 必须 | 无 | API 认证密钥 |
| `base_url` | ❌ 可选 | `https://api.openai.com/v1` | API 基础 URL（尾部斜杠自动去除） |
| `model` | ❌ 可选 | `gpt-4o` | 模型名称 |
| `max_tokens` | ❌ 可选 | 无 | 最大 completion token 数（整数） |

#### `[mcp.servers]` 节

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `command` | ✅ 必须 | 无 | MCP 服务器启动命令 |
| `args` | ❌ 可选 | `[]` | 命令行参数 |
| `enabled` | ❌ 可选 | `true` | 是否启用该服务器 |
| `env` | ❌ 可选 | `{}` | 额外环境变量 |

### 2.4 使用兼容 API

如果使用非 OpenAI 的兼容 API（如 Groq、Together AI、内部 endpoint），只需修改 `base_url` 和 `model`：

```toml
[api]
api_key = "sk-your-key"
base_url = "https://chat.intern-ai.org.cn/api/v1/"
model = "intern-latest"
```

**注意**：API 必须支持 OpenAI 兼容的 SSE 流式响应（`/chat/completions` endpoint + `stream=true`）。

### 2.5 旧版兼容

旧版 `.env` 文件（`OPENAI_API_KEY` / `OPENAI_BASE_URL` / `OPENAI_MODEL` / `OPENAI_MAX_TOKENS`）和 `mcp_servers.json` 仍被支持作为降级方案，当 `.mp_agent/config.toml` 和全局配置都不存在时自动使用。

## 3. 构建项目

### 3.1 调试构建

```bash
cargo build
```

输出在 `target/debug/mp_agent`，包含调试符号，适合开发和调试。

### 3.2 发布构建

```bash
cargo build --release
```

输出在 `target/release/mp_agent`，优化编译，体积更小、运行更快。

### 3.3 检查代码

```bash
# 检查编译不通过
cargo check

# 运行测试（当前无测试，但框架已就绪）
cargo test

# 格式化
cargo fmt

# 静态分析
cargo clippy
```

## 4. 运行

### 4.1 直接运行

```bash
cargo run
```

这会先构建（如有变更）然后启动程序。

### 4.2 运行发布版本

```bash
./target/release/mp_agent
```

### 4.3 日志输出

mp_agent 运行时会将 tracing 日志写入 `mp_agent.log` 文件（追加模式）：

```bash
# 实时查看日志
tail -f mp_agent.log

# 过滤日志级别
RUST_LOG=debug cargo run
```

可用的日志级别：`trace`、`debug`、`info`、`warn`、`error`。

默认日志过滤器为 `mp_agent=info`，可通过 `RUST_LOG` 环境变量覆盖。

## 5. 配置 MCP 服务器

MCP 服务器通过在 `.mp_agent/config.toml` 或全局配置的 `[mcp.servers]` 节注册。

### 5.1 示例

```toml
[mcp.servers]
[mcp.servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
enabled = true

[mcp.servers.git]
command = "uvx"
args = ["mcp-server-git"]
enabled = true

[mcp.servers.db]
command = "npx"
args = ["-y", "mcp-server-postgres", "postgresql://user:pass@localhost/db"]
enabled = false
env = { PGPASSWORD = "secret" }
```

### 5.2 加载顺序

1. 项目级配置 `.mp_agent/config.toml`
2. 全局配置 `~/.config/mp_agent/config.toml`（Linux）或 `%APPDATA%/mp_agent/config.toml`（Windows）
3. 旧版 `mcp_servers.json`（降级兼容）

**注意**：MCP 服务器需要预先安装在系统中（如 `npx` 可访问对应包）。

## 6. 配置技能

参见 [技能系统](./06-skills.md)。简要步骤：

1. 创建 `.mp_agent/skills/` 目录（项目级）或 `~/.config/mp_agent/skills/`（全局级）
2. 添加 `.skill` / `.md` / `.txt` 文件
3. 第一行名称，第二行描述，后续正文
4. 重启 Agent 生效

## 7. 常见问题

### 7.1 API 密钥错误

```
API error: HTTP 401 - {"error": {"message": "Invalid API key", ...}}
```

**解决**：检查 `.mp_agent/config.toml` 中的 `api_key` 是否正确，是否有前导/后导空格。

### 7.2 连接超时

```
API error: request timeout
```

**解决**：检查网络连接，或修改 `OPENAI_BASE_URL` 为可用的 endpoint。

### 7.3 模型不支持流式

```
API error: HTTP 400 - "stream" is not supported for this model
```

**解决**：更换支持流式的模型，或在 `config.rs` 中关闭 `stream(true)`（需修改代码）。

### 7.4 终端显示异常

- 中文乱码：确保终端支持 UTF-8
- 布局错乱：确保终端宽度至少 80 列
- 颜色不显示：确保终端支持 256 色或真彩色

### 7.5 权限请求卡住

如果权限请求界面出现但按键无响应，确认：

- 没有处于 CapsLock 状态（y/n/a/d 区分大小写，但大小写都接受）
- 焦点在终端内

### 7.6 日志文件过大

`mp_agent.log` 是追加模式，长期运行可能变大：

```bash
# 清理日志
> mp_agent.log

# 或限制日志大小（未来版本支持 log rotate）
```

## 8. 已实现的配置文件功能

`.mp_agent/config.toml` 当前已支持：

- ✅ MCP 服务器注册（`[mcp.servers]`）
- ✅ 默认模型和参数（`[api]`）
- ✅ 全局自动生成默认配置
- ✅ 项目级配置覆盖全局配置

未来计划增加：

- 权限规则持久化
- 主题和 UI 自定义
- 技能目录覆盖

## 9. 调试技巧

### 9.1 开启详细日志

```bash
RUST_LOG=mp_agent=debug cargo run
```

### 9.2 查看 API 请求/响应

在 `agent.rs` 中可以通过 `tracing::info!` 记录请求体和响应体（当前已记录 token 流解析信息）。

### 9.3 单步调试

使用 `gdb` 或 `lldb`：

```bash
cargo build
lldb target/debug/mp_agent
(lldb) run
```

### 9.4 网络抓包

如果 API 通信有问题：

```bash
# macOS
sudo tcpdump -i lo0 -A port 443 | grep openai

# Linux
sudo tcpdump -i any -A port 443 | grep openai
```

**注意**：API 密钥不应出现在日志或抓包中（通过 Authorization header 传输）。