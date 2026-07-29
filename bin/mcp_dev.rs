use rmcp::model::{CallToolResult, ClientRequest, ListToolsResult, ServerResult, Tool};
use rmcp::model::{ContentBlock, ErrorCode, ErrorData};
use rmcp::service::{RoleServer, Service, ServiceExt};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ProjectStatsRequest {
    pub path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LargeFilesRequest {
    pub path: Option<String>,
    pub min_mb: Option<u64>,
    pub max_results: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PortCheckRequest {
    pub port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DiskUsageRequest {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DevMcpServer;

impl DevMcpServer {
    fn project_stats(&self, req: ProjectStatsRequest) -> CallToolResult {
        let root = req.path.as_deref().unwrap_or(".");
        let entries = walk_dir(root).unwrap_or_default();
        let mut stats: Vec<(String, usize, usize)> = Vec::new();
        for path in &entries {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let lang = match ext {
                    "rs" => "Rust",
                    "ts" | "tsx" => "TypeScript",
                    "js" | "jsx" => "JavaScript",
                    "py" => "Python",
                    "go" => "Go",
                    "java" => "Java",
                    "c" | "h" => "C",
                    "cpp" | "hpp" | "cc" => "C++",
                    "toml" => "TOML",
                    "json" => "JSON",
                    "yaml" | "yml" => "YAML",
                    "md" => "Markdown",
                    "css" | "scss" => "CSS",
                    "html" => "HTML",
                    "sh" | "bash" => "Shell",
                    _ => continue,
                };
                let lines = count_lines(path);
                if let Ok((line_count, byte_count)) = lines {
                    if let Some(entry) = stats.iter_mut().find(|(l, _, _)| l == lang) {
                        entry.1 += line_count;
                        entry.2 += byte_count;
                    } else {
                        stats.push((lang.to_string(), line_count, byte_count));
                    }
                }
            }
        }

        stats.sort_by_key(|b| std::cmp::Reverse(b.1));

        let mut output = String::new();
        output.push_str(&format!("Project Stats for: {}\n\n", root));
        output.push_str(&format!(
            "{:<15} {:>12} {:>12}\n",
            "Language", "Lines", "Bytes"
        ));
        output.push_str(&format!("{:-<15} {:-<12} {:-<12}\n", "", "", ""));
        let (mut total_lines, mut total_bytes) = (0, 0);
        for (lang, lines, bytes) in &stats {
            output.push_str(&format!("{:<15} {:>12} {:>12}\n", lang, lines, bytes));
            total_lines += lines;
            total_bytes += bytes;
        }
        output.push_str(&format!("{:-<15} {:-<12} {:-<12}\n", "", "", ""));
        output.push_str(&format!(
            "{:<15} {:>12} {:>12}\n",
            "TOTAL", total_lines, total_bytes
        ));
        CallToolResult::success(vec![ContentBlock::text(output)])
    }

    fn find_large_files(&self, req: LargeFilesRequest) -> CallToolResult {
        let root = req.path.as_deref().unwrap_or(".");
        let min_bytes = req.min_mb.unwrap_or(1) * 1024 * 1024;
        let max_results = req.max_results.unwrap_or(20).min(100);
        let mut large_files: Vec<(String, u64)> = Vec::new();

        let entries = walk_dir(root).unwrap_or_default();
        for path in &entries {
            if let Ok(meta) = std::fs::metadata(path)
                && meta.is_file()
                && meta.len() >= min_bytes
            {
                large_files.push((path.to_string_lossy().to_string(), meta.len()));
            }
        }

        large_files.sort_by_key(|b| std::cmp::Reverse(b.1));
        large_files.truncate(max_results);

        let mut output = String::new();
        output.push_str(&format!(
            "Files larger than {} MB in {}\n\n",
            req.min_mb.unwrap_or(1),
            root
        ));
        if large_files.is_empty() {
            output.push_str("No large files found.");
        } else {
            output.push_str(&format!("{:<8} {}\n", "Size", "Path"));
            output.push_str(&format!("{:-<8} {:-<1}\n", "", ""));
            for (path, size) in &large_files {
                let size_mb = *size as f64 / (1024.0 * 1024.0);
                output.push_str(&format!("{:>7.1}M {}\n", size_mb, path));
            }
        }

        CallToolResult::success(vec![ContentBlock::text(output)])
    }

    fn port_check(&self, req: PortCheckRequest) -> CallToolResult {
        let in_use = std::net::TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", req.port).parse().unwrap(),
            std::time::Duration::from_millis(500),
        )
        .is_ok();

        let output = if in_use {
            format!("Port {} is IN USE", req.port)
        } else {
            format!("Port {} is AVAILABLE", req.port)
        };
        CallToolResult::success(vec![ContentBlock::text(output)])
    }

    fn disk_usage(&self, req: DiskUsageRequest) -> CallToolResult {
        let path = req.path.as_deref().unwrap_or(".");
        let output = std::process::Command::new("du")
            .args(["-sh", path])
            .output();
        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout).to_string();
                CallToolResult::success(vec![ContentBlock::text(text)])
            }
            Err(e) => CallToolResult::error(vec![ContentBlock::text(format!("du error: {}", e))]),
        }
    }

    fn list_tools(&self) -> Vec<Tool> {
        vec![
            Tool::new(
                "project_stats",
                "Count lines of code and bytes by language in a directory tree. Ignores hidden dirs and common build artifacts.",
                Arc::new(schema_for_type::<ProjectStatsRequest>()),
            )
            .with_title("Project Stats"),
            Tool::new(
                "find_large_files",
                "Find files larger than a threshold (default 1 MB) in a directory tree.",
                Arc::new(schema_for_type::<LargeFilesRequest>()),
            )
            .with_title("Find Large Files"),
            Tool::new(
                "port_check",
                "Check if a TCP port is in use on localhost.",
                Arc::new(schema_for_type::<PortCheckRequest>()),
            )
            .with_title("Port Check"),
            Tool::new(
                "disk_usage",
                "Show disk usage summary for a path (runs `du -sh`).",
                Arc::new(schema_for_type::<DiskUsageRequest>()),
            )
            .with_title("Disk Usage"),
        ]
    }
}

