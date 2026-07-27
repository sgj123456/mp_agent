use std::path::PathBuf;

use serde_json::{Value, json};

pub fn native_tool_definitions() -> Vec<Value> {
    vec![
        bash_tool(),
        read_file_tool(),
        write_file_tool(),
        edit_file_tool(),
        glob_tool(),
        grep_tool(),
        list_directory_tool(),
        add_todo_tool(),
        update_todo_tool(),
        list_todos_tool(),
        remove_todo_tool(),
        present_choices_tool(),
    ]
}

pub fn native_tool_names() -> Vec<&'static str> {
    vec![
        "bash",
        "read_file",
        "write_file",
        "edit_file",
        "glob",
        "grep",
        "list_directory",
        "add_todo",
        "update_todo",
        "list_todos",
        "remove_todo",
        "present_choices",
    ]
}

fn bash_tool() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "bash",
            "description": "Execute a bash command and return its stdout/stderr output.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The bash command to execute"
                    },
                    "workdir": {
                        "type": "string",
                        "description": "Working directory (optional)"
                    }
                },
                "required": ["command"]
            }
        }
    })
}

fn read_file_tool() -> Value {
    json!({
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
    })
}

fn write_file_tool() -> Value {
    json!({
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
    })
}

fn edit_file_tool() -> Value {
    json!({
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
    })
}

fn glob_tool() -> Value {
    json!({
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
    })
}

fn grep_tool() -> Value {
    json!({
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
    })
}

fn list_directory_tool() -> Value {
    json!({
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
    })
}

fn add_todo_tool() -> Value {
    json!({
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
    })
}

fn update_todo_tool() -> Value {
    json!({
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
    })
}

fn list_todos_tool() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "list_todos",
            "description": "List all current todos with their status and priority.",
            "parameters": {
                "type": "object",
                "properties": {}
            }
        }
    })
}

fn remove_todo_tool() -> Value {
    json!({
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
    })
}

fn present_choices_tool() -> Value {
    json!({
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
    })
}

pub async fn execute_native_tool(name: &str, args: &Value) -> String {
    match name {
        "bash" => execute_bash(args).await,
        "read_file" => execute_read_file(args).await,
        "write_file" => execute_write_file(args).await,
        "edit_file" => execute_edit_file(args).await,
        "glob" => execute_glob(args).await,
        "grep" => execute_grep(args).await,
        "list_directory" => execute_list_directory(args).await,
        _ => format!("Unknown tool: {}", name),
    }
}

async fn execute_bash(args: &Value) -> String {
    let command = args["command"].as_str().unwrap_or("");
    let workdir = args["workdir"].as_str().map(PathBuf::from);

    let mut cmd = tokio::process::Command::new("bash");
    cmd.arg("-c").arg(command);
    if let Some(dir) = workdir {
        cmd.current_dir(dir);
    }

    match cmd.output().await {
        Ok(output) => {
            let status = output.status;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut result = String::new();
            if !status.success() {
                result.push_str(&format!("(exit code: {})\n", status.code().unwrap_or(-1)));
            }
            if !stdout.is_empty() {
                result.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !result.is_empty() {
                    result.push_str("\n--- STDERR ---\n");
                }
                result.push_str(&stderr);
            }
            if result.is_empty() {
                "(no output)".to_string()
            } else {
                result
            }
        }
        Err(e) => format!("Failed to execute command: {}", e),
    }
}

async fn execute_read_file(args: &Value) -> String {
    let path = match args["path"].as_str() {
        Some(p) => p,
        None => return "Error: missing 'path' parameter".to_string(),
    };

    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(2000) as usize;

    match tokio::fs::read_to_string(path).await {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let end = std::cmp::min(offset + limit, lines.len());
            if offset >= lines.len() {
                "(offset beyond file length)".to_string()
            } else {
                let total = lines.len();
                let shown = end - offset;
                let mut result = format!(
                    "File: {} ({} lines, showing {}-{})\n",
                    path,
                    total,
                    offset + 1,
                    end
                );
                result.push_str(&lines[offset..end].join("\n"));
                if shown < total {
                    result.push_str(&format!(
                        "\n(... truncated, showing {} of {} lines)",
                        shown, total
                    ));
                }
                result
            }
        }
        Err(e) => format!("Error reading file: {}", e),
    }
}

