use rmcp::model::{CallToolResult, ClientRequest, ListToolsResult, ServerResult, Tool};
use rmcp::model::{ContentBlock, ErrorCode, ErrorData};
use rmcp::service::{RoleServer, Service, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct StatusRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub struct DiffRequest {
    pub staged: Option<bool>,
    pub path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LogRequest {
    pub count: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BranchRequest {
    pub action: Option<String>,
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommitRequest {
    pub message: String,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GitMcpServer;

impl GitMcpServer {
    fn run_git(&self, args: &[&str]) -> CallToolResult {
        let output = std::process::Command::new("git").args(args).output();
        match output {
            Ok(out) => {
                let mut text = String::new();
                if !out.stdout.is_empty() {
                    text.push_str(&String::from_utf8_lossy(&out.stdout));
                }
                if !out.stderr.is_empty() {
                    text.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                if !out.status.success() {
                    text = format!(
                        "git {} failed (exit {}):\n{}",
                        args.join(" "),
                        out.status.code().unwrap_or(-1),
                        text
                    );
                }
                CallToolResult::success(vec![ContentBlock::text(text)])
            }
            Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("git error: {}", e))]),
        }
    }

    fn status(&self, _req: StatusRequest) -> CallToolResult {
        self.run_git(&["status", "--short", "--branch"])
    }

    fn diff(&self, req: DiffRequest) -> CallToolResult {
        let mut args = vec!["diff", "--no-color"];
        if req.staged.unwrap_or(false) {
            args.push("--cached");
        }
        if let Some(p) = &req.path {
            args.push("--");
            args.push(p);
        }
        self.run_git(&args)
    }

    fn log(&self, req: LogRequest) -> CallToolResult {
        let count = req.count.unwrap_or(10).min(100);
        self.run_git(&[
            "log",
            &format!("-{}", count),
            "--oneline",
            "--graph",
            "--decorate",
        ])
    }

    fn branch(&self, req: BranchRequest) -> CallToolResult {
        match req.action.as_deref() {
            Some("create") | Some("new") => {
                if let Some(name) = &req.name {
                    self.run_git(&["branch", name])
                } else {
                    CallToolResult::error(vec![ContentBlock::text(
                        "branch name required for create action",
                    )])
                }
            }
            Some("delete") | Some("del") => {
                if let Some(name) = &req.name {
                    self.run_git(&["branch", "-d", name])
                } else {
                    CallToolResult::error(vec![ContentBlock::text(
                        "branch name required for delete action",
                    )])
                }
            }
            _ => self.run_git(&["branch"]),
        }
    }

    fn commit(&self, req: CommitRequest) -> CallToolResult {
        let msg = if let Some(body) = &req.body {
            format!("{}\n\n{}", req.message, body)
        } else {
            req.message.clone()
        };
        let output = std::process::Command::new("git")
            .args(["commit", "-m", &msg])
            .output();
        match output {
            Ok(out) => {
                let text = if !out.stdout.is_empty() {
                    String::from_utf8_lossy(&out.stdout).to_string()
                } else if !out.stderr.is_empty() {
                    String::from_utf8_lossy(&out.stderr).to_string()
                } else {
                    "Commit created.".to_string()
                };
                CallToolResult::success(vec![ContentBlock::text(text)])
            }
            Err(e) => {
                CallToolResult::error(vec![ContentBlock::text(format!("git commit error: {}", e))])
            }
        }
    }

    fn list_tools(&self) -> Vec<Tool> {
        vec![
            Tool::new(
                "git_status",
                "Show working tree status (short format with branch info)",
                Arc::new(schema_for_type::<StatusRequest>()),
            )
            .with_title("Git Status"),
            Tool::new(
                "git_diff",
                "Show staged or unstaged diff. Use staged=true to show staged changes.",
                Arc::new(schema_for_type::<DiffRequest>()),
            )
            .with_title("Git Diff"),
            Tool::new(
                "git_log",
                "Show commit log with oneline format, graph, and decorations. Default 10 entries, max 100.",
                Arc::new(schema_for_type::<LogRequest>()),
            )
            .with_title("Git Log"),
            Tool::new(
                "git_branch",
                "List, create, or delete branches. action: 'list' (default) | 'create' | 'delete'",
                Arc::new(schema_for_type::<BranchRequest>()),
            )
            .with_title("Git Branch"),
            Tool::new(
                "git_commit",
                "Create a commit with the given message and optional body.",
                Arc::new(schema_for_type::<CommitRequest>()),
            )
            .with_title("Git Commit"),
        ]
    }
}

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

impl DefaultSchema for StatusRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }
}

impl DefaultSchema for DiffRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "staged": { "type": "boolean", "description": "Show staged changes (--cached)" },
                "path": { "type": "string", "description": "Filter diff to specific path" }
            }
        })
    }
}