fn walk_dir(path: &str) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    let ignore_dirs = [
        ".git",
        "node_modules",
        "target",
        ".hg",
        ".svn",
        "__pycache__",
        ".venv",
        "dist",
        "build",
    ];
    let mut stack = vec![std::path::PathBuf::from(path)];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !ignore_dirs.contains(&name) && !name.starts_with('.') {
                        stack.push(path);
                    }
                } else {
                    files.push(path);
                }
            }
        }
    }
    Ok(files)
}

fn count_lines(path: &std::path::Path) -> std::io::Result<(usize, usize)> {
    let content = std::fs::read(path)?;
    let line_count = content.iter().filter(|&&b| b == b'\n').count() + 1;
    let byte_count = content.len();
    Ok((line_count, byte_count))
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

impl DefaultSchema for ProjectStatsRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory to analyze (default: current directory)" }
            }
        })
    }
}

impl DefaultSchema for LargeFilesRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory to search (default: current directory)" },
                "min_mb": { "type": "integer", "description": "Minimum file size in MB (default: 1)" },
                "max_results": { "type": "integer", "description": "Max files to return (default: 20, max: 100)" }
            }
        })
    }
}

impl DefaultSchema for PortCheckRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "port": { "type": "integer", "description": "TCP port number to check" }
            },
            "required": ["port"]
        })
    }
}

impl DefaultSchema for DiskUsageRequest {
    fn default_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to check (default: current directory)" }
            }
        })
    }
}

impl Service<RoleServer> for DevMcpServer {
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
                        "project_stats" => {
                            let params: ProjectStatsRequest = args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                                .unwrap_or_default();
                            self_clone.project_stats(params)
                        }
                        "find_large_files" => {
                            let params: LargeFilesRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid find_large_files params",
                                        None,
                                    ));
                                }
                            };
                            self_clone.find_large_files(params)
                        }
                        "port_check" => {
                            let params: PortCheckRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid port_check params",
                                        None,
                                    ));
                                }
                            };
                            self_clone.port_check(params)
                        }
                        "disk_usage" => {
                            let params: DiskUsageRequest = match args
                                .and_then(|v| serde_json::from_value(Value::Object(v.clone())).ok())
                            {
                                Some(p) => p,
                                None => {
                                    return Err(ErrorData::new(
                                        ErrorCode::INVALID_PARAMS,
                                        "invalid disk_usage params",
                                        None,
                                    ));
                                }
                            };
                            self_clone.disk_usage(params)
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
    let filter = tracing_subscriber::EnvFilter::new("mcp_dev=info");
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    let transport = (tokio::io::stdin(), tokio::io::stdout());
    let server = DevMcpServer;
    let service = server
        .serve(transport)
        .await
        .map_err(|e| anyhow::anyhow!("failed to serve: {}", e))?;

    tracing::info!("mcp-dev started, serving on stdio");
    let _ = tokio::signal::ctrl_c().await;
    service.cancel().await.ok();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dev_mcp_server_tool_metadata() {
        let server = DevMcpServer;
        let tools = server.list_tools();
        assert_eq!(tools.len(), 4);
        assert!(tools.iter().any(|t| t.name.as_ref() == "project_stats"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "find_large_files"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "port_check"));
        assert!(tools.iter().any(|t| t.name.as_ref() == "disk_usage"));
    }

    #[test]
    fn test_port_check_available() {
        let srv = DevMcpServer;
        let result = srv.port_check(PortCheckRequest { port: 65530 });
        assert!(
            result.content[0]
                .as_text()
                .unwrap()
                .text
                .contains("AVAILABLE")
        );
    }

    #[test]
    fn test_project_stats_empty_dir() {
        let srv = DevMcpServer;
        let dir = tempfile::TempDir::new().unwrap();
        let result = srv.project_stats(ProjectStatsRequest {
            path: Some(dir.path().to_string_lossy().to_string()),
        });
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("Project Stats"));
    }

    #[test]
    fn test_find_large_files_no_results() {
        let srv = DevMcpServer;
        let dir = tempfile::TempDir::new().unwrap();
        let result = srv.find_large_files(LargeFilesRequest {
            path: Some(dir.path().to_string_lossy().to_string()),
            min_mb: Some(1),
            max_results: Some(10),
        });
        let text = result.content[0].as_text().unwrap().text.clone();
        assert!(text.contains("No large files found"));
    }
}