async fn execute_write_file(args: &Value) -> String {
    let path = match args["path"].as_str() {
        Some(p) => p,
        None => return "Error: missing 'path' parameter".to_string(),
    };
    let content = match args["content"].as_str() {
        Some(c) => c,
        None => return "Error: missing 'content' parameter".to_string(),
    };

    let old_exists = tokio::fs::metadata(path).await.is_ok();
    let old_size = if old_exists {
        tokio::fs::read_to_string(path).await.ok().map(|c| c.len())
    } else {
        None
    };

    match tokio::fs::write(path, content).await {
        Ok(_) => {
            let new_size = content.len();
            if let Some(old_len) = old_size {
                let delta = if new_size > old_len {
                    format!("+{}", new_size - old_len)
                } else {
                    format!("-{}", old_len - new_size)
                };
                format!(
                    "Wrote {} ({} bytes, was {} bytes, delta {})",
                    path, new_size, old_len, delta
                )
            } else {
                format!("Created {} ({} bytes)", path, new_size)
            }
        }
        Err(e) => format!("Error writing file: {}", e),
    }
}

async fn execute_edit_file(args: &Value) -> String {
    let path = match args["path"].as_str() {
        Some(p) => p,
        None => return "Error: missing 'path' parameter".to_string(),
    };
    let old_string = match args["old_string"].as_str() {
        Some(s) => s,
        None => return "Error: missing 'old_string' parameter".to_string(),
    };
    let new_string = match args["new_string"].as_str() {
        Some(s) => s,
        None => return "Error: missing 'new_string' parameter".to_string(),
    };

    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(e) => return format!("Error reading file: {}", e),
    };

    let count = content.matches(old_string).count();
    if count == 0 {
        let display = safe_truncate_old(old_string, 100);
        return format!(
            "Error: old_string not found in {}. old_string={:?}",
            path, display
        );
    }
    if count > 1 {
        return format!(
            "Error: old_string appears {} times in {}. Use a more unique match.",
            count, path
        );
    }

    let new_content = content.replacen(old_string, new_string, 1);

    match tokio::fs::write(path, new_content).await {
        Ok(_) => {
            let diff = format_edit_diff(path, &content, old_string, new_string);
            format!("Edited {}\n{}", path, diff)
        }
        Err(e) => format!("Error writing file: {}", e),
    }
}

fn format_edit_diff(path: &str, content: &str, old_string: &str, new_string: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let old_first_line = old_string.lines().next().unwrap_or(old_string);
    let line_idx = lines.iter().position(|l| l.contains(old_first_line));

    let mut result = String::new();
    result.push_str(&format!("--- a/{}\n+++ b/{}\n", path, path));

    if let Some(idx) = line_idx {
        let ctx_start = idx.saturating_sub(2);
        let ctx_end = (idx + old_string.lines().count() + 1).min(lines.len());
        result.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            ctx_start + 1,
            ctx_end - ctx_start,
            ctx_start + 1,
            ctx_end - ctx_start + new_string.lines().count() - old_string.lines().count(),
        ));
        for line in &lines[ctx_start..idx] {
            result.push_str(&format!(" {}\n", line));
        }
        for line in old_string.lines() {
            result.push_str(&format!("-{}\n", line));
        }
        for line in new_string.lines() {
            result.push_str(&format!("+{}\n", line));
        }
        let after_start = idx + old_string.lines().count();
        for line in &lines[after_start..ctx_end.min(after_start + 2)] {
            result.push_str(&format!(" {}\n", line));
        }
    } else {
        result.push_str(&format!("-{}\n+{}\n", old_string, new_string));
    }

    result
}

async fn execute_glob(args: &Value) -> String {
    let pattern = match args["pattern"].as_str() {
        Some(p) => p,
        None => return "Error: missing 'pattern' parameter".to_string(),
    };

    match glob::glob(pattern) {
        Ok(paths) => {
            let results: Vec<String> = paths
                .filter_map(|entry| entry.ok())
                .map(|path| path.display().to_string())
                .collect();
            if results.is_empty() {
                "No files matched the pattern".to_string()
            } else {
                results.join("\n")
            }
        }
        Err(e) => format!("Error with glob pattern: {}", e),
    }
}

