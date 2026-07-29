use rmcp::model::Tool as McpTool;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::info;

/// Configuration for a single MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: HashMap<String, McpServerConfig>,
}

impl McpConfig {
    /// Parse McpConfig from the `[mcp]` section of a unified TOML config.
    pub fn from_toml_str(toml_str: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Toplevel {
            #[serde(default)]
            mcp: Option<McpConfig>,
        }
        let toplevel: Toplevel =
            toml::from_str(toml_str).map_err(|e| format!("Failed to parse TOML: {}", e))?;
        Ok(toplevel.mcp.unwrap_or_default())
    }

    pub fn from_file(path: &str) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read MCP config {}: {}", path, e))?;
        let config: McpConfig = serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse MCP config: {}", e))?;
        Ok(config)
    }

    pub fn server_config(&self, name: &str) -> Option<&McpServerConfig> {
        self.servers.get(name).filter(|cfg| cfg.enabled)
    }
}

pub struct McpManager {
    connections: HashMap<String, McpConnection>,
    config: McpConfig,
}

struct McpConnection {
    tools: Vec<McpTool>,
    child: Arc<Mutex<tokio::process::Child>>,
}

impl McpManager {
    pub fn new() -> Self {
        McpManager {
            connections: HashMap::new(),
            config: McpConfig::default(),
        }
    }

    /// Load MCP config from the project's or global unified config file.
    /// Tries `.mp_agent/config.toml` → global config → `mcp_servers.json`.
    pub fn from_project() -> Self {
        // Try project-level config
        let project_path = Path::new(".mp_agent/config.toml");
        if let Some(manager) = Self::try_load_toml(project_path) {
            return manager;
        }
        // Try global user-level config
        if let Some(global_dir) = crate::config::global_config_dir() {
            let global_path = global_dir.join("config.toml");
            if let Some(manager) = Self::try_load_toml(&global_path) {
                return manager;
            }
        }
        // Fall back to legacy JSON
        if Path::new("mcp_servers.json").exists()
            && let Ok(manager) = McpManager::from_config("mcp_servers.json")
        {
            return manager;
        }
        McpManager::new()
    }

    /// Try to load MCP config from a TOML file at the given path.
    fn try_load_toml(path: &Path) -> Option<Self> {
        if path.exists()
            && let Ok(contents) = std::fs::read_to_string(path)
            && let Ok(config) = McpConfig::from_toml_str(&contents)
        {
            Some(McpManager {
                connections: HashMap::new(),
                config,
            })
        } else {
            None
        }
    }

    pub fn from_config(path: &str) -> Result<Self, String> {
        let config = McpConfig::from_file(path)?;
        Ok(McpManager {
            connections: HashMap::new(),
            config,
        })
    }

    pub fn config_has_servers(&self) -> bool {
        !self.config.servers.is_empty()
    }

    pub async fn connect_servers(&mut self) -> Vec<(String, Result<Vec<McpTool>, String>)> {
        let mut results = Vec::new();
        let server_names: Vec<String> = self.config.servers.keys().cloned().collect();
        for name in server_names {
            if let Some(cfg) = self.config.server_config(&name) {
                let result = self
                    .connect(name.clone(), cfg.command.clone(), cfg.args.clone())
                    .await;
                results.push((name, result));
            }
        }
        results
    }

    pub async fn connect(
        &mut self,
        name: String,
        command: String,
        args: Vec<String>,
    ) -> Result<Vec<McpTool>, String> {
        let mut child = Command::new(&command)
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn MCP server '{}': {}", command, e))?;

        // Perform MCP handshake and list tools
        let tools = Self::perform_handshake(&mut child).await?;

        info!(
            "Connected to MCP server '{}', found {} tools",
            name,
            tools.len()
        );

        let conn = McpConnection {
            tools: tools.clone(),
            child: Arc::new(Mutex::new(child)),
        };

        self.connections.insert(name.clone(), conn);
        Ok(tools)
    }

