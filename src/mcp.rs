use rmcp::model::{CallToolRequestParam, Tool as McpTool};
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use rmcp::transport::TokioChildProcess;
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio::process::Command;
use tracing::{info, warn};

pub struct McpManager {
    connections: HashMap<String, McpConnection>,
}

struct McpConnection {
    service: RunningService<RoleClient, ()>,
    server_name: String,
    tools: Vec<McpTool>,
}

impl McpManager {
    pub fn new() -> Self {
        McpManager {
            connections: HashMap::new(),
        }
    }

    pub async fn connect(
        &mut self,
        name: String,
        command: String,
        args: Vec<String>,
    ) -> Result<Vec<McpTool>, String> {
        let mut cmd = Command::new(&command);
        cmd.args(&args);
        let transport = TokioChildProcess::new(cmd)
            .map_err(|e| format!("Failed to spawn MCP server '{}': {}", command, e))?;

        let service = ()
            .serve(transport)
            .await
            .map_err(|e| format!("Failed to start MCP service: {}", e))?;

        let tools = match service.list_tools(None).await {
            Ok(response) => response.tools,
            Err(e) => {
                let _ = service.cancel().await;
                return Err(format!("Failed to list tools: {}", e));
            }
        };

        info!(
            "Connected to MCP server '{}', found {} tools",
            name,
            tools.len()
        );

        let conn = McpConnection {
            service,
            server_name: name.clone(),
            tools: tools.clone(),
        };

        self.connections.insert(name, conn);
        Ok(tools)
    }

    pub fn all_mcp_tools(&self) -> Vec<&McpTool> {
        self.connections
            .values()
            .flat_map(|c| c.tools.iter())
            .collect()
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<String, String> {
        tracing::info!("MCP call: {}", name);
        for conn in self.connections.values() {
            if conn.tools.iter().any(|t| t.name.as_ref() == name) {
                let params = CallToolRequestParam {
                    name: name.to_string().into(),
                    arguments: serde_json::from_value(args).ok(),
                };

                let result = conn
                    .service
                    .call_tool(params)
                    .await
                    .map_err(|e| format!("MCP tool call failed: {}", e))?;

                let mut output = String::new();
                for item in &result.content {
                    if let Some(text) = item.as_text() {
                        output.push_str(&text.text);
                    }
                }
                return Ok(output);
            }
        }

        Err(format!("MCP tool '{}' not found", name))
    }

    pub async fn disconnect_all(&mut self) {
        for (name, conn) in self.connections.drain() {
            if let Err(e) = conn.service.cancel().await {
                warn!("Error disconnecting from '{}': {}", name, e);
            }
        }
    }
}

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