async fn execute_grep(args: &Value) -> String {
    let pattern = match args["pattern"].as_str() {
        Some(p) => p,
        None => return "Error: missing 'pattern' parameter".to_string(),
    };
    let include = args["include"].as_str().unwrap_or("*");
    let path = args["path"].as_str().unwrap_or(".");

    let regex = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return format!("Invalid regex: {}", e),
    };

    let glob_pattern = if include == "*" {
        format!("{}/**/*", path)
    } else {
        format!("{}/{}", path, include)
    };

    let mut results = Vec::new();
    if let Ok(paths) = glob::glob(&glob_pattern) {
        for entry in paths.filter_map(|e| e.ok()) {
            if entry.is_file() {
                let path_str = entry.display().to_string();
                if let Ok(content) = std::fs::read_to_string(&entry) {
                    for (line_num, line) in content.lines().enumerate() {
                        if regex.is_match(line) {
                            results.push(format!("{}:{}: {}", path_str, line_num + 1, line));
                        }
                    }
                }
            }
        }
    }

    if results.is_empty() {
        "No matches found".to_string()
    } else {
        results.join("\n")
    }
}

async fn execute_list_directory(args: &Value) -> String {
    let path = match args["path"].as_str() {
        Some(p) => p,
        None => return "Error: missing 'path' parameter".to_string(),
    };

    match tokio::fs::read_dir(path).await {
        Ok(mut entries) => {
            let mut results = Vec::new();
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry
                    .file_type()
                    .await
                    .map(|ft| ft.is_dir())
                    .unwrap_or(false);
                let prefix = if is_dir { "[DIR] " } else { "[FILE] " };
                results.push(format!("{}{}", prefix, name));
            }
            results.sort();
            if results.is_empty() {
                "Directory is empty".to_string()
            } else {
                results.join("\n")
            }
        }
        Err(e) => format!("Error reading directory: {}", e),
    }
}