    /// Read lines from stdout until we find one that parses as valid JSON.
    /// Skips non-JSON lines (e.g. stray log output written to stdout by the server).
    async fn read_json_line(
        reader: &mut BufReader<&mut tokio::process::ChildStdout>,
    ) -> Result<String, String> {
        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| format!("Read error: {}", e))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if serde_json::from_str::<Value>(trimmed).is_ok() {
                return Ok(line);
            }
            // skip non-JSON line (likely log output)
            tracing::warn!("skipping non-JSON line from MCP server: {:?}", trimmed);
        }
    }

    /// Send a JSON-RPC request over the MCP child's stdin and parse the
    /// corresponding JSON response from stdout.
    async fn json_rpc_call(
        child: &mut tokio::process::Child,
        request: &Value,
    ) -> Result<Value, String> {
        let line = format!(
            "{}\n",
            serde_json::to_string(request).map_err(|e| e.to_string())?
        );
        {
            let stdin = child.stdin.as_mut().ok_or("No stdin")?;
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|e| format!("Write error: {}", e))?;
        }

        let stdout = child.stdout.as_mut().ok_or("No stdout")?;
        let mut reader = BufReader::new(stdout);
        let response_line = Self::read_json_line(&mut reader).await?;
        serde_json::from_str(&response_line).map_err(|e| format!("Parse error: {}", e))
    }

    /// Extract the "result" field from an MCP JSON-RPC response, returning
    /// an error if the response contains an "error" field.
    fn extract_result(response: &Value) -> Result<&Value, String> {
        if let Some(err) = response.get("error") {
            return Err(format!(
                "MCP error: {}",
                err["message"].as_str().unwrap_or("unknown")
            ));
        }
        response
            .get("result")
            .ok_or_else(|| "No result field in MCP response".to_string())
    }

    async fn perform_handshake(child: &mut tokio::process::Child) -> Result<Vec<McpTool>, String> {
        // Step 1: Initialize
        let init_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "mp_agent", "version": "0.1.0" }
            }
        });

        let init_response = Self::json_rpc_call(child, &init_request).await?;
        let result = Self::extract_result(&init_response)?;
        if result.is_null() {
            return Err("Initialize result is null".into());
        }

        // Step 2: Send initialized notification
        let notif = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        {
            let stdin = child.stdin.as_mut().ok_or("No stdin")?;
            let line = format!(
                "{}\n",
                serde_json::to_string(&notif).map_err(|e| e.to_string())?
            );
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|e| format!("Write error: {}", e))?;
        }

        // Step 3: List tools
        let list_request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        });

        let list_response = Self::json_rpc_call(child, &list_request).await?;
        let list_result = Self::extract_result(&list_response)?;
        let tools_val = list_result["tools"].clone();
        let mcp_tools: Vec<McpTool> =
            serde_json::from_value(tools_val).map_err(|e| format!("Tool parse error: {}", e))?;

        Ok(mcp_tools)
    }

    pub fn get_openai_tools(&self) -> Vec<(String, String, Value)> {
        let mut result = Vec::new();
        for (server_name, conn) in &self.connections {
            for tool in &conn.tools {
                let prefixed_name = format!("{}_{}", server_name, tool.name);
                let input_schema = mcp_input_schema_to_openai(&tool.input_schema);
                let def = json!({
                    "type": "function",
                    "function": {
                        "name": prefixed_name,
                        "description": tool.description.as_deref().unwrap_or("MCP tool"),
                        "parameters": input_schema
                    }
                });
                result.push((server_name.clone(), tool.name.to_string(), def));
            }
        }
        result
    }

    pub fn has_prefixed_tool(&self, prefixed_name: &str) -> bool {
        for (conn_name, conn) in &self.connections {
            for tool in &conn.tools {
                let full_name = format!("{}_{}", conn_name, tool.name.as_ref());
                if full_name == prefixed_name {
                    return true;
                }
            }
        }
        false
    }

    pub async fn call_prefixed_tool(
        &self,
        prefixed_name: &str,
        args: Value,
    ) -> Result<String, String> {
        for (conn_name, conn) in &self.connections {
            for tool in &conn.tools {
                let full_name = format!("{}_{}", conn_name, tool.name);
                if full_name == prefixed_name {
                    return self.call_tool_raw(&conn.child, &tool.name, args).await;
                }
            }
        }
        Err(format!("MCP tool '{}' not found", prefixed_name))
    }

    async fn call_tool_raw(
        &self,
        child_lock: &Arc<Mutex<tokio::process::Child>>,
        tool_name: &str,
        args: Value,
    ) -> Result<String, String> {
        let mut child = child_lock.lock().await;

        let args_map = match &args {
            Value::Object(map) => Some(map.clone()),
            _ => None,
        };

        let call_request = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": args_map
            }
        });

        let response = Self::json_rpc_call(&mut child, &call_request).await?;
        let result = Self::extract_result(&response)?;

        // Extract text content from result
        let content = &result["content"];
        let mut output = String::new();
        if let Some(items) = content.as_array() {
            for item in items {
                if item["type"] == "text"
                    && let Some(text) = item["text"].as_str()
                {
                    output.push_str(text);
                }
            }
        }

        Ok(output)
    }
}