impl DefaultSchema for LogRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer", "description": "Number of commits to show (default 10, max 100)" }
            }
        })
    }
}

impl DefaultSchema for BranchRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action: 'list' (default), 'create', or 'delete'",
                    "enum": ["list", "create", "delete"]
                },
                "name": { "type": "string", "description": "Branch name (required for create/delete)" }
            }
        })
    }
}

impl DefaultSchema for CommitRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Commit message (first line)" },
                "body": { "type": "string", "description": "Optional commit body" }
            },
            "required": ["message"]
        })
    }
}

impl Service<RoleServer> for GitMcpServer {
    fn handle_request(
        &self,
        request: ClientRequest,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ServerResult, ErrorData>> + Send + '_ {
        let self_clone = self.clone();
        async move {
            match request {
                ClientRequest::InitializeRequest(_req) => {
                    Ok(ServerResult::InitializeResult(self_clone.get_info()))
                }
                ClientRequest::ListToolsRequest(_) => Ok(ServerResult::ListToolsResult(
                    ListToolsResult::with_all_items(self_clone.list_tools()),
                )),
                ClientRequest::CallToolRequest(req) => {
                    let name: &str = req.params.name.as_ref();
                    let args = req.params.arguments.as_ref();
                    let result = match name {
                        "git_status" => {
                            let params: StatusRequest = args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                                .unwrap_or_default();
                            self_clone.status(params)
                        }
                        "git_diff" => {
                            let params: DiffRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid diff params",
                                        None,
                                    ));
                                }
                            };
                            self_clone.diff(params)
                        }
                        "git_log" => {
                            let params: LogRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid log params",
                                        None,
                                    ));
                                }
                            };
                            self_clone.log(params)
                        }
                        "git_branch" => {
                            let params: BranchRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid branch params",
                                        None,
                                    ));
                                }
                            };
                            self_clone.branch(params)
                        }
                        "git_commit" => {
                            let params: CommitRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid commit params",
                                        None,
                                    ));
                                }
                            };
                            self_clone.commit(params)
                        }
                        unknown => {
                            return Err(ErrorData::new(
                                ErrorCode::METHOD_NOT_FOUND,
                                format!("unknown tool: {}", unknown),
                                None,
                            ));
                        }
                    };
                    Ok(ServerResult::CallToolResult(result))
                }
                _ => Ok(ServerResult::EmptyResult(rmcp::model::EmptyObject {})),
            }
        }
    }

    async fn handle_notification(
        &self,
        _notification: rmcp::model::ClientNotification,
        _context: rmcp::service::NotificationContext<RoleServer>,
    ) -> Result<(), ErrorData> {
        Ok(())
    }

    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::InitializeResult::new(rmcp::model::ServerCapabilities::default())
            .with_server_info(rmcp::model::Implementation::from_build_env())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::new("mcp_git=info");
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let server = GitMcpServer;
    let service = server
        .serve(transport)
        .await
        .map_err(|e| anyhow::anyhow!("failed to serve: {}", e))?;

    tracing::info!("mcp-git started, serving on stdio");
    let _ = tokio::signal::ctrl_c().await;
    service.cancel().await.ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_mcp_server_tool_metadata() {
        let server = GitMcpServer;
        let tools = server.list_tools();
        assert_eq!(tools.len(), 5);
        assert!(tools.iter().any(|t| t.name.as_ref() == "git_status"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "git_diff"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "git_log"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "git_branch"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "git_commit"));
    }
}