fn safe_truncate_old(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max_chars {
        let truncated: String = chars[..max_chars].iter().collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_native_tool_names_count() {
        let names = native_tool_names();
        assert_eq!(names.len(), 12);
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"present_choices"));
    }

    #[test]
    fn test_native_tool_definitions_count() {
        let defs = native_tool_definitions();
        assert_eq!(defs.len(), 12);
        for def in &defs {
            let func = def.get("function").unwrap();
            assert!(func.get("name").is_some());
            assert!(func.get("description").is_some());
            assert!(func.get("parameters").is_some());
        }
    }

    #[test]
    fn test_safe_truncate_old_short() {
        assert_eq!(safe_truncate_old("hello", 10), "hello");
    }

    #[test]
    fn test_safe_truncate_old_long() {
        let result = safe_truncate_old("hello world", 5);
        assert_eq!(result, "hello...");
    }

    #[tokio::test]
    async fn test_execute_bash_success() {
        let args = json!({"command": "echo hello"});
        let result = execute_bash(&args).await;
        assert!(result.contains("hello"));
    }

    #[tokio::test]
    async fn test_execute_bash_with_workdir() {
        let tmp = TempDir::new().unwrap();
        let args = json!({"command": "pwd", "workdir": tmp.path().to_string_lossy()});
        let result = execute_bash(&args).await;
        assert!(result.contains(tmp.path().to_string_lossy().as_ref()));
    }

    #[tokio::test]
    async fn test_execute_bash_failure() {
        let args = json!({"command": "false"});
        let result = execute_bash(&args).await;
        assert!(result.contains("exit code"));
    }

    #[tokio::test]
    async fn test_execute_read_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        fs::write(&path, "line1\nline2\nline3").unwrap();

        let args = json!({"path": path.to_string_lossy(), "offset": 1, "limit": 1});
        let result = execute_read_file(&args).await;
        assert!(result.contains("line2"));
    }

    #[tokio::test]
    async fn test_execute_read_file_not_found() {
        let args = json!({"path": "/nonexistent/file.txt"});
        let result = execute_read_file(&args).await;
        assert!(result.contains("Error"));
    }

    #[tokio::test]
    async fn test_execute_read_file_offset_beyond() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        fs::write(&path, "hello").unwrap();

        let args = json!({"path": path.to_string_lossy(), "offset": 100});
        let result = execute_read_file(&args).await;
        assert!(result.contains("offset beyond"));
    }

    #[tokio::test]
    async fn test_execute_write_file_create() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("new.txt");
        let args = json!({"path": path.to_string_lossy(), "content": "hello world"});
        let result = execute_write_file(&args).await;
        assert!(result.contains("Created"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_execute_write_file_overwrite() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("existing.txt");
        fs::write(&path, "old content").unwrap();

        let args = json!({"path": path.to_string_lossy(), "content": "new"});
        let result = execute_write_file(&args).await;
        assert!(result.contains("delta"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[tokio::test]
    async fn test_execute_edit_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("edit.txt");
        fs::write(&path, "line1\nold line\nline3").unwrap();

        let args = json!({
            "path": path.to_string_lossy(),
            "old_string": "old line",
            "new_string": "new line"
        });
        let result = execute_edit_file(&args).await;
        assert!(result.contains("Edited"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "line1\nnew line\nline3");
    }

    #[tokio::test]
    async fn test_execute_edit_file_not_found() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("edit.txt");
        fs::write(&path, "hello").unwrap();

        let args = json!({
            "path": path.to_string_lossy(),
            "old_string": "not found",
            "new_string": "x"
        });
        let result = execute_edit_file(&args).await;
        assert!(result.contains("not found"));
    }

    #[tokio::test]
    async fn test_execute_edit_file_multiple_matches() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("edit.txt");
        fs::write(&path, "x\nx").unwrap();

        let args = json!({
            "path": path.to_string_lossy(),
            "old_string": "x",
            "new_string": "y"
        });
        let result = execute_edit_file(&args).await;
        assert!(result.contains("appears 2 times"));
    }

    #[tokio::test]
    async fn test_execute_glob() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        fs::write(&path, "").unwrap();

        let pattern = format!("{}/*.txt", tmp.path().to_string_lossy());
        let args = json!({"pattern": pattern});
        let result = execute_glob(&args).await;
        assert!(result.contains("test.txt"));
    }

    #[tokio::test]
    async fn test_execute_grep() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("grep.txt");
        fs::write(&path, "hello world").unwrap();

        let args = json!({
            "pattern": "hello",
            "include": "*.txt",
            "path": tmp.path().to_string_lossy()
        });
        let result = execute_grep(&args).await;
        assert!(result.contains("hello world"));
    }

    #[tokio::test]
    async fn test_execute_grep_no_matches() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("grep.txt");
        fs::write(&path, "hello world").unwrap();

        let args = json!({
            "pattern": "nonexistent",
            "include": "*.txt",
            "path": tmp.path().to_string_lossy()
        });
        let result = execute_grep(&args).await;
        assert_eq!(result, "No matches found");
    }

    #[tokio::test]
    async fn test_execute_list_directory() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), "").unwrap();

        let args = json!({"path": tmp.path().to_string_lossy()});
        let result = execute_list_directory(&args).await;
        assert!(
            result.contains("[DIR] subdir"),
            "expected subdir in dir listing, got: {}",
            result
        );

        let args2 = json!({"path": subdir.to_string_lossy()});
        let result2 = execute_list_directory(&args2).await;
        assert!(
            result2.contains("[FILE] file.txt"),
            "expected file.txt in subdir listing, got: {}",
            result2
        );
    }

    #[tokio::test]
    async fn test_execute_list_directory_empty() {
        let tmp = TempDir::new().unwrap();
        let args = json!({"path": tmp.path().to_string_lossy()});
        let result = execute_list_directory(&args).await;
        assert_eq!(result, "Directory is empty");
    }

    #[tokio::test]
    async fn test_execute_list_directory_not_found() {
        let args = json!({"path": "/nonexistent/dir"});
        let result = execute_list_directory(&args).await;
        assert!(result.contains("Error"));
    }

    #[test]
    fn test_format_edit_diff_found() {
        let content = "line1\nold line\nline3";
        let result = format_edit_diff("/test.txt", content, "old line", "new line");
        assert!(result.contains("--- a/"));
        assert!(result.contains("old line"));
        assert!(result.contains("+new line"));
    }

    #[test]
    fn test_format_edit_diff_not_found() {
        let content = "line1\nline2\nline3";
        let result = format_edit_diff("/test.txt", content, "old line", "new line");
        assert!(result.contains("--- a/"));
        assert!(result.contains("+new line"));
    }
}