pub fn mcp_input_schema_to_openai(schema: &Arc<Map<String, Value>>) -> Value {
    let map: &Map<String, Value> = schema.as_ref();
    serde_json::to_value(map).unwrap_or_else(|_| json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_config_from_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("mcp.json");
        std::fs::write(
            &path,
            r#"{
                "servers": {
                    "test": {
                        "command": "echo",
                        "args": ["hello"],
                        "enabled": true
                    }
                }
            }"#,
        )
        .unwrap();

        let config = McpConfig::from_file(&path.to_string_lossy()).unwrap();
        assert_eq!(config.servers.len(), 1);
        assert!(config.servers.contains_key("test"));
        let srv = config.servers.get("test").unwrap();
        assert_eq!(srv.command, "echo");
        assert_eq!(srv.args, vec!["hello"]);
        assert!(srv.enabled);
    }

    #[test]
    fn test_mcp_config_default_disabled() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("mcp.json");
        std::fs::write(
            &path,
            r#"{
                "servers": {
                    "test": {
                        "command": "echo"
                    }
                }
            }"#,
        )
        .unwrap();

        let config = McpConfig::from_file(&path.to_string_lossy()).unwrap();
        let srv = config.servers.get("test").unwrap();
        assert!(srv.enabled);
    }

    #[test]
    fn test_mcp_config_server_config_enabled() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("mcp.json");
        std::fs::write(
            &path,
            r#"{
                "servers": {
                    "on": { "command": "echo", "enabled": true },
                    "off": { "command": "echo", "enabled": false }
                }
            }"#,
        )
        .unwrap();

        let config = McpConfig::from_file(&path.to_string_lossy()).unwrap();
        assert!(config.server_config("on").is_some());
        assert!(config.server_config("off").is_none());
    }

    #[tokio::test]
    async fn test_mcp_get_openai_tools() {
        use rmcp::model::Tool;
        use std::sync::Arc;
        let tool = Tool::new("test", "test tool", Arc::new(serde_json::Map::new()));
        let child = Arc::new(Mutex::new(
            tokio::process::Command::new("echo").spawn().unwrap(),
        ));
        let conn = McpConnection {
            tools: vec![tool],
            child,
        };
        let mut connections = HashMap::new();
        connections.insert("test_server".into(), conn);
        let manager = McpManager {
            connections,
            config: McpConfig {
                servers: HashMap::new(),
            },
        };
        let tools = manager.get_openai_tools();
        assert_eq!(tools.len(), 1);
        let (_, _, def) = &tools[0];
        let func = &def["function"];
        assert_eq!(func["name"].as_str().unwrap(), "test_server_test");
    }
}
