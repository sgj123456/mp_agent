//! A minimal test MCP server for mp_agent.
//!
//! Run with: `cargo run --bin test_mcp_server`
//!
//! This server exposes a handful of simple tools via the MCP protocol using
//! the `rmcp` crate. It is intended to be spawned as a child process by
//! `mp_agent`'s `McpManager`, proving that the MCP integration path works
//! end-to-end.
//!
//! To avoid schemars version conflicts between rmcp's re-exported schemars
//! and the workspace schemars dependency, we build input schemas manually as
//! JSON objects rather than deriving them with schemars.

use rmcp::model::ErrorData;
use rmcp::model::{
    CallToolResult, ClientRequest, Content, ErrorCode, ListToolsResult, ServerResult, Tool,
};
use rmcp::service::{RoleServer, Service, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;

// ============================================================================
// Parameter types
// ============================================================================

#[derive(Serialize, Deserialize, Debug)]
pub struct EchoRequest {
    /// The message to echo back
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AddRequest {
    pub a: i64,
    pub b: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GreetRequest {
    pub name: String,
}

// ============================================================================
// Server implementation
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct TestMcpServer;

impl TestMcpServer {
    fn echo(&self, request: EchoRequest) -> CallToolResult {
        CallToolResult::success(vec![Content::text(format!("echo: {}", request.message))])
    }

    fn add(&self, request: AddRequest) -> CallToolResult {
        CallToolResult::success(vec![Content::text(format!("{}", request.a + request.b))])
    }

    fn greet(&self, request: GreetRequest) -> CallToolResult {
        CallToolResult::success(vec![Content::text(format!("Hello, {}!", request.name))])
    }

    fn echo_schema() -> Arc<Map<String, Value>> {
        Arc::new(schema_for_type::<EchoRequest>())
    }

    fn add_schema() -> Arc<Map<String, Value>> {
        Arc::new(schema_for_type::<AddRequest>())
    }

    fn greet_schema() -> Arc<Map<String, Value>> {
        Arc::new(schema_for_type::<GreetRequest>())
    }

    fn list_tools(&self) -> Vec<Tool> {
        vec![
            Tool {
                name: "echo".into(),
                title: Some("Echo".into()),
                description: Some("Echo back a message".into()),
                input_schema: Self::echo_schema(),
                output_schema: None,
                annotations: None,
                icons: None,
            },
            Tool {
                name: "add".into(),
                title: Some("Add".into()),
                description: Some("Add two numbers".into()),
                input_schema: Self::add_schema(),
                output_schema: None,
                annotations: None,
                icons: None,
            },
            Tool {
                name: "greet".into(),
                title: Some("Greet".into()),
                description: Some("Greet someone".into()),
                input_schema: Self::greet_schema(),
                output_schema: None,
                annotations: None,
                icons: None,
            },
        ]
    }
}

/// Build a minimal JSON schema for a struct with string/i64 fields.
/// This is deliberately simple to avoid the schemars version conflict.
fn schema_for_type<T: Serialize + DefaultSchema>() -> Map<String, Value> {
    let json = serde_json::to_value(T::default_schema()).unwrap();
    serde_json::from_value(json).unwrap_or_else(|_| {
        let mut map = Map::new();
        map.insert("type".into(), Value::String("object".into()));
        map.insert("properties".into(), Value::Object(Map::new()));
        map
    })
}

trait DefaultSchema {
    fn default_schema() -> Value;
}

impl DefaultSchema for EchoRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The message to echo back"
                }
            },
            "required": ["message"]
        })
    }
}

impl DefaultSchema for AddRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "a": { "type": "integer" },
                "b": { "type": "integer" }
            },
            "required": ["a", "b"]
        })
    }
}

impl DefaultSchema for GreetRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the person to greet"
                }
            },
            "required": ["name"]
        })
    }
}

impl Service<RoleServer> for TestMcpServer {
    fn handle_request(
        &self,
        request: ClientRequest,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ServerResult, ErrorData>> + Send + '_ {
        let self_clone = self.clone();
        async move {
            match request {
                ClientRequest::InitializeRequest(_req) => {
                    let info = self_clone.get_info();
                    Ok(ServerResult::InitializeResult(
                        rmcp::model::InitializeResult {
                            protocol_version: info.protocol_version,
                            capabilities: info.capabilities,
                            server_info: info.server_info,
                            instructions: info.instructions,
                        },
                    ))
                }
                ClientRequest::ListToolsRequest(_) => Ok(ServerResult::ListToolsResult(
                    ListToolsResult::with_all_items(self_clone.list_tools()),
                )),
                ClientRequest::CallToolRequest(req) => {
                    let name: &str = req.params.name.as_ref();
                    let args = req.params.arguments.as_ref();

                    match name {
                        "echo" => {
                            let params: EchoRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid echo params",
                                        None,
                                    ));
                                }
                            };
                            Ok(ServerResult::CallToolResult(self_clone.echo(params)))
                        }
                        "add" => {
                            let params: AddRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid add params",
                                        None,
                                    ));
                                }
                            };
                            Ok(ServerResult::CallToolResult(self_clone.add(params)))
                        }
                        "greet" => {
                            let params: GreetRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid greet params",
                                        None,
                                    ));
                                }
                            };
                            Ok(ServerResult::CallToolResult(self_clone.greet(params)))
                        }
                        unknown => Err(ErrorData::new(
                            ErrorCode::METHOD_NOT_FOUND,
                            format!("unknown tool: {}", unknown),
                            None,
                        )),
                    }
                }
                // Pass through other requests as empty results
                _ => Ok(ServerResult::EmptyResult(rmcp::model::EmptyObject {})),
            }
        }
    }

    async fn handle_notification(
        &self,
        _notification: rmcp::model::ClientNotification,
        _context: rmcp::service::NotificationContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        async move { Ok(()) }.await
    }

    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            protocol_version: rmcp::model::ProtocolVersion::default(),
            capabilities: rmcp::model::ServerCapabilities::default(),
            server_info: rmcp::model::Implementation::from_build_env(),
            instructions: None,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::new("test_mcp_server=info,rmcp=info");
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    // Serve over stdio — must use stderr for logging
    let transport = (tokio::io::stdin(), tokio::io::stdout());

    let server = TestMcpServer;
    let service = server
        .serve(transport)
        .await
        .map_err(|e| anyhow::anyhow!("failed to serve: {}", e))?;

    tracing::info!("test_mcp_server started, serving on stdio");

    // Keep running until the transport closes
    let _ = tokio::signal::ctrl_c().await;
    service.cancel().await.ok();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_tool_metadata() {
        let server = TestMcpServer;
        let tools = server.list_tools();
        assert_eq!(tools.len(), 3);
        assert!(tools.iter().any(|t| t.name.as_ref() == "echo"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "add"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "greet"));
    }

    #[test]
    fn test_mcp_server_tool_calls() {
        let server = TestMcpServer;

        let result = server.echo(EchoRequest {
            message: "hello mcp".into(),
        });
        assert_eq!(result.content[0].as_text().unwrap().text, "echo: hello mcp");

        let result = server.add(AddRequest { a: 2, b: 3 });
        assert_eq!(result.content[0].as_text().unwrap().text, "5");

        let result = server.greet(GreetRequest {
            name: "World".into(),
        });
        assert_eq!(result.content[0].as_text().unwrap().text, "Hello, World!");
    }
}
