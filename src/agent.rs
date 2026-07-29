pub mod request;
pub mod skill;
pub mod tools;

pub use tools::truncate;

use async_openai::types::chat::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};

use crate::agent::request::{ParsedStream, ToolCallState, parse_stream, send_request};
use crate::config::Config;
use crate::mcp::McpManager;
use crate::permission::{PermissionDecision, PermissionOp, PermissionRequest};

#[derive(Debug)]
pub enum AgentEvent {
    Token(String),
    MessageComplete(String),
    ToolCallStart {
        name: String,
        args: String,
    },
    ToolCallResult {
        name: String,
        result: String,
    },
    Done,
    Error(String),
    Status(String),
    TokenUsage {
        prompt: u64,
        completion: u64,
    },
    PermissionRequired {
        request: PermissionRequest,
        respond: oneshot::Sender<PermissionDecision>,
    },
    ChoiceRequired {
        choices: Vec<String>,
        respond: oneshot::Sender<ChoiceResult>,
    },
    McpServerConnected {
        server_name: String,
        prefixed_tools: Vec<String>,
    },
    McpServerFailed {
        server_name: String,
        error: String,
    },
    McpConnectionsDone,
    /// AI-predicted next user input (generated after each response).
    NextInputPrediction(String),
}

#[derive(Debug)]
pub struct ChoiceResult {
    pub selected_index: usize,
    pub custom_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: u64,
    pub description: String,
    pub status: TodoStatus,
    pub priority: TodoPriority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoStatus {
    Pending,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TodoPriority {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TodoStore {
    todos: Vec<Todo>,
    next_id: u64,
}

impl TodoStore {
    fn new() -> Self {
        TodoStore {
            todos: Vec::new(),
            next_id: 1,
        }
    }

    fn add(&mut self, description: String, priority: TodoPriority) -> Todo {
        let todo = Todo {
            id: self.next_id,
            description,
            status: TodoStatus::Pending,
            priority,
        };
        self.next_id += 1;
        self.todos.push(todo.clone());
        todo
    }

    fn update(&mut self, id: u64, updates: TodoUpdate) -> Option<Todo> {
        let todo = self.todos.iter_mut().find(|t| t.id == id)?;
        if let Some(status) = updates.status {
            todo.status = status;
        }
        if let Some(desc) = updates.description {
            todo.description = desc;
        }
        if let Some(priority) = updates.priority {
            todo.priority = priority;
        }
        Some(todo.clone())
    }

    fn remove(&mut self, id: u64) -> bool {
        let idx = self.todos.iter().position(|t| t.id == id);
        if let Some(i) = idx {
            self.todos.remove(i);
            true
        } else {
            false
        }
    }

    fn list(&self) -> &[Todo] {
        &self.todos
    }
}

#[derive(Debug)]
struct TodoUpdate {
    status: Option<TodoStatus>,
    description: Option<String>,
    priority: Option<TodoPriority>,
}

pub struct Agent {
    http_client: reqwest::Client,
    config: Config,
    messages: Vec<ChatCompletionRequestMessage>,
    system_prompt: String,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    todo_store: TodoStore,
    mcp_manager: McpManager,
}

impl Agent {
    pub fn new(
        config: Config,
        system_prompt: String,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
        mcp_manager: McpManager,
    ) -> Self {
        let messages = vec![ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(system_prompt.clone()),
                name: None,
            },
        )];

        Agent {
            http_client: reqwest::Client::new(),
            config,
            messages,
            system_prompt,
            event_tx,
            todo_store: TodoStore::new(),
            mcp_manager,
        }
    }

    fn get_tools(&self) -> Vec<ChatCompletionTools> {
        let mut all_tools = Vec::new();

        // Native tools
        let native_tools = tools::native_tool_definitions();
        for tool_def in native_tools {
            if let Ok(func_obj) = serde_json::from_value(tool_def["function"].clone()) {
                all_tools.push(ChatCompletionTools::Function(ChatCompletionTool {
                    function: func_obj,
                }));
            }
        }

        // MCP tools (prefixed with server name to avoid collisions)
        for (_, _, tool_def) in self.mcp_manager.get_openai_tools() {
            if let Ok(func_obj) = serde_json::from_value(tool_def["function"].clone()) {
                all_tools.push(ChatCompletionTools::Function(ChatCompletionTool {
                    function: func_obj,
                }));
            }
        }

        all_tools
    }

    pub async fn send_message(&mut self, user_message: &str) -> String {
        tracing::info!(
            "Processing message, history length: {}",
            self.messages.len()
        );
        self.messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(user_message.to_string()),
                name: None,
            },
        ));

        let mut full_response = String::new();
        let max_iterations = 500;

        for _iteration in 0..max_iterations {
            let _ = self.event_tx.send(AgentEvent::Status("Thinking...".into()));

            let tools = self.get_tools();

            let http_resp = match send_request(
                &self.config,
                self.messages.clone(),
                tools,
                &self.http_client,
            )
            .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let _ = self.event_tx.send(AgentEvent::Error(e.clone()));
                    let _ = self.event_tx.send(AgentEvent::Done);
                    return e;
                }
            };

            let parsed = match parse_stream(http_resp, Some(self.event_tx.clone())).await {
                Ok(p) => p,
                Err(e) => {
                    let _ = self.event_tx.send(AgentEvent::Error(e.clone()));
                    let _ = self.event_tx.send(AgentEvent::Done);
                    return e;
                }
            };

            let ParsedStream {
                content,
                tool_calls,
            } = parsed;

            if tool_calls.is_empty() {
                let stripped = extract_answer(&content);
                let final_text = stripped.unwrap_or(content.clone());

                if !final_text.is_empty() {
                    full_response = final_text.clone();
                    self.messages.push(ChatCompletionRequestMessage::Assistant(
                        ChatCompletionRequestAssistantMessage {
                            content: Some(ChatCompletionRequestAssistantMessageContent::Text(
                                final_text,
                            )),
                            ..Default::default()
                        },
                    ));
                }

                let _ = self
                    .event_tx
                    .send(AgentEvent::MessageComplete(full_response.clone()));
                let _ = self.event_tx.send(AgentEvent::Done);

                // Fire off a lightweight prediction for the next user input
                let tx = self.event_tx.clone();
                let config = self.config.clone();
                let recent = self
                    .messages
                    .iter()
                    .rev()
                    .take(4)
                    .filter_map(msg_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                tokio::spawn(async move {
                    if let Some(prediction) = predict_next_input(&config, &recent).await {
                        let _ = tx.send(AgentEvent::NextInputPrediction(prediction));
                    }
                });

                return full_response;
            }

            if let Err(e) = self.handle_tool_calls(tool_calls, &content).await {
                let _ = self.event_tx.send(AgentEvent::Error(e.clone()));
                let _ = self.event_tx.send(AgentEvent::Done);
                return e;
            }

            continue;
        }

        let _ = self
            .event_tx
            .send(AgentEvent::Status("Max iterations reached".into()));
        let _ = self.event_tx.send(AgentEvent::Done);
        full_response
    }

    /// Build assistant and tool-call messages from parsed tool calls,
    /// execute each tool, and push the results as tool messages.
    async fn handle_tool_calls(
        &mut self,
        tool_calls: Vec<ToolCallState>,
        content_buffer: &str,
    ) -> Result<(), String> {
        tracing::info!("Executing {} tool call(s)", tool_calls.len());
        // Push assistant message with tool calls
        {
            let assistant_tool_calls: Vec<ChatCompletionMessageToolCalls> = tool_calls
                .iter()
                .map(|tc| {
                    ChatCompletionMessageToolCalls::Function(ChatCompletionMessageToolCall {
                        id: tc.id.clone(),
                        function: FunctionCall {
                            name: if tc.name.is_empty() {
                                "unknown".to_string()
                            } else {
                                tc.name.clone()
                            },
                            arguments: tc.arguments.clone(),
                        },
                    })
                })
                .collect();

            self.messages.push(ChatCompletionRequestMessage::Assistant(
                ChatCompletionRequestAssistantMessage {
                    content: if content_buffer.is_empty() {
                        None
                    } else {
                        Some(ChatCompletionRequestAssistantMessageContent::Text(
                            content_buffer.to_string(),
                        ))
                    },
                    tool_calls: Some(assistant_tool_calls),
                    ..Default::default()
                },
            ));
        }

        for tc in &tool_calls {
            if tc.name.is_empty() {
                let _ = self.event_tx.send(AgentEvent::ToolCallStart {
                    name: "(incomplete)".into(),
                    args: tc.arguments.clone(),
                });
                let error_msg =
                    "Error: tool call stream ended before tool name was received".to_string();
                let _ = self.event_tx.send(AgentEvent::ToolCallResult {
                    name: "(incomplete)".into(),
                    result: error_msg.clone(),
                });
                self.messages.push(ChatCompletionRequestMessage::Tool(
                    ChatCompletionRequestToolMessage {
                        content: ChatCompletionRequestToolMessageContent::Text(error_msg),
                        tool_call_id: tc.id.clone(),
                    },
                ));
                continue;
            }

            let _ = self.event_tx.send(AgentEvent::ToolCallStart {
                name: tc.name.clone(),
                args: tc.arguments.clone(),
            });

            let args: Value = match serde_json::from_str(&tc.arguments) {
                Ok(v) => v,
                Err(e) => {
                    let error_msg = format!(
                        "Error: failed to parse tool arguments for '{}': {}",
                        tc.name, e
                    );
                    let _ = self.event_tx.send(AgentEvent::ToolCallResult {
                        name: tc.name.clone(),
                        result: error_msg.clone(),
                    });
                    self.messages.push(ChatCompletionRequestMessage::Tool(
                        ChatCompletionRequestToolMessage {
                            content: ChatCompletionRequestToolMessageContent::Text(error_msg),
                            tool_call_id: tc.id.clone(),
                        },
                    ));
                    continue;
                }
            };

            let start = std::time::Instant::now();

            let result = match tc.name.as_str() {
                "add_todo" | "update_todo" | "list_todos" | "remove_todo" => {
                    self.execute_todo_tool(&tc.name, &args).await
                }
                "present_choices" => self.execute_choices_tool(&args).await,
                _ => {
                    if self.mcp_manager.has_prefixed_tool(&tc.name) {
                        match self
                            .mcp_manager
                            .call_prefixed_tool(&tc.name, args.clone())
                            .await
                        {
                            Ok(result_str) => result_str,
                            Err(e) => format!("MCP tool error: {}", e),
                        }
                    } else if let Some((op, path)) =
                        crate::permission::needs_permission(&tc.name, &args)
                    {
                        let decision = self.request_permission(op, &path, &tc.name).await;
                        if decision == PermissionDecision::Deny {
                            format!("⛔ Permission denied: {} on {}", tc.name, path)
                        } else {
                            tools::execute_native_tool(&tc.name, &args).await
                        }
                    } else {
                        tools::execute_native_tool(&tc.name, &args).await
                    }
                }
            };

            let elapsed = start.elapsed();
            let elapsed_secs = elapsed.as_secs_f64();

            let display_result = if elapsed_secs >= 1.0 {
                format!("[{:.1}s] {}", elapsed_secs, result)
            } else {
                format!("[{:.0}ms] {}", elapsed_secs * 1000.0, result)
            };

            let _ = self.event_tx.send(AgentEvent::ToolCallResult {
                name: tc.name.clone(),
                result: display_result,
            });

            self.messages.push(ChatCompletionRequestMessage::Tool(
                ChatCompletionRequestToolMessage {
                    content: ChatCompletionRequestToolMessageContent::Text(result),
                    tool_call_id: tc.id.clone(),
                },
            ));
        }

        Ok(())
    }

    pub fn clear_history(&mut self) {
        self.messages.clear();
        self.messages.push(ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(
                    self.system_prompt.clone(),
                ),
                name: None,
            },
        ));
    }

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

    async fn execute_todo_tool(&mut self, name: &str, args: &Value) -> String {
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let id = args.get("id").and_then(|v| v.as_u64());
        let status = args.get("status").and_then(|v| v.as_str());
        let priority = args.get("priority").and_then(|v| v.as_str());

        match name {
            "add_todo" => {
                if description.is_empty() {
                    return "Error: missing 'description' parameter".to_string();
                }
                let prio = match priority {
                    Some("high") | Some("High") => TodoPriority::High,
                    Some("low") | Some("Low") => TodoPriority::Low,
                    _ => TodoPriority::Medium,
                };
                let todo = self.todo_store.add(description.to_string(), prio);
                format!(
                    "✅ Todo #{} added: {} [{}]",
                    todo.id,
                    todo.description,
                    match todo.priority {
                        TodoPriority::High => "High",
                        TodoPriority::Medium => "Medium",
                        TodoPriority::Low => "Low",
                    }
                )
            }
            "update_todo" => {
                let todo_id = match id {
                    Some(i) => i,
                    None => return "Error: missing 'id' parameter".to_string(),
                };
                let todo_status = match status {
                    Some("done") | Some("Done") => Some(TodoStatus::Done),
                    Some("pending") | Some("Pending") => Some(TodoStatus::Pending),
                    _ => None,
                };
                let todo_priority = match priority {
                    Some("high") | Some("High") => Some(TodoPriority::High),
                    Some("low") | Some("Low") => Some(TodoPriority::Low),
                    Some("medium") | Some("Medium") => Some(TodoPriority::Medium),
                    _ => None,
                };
                let desc = if description.is_empty() {
                    None
                } else {
                    Some(description.to_string())
                };
                let updates = TodoUpdate {
                    status: todo_status,
                    description: desc,
                    priority: todo_priority,
                };
                if self.todo_store.update(todo_id, updates).is_some() {
                    format!("✅ Todo #{} updated", todo_id)
                } else {
                    format!("❌ Todo #{} not found", todo_id)
                }
            }
            "list_todos" => {
                let todos = self.todo_store.list();
                if todos.is_empty() {
                    "📋 No todos yet.".to_string()
                } else {
                    let mut result = "📋 **Todos:**\n".to_string();
                    for todo in todos {
                        let status_mark = match todo.status {
                            TodoStatus::Done => "✅",
                            TodoStatus::Pending => "⬜",
                        };
                        let priority_icon = match todo.priority {
                            TodoPriority::High => " 🔴",
                            TodoPriority::Medium => " 🟡",
                            TodoPriority::Low => " 🟢",
                        };
                        result.push_str(&format!(
                            "- {} `#{}` {}{}\n",
                            status_mark, todo.id, todo.description, priority_icon
                        ));
                    }
                    result
                }
            }
            "remove_todo" => {
                let todo_id = match id {
                    Some(i) => i,
                    None => return "Error: missing 'id' parameter".to_string(),
                };
                if self.todo_store.remove(todo_id) {
                    format!("🗑️ Todo #{} removed", todo_id)
                } else {
                    format!("❌ Todo #{} not found", todo_id)
                }
            }
            _ => format!("Unknown todo tool: {}", name),
        }
    }

    async fn execute_choices_tool(&self, args: &Value) -> String {
        let choices = match args.get("choices").and_then(|v| v.as_array()) {
            Some(arr) => {
                let opts: Vec<String> = arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                if opts.is_empty() {
                    return "Error: 'choices' array is empty or invalid".to_string();
                }
                opts
            }
            None => return "Error: missing 'choices' parameter".to_string(),
        };

        let (tx, rx) = oneshot::channel();
        let _ = self.event_tx.send(AgentEvent::ChoiceRequired {
            choices: choices.clone(),
            respond: tx,
        });

        let result = rx.await.unwrap_or(ChoiceResult {
            selected_index: 0,
            custom_text: None,
        });

        if let Some(custom) = result.custom_text {
            format!(
                "User chose a custom approach (option #{}): {}",
                result.selected_index + 1,
                custom
            )
        } else {
            format!(
                "User selected option #{}: {}",
                result.selected_index + 1,
                choices
                    .get(result.selected_index)
                    .unwrap_or(&"unknown".to_string())
            )
        }
    }

    /// Connect to all configured MCP servers in the background.
    /// Sends events for each server connection attempt and when all are done.
    pub async fn connect_mcp_servers(&mut self) {
        let _ = self
            .event_tx
            .send(AgentEvent::Status("Connecting MCP servers...".into()));

        let results = self.mcp_manager.connect_servers().await;

        for (name, result) in results {
            match result {
                Ok(tools) => {
                    let prefixed: Vec<String> = tools
                        .iter()
                        .map(|t| format!("{}_{}", name, t.name))
                        .collect();
                    let _ = self.event_tx.send(AgentEvent::McpServerConnected {
                        server_name: name,
                        prefixed_tools: prefixed,
                    });
                }
                Err(e) => {
                    let _ = self.event_tx.send(AgentEvent::McpServerFailed {
                        server_name: name,
                        error: e,
                    });
                }
            }
        }

        let _ = self.event_tx.send(AgentEvent::McpConnectionsDone);
    }
}

/// Extract content from `<answer>...</answer>` tags if present.
/// Returns `Some(text)` with the tags stripped, or `None` if no tags found.
fn extract_answer(content: &str) -> Option<String> {
    let start = content.find("<answer>")?;
    let end = content.rfind("</answer>")?;
    let inner_start = start + "<answer>".len();
    if inner_start >= end {
        return Some(String::new());
    }
    Some(content[inner_start..end].trim().to_string())
}

/// Extract the text content of a chat message (if it has text).
fn msg_text(msg: &ChatCompletionRequestMessage) -> Option<String> {
    match msg {
        ChatCompletionRequestMessage::User(u) => match &u.content {
            ChatCompletionRequestUserMessageContent::Text(t) => Some(format!("User: {}", t)),
            _ => None,
        },
        ChatCompletionRequestMessage::Assistant(a) => match &a.content {
            Some(ChatCompletionRequestAssistantMessageContent::Text(t)) => {
                Some(format!("Assistant: {}", t))
            }
            _ => None,
        },
        ChatCompletionRequestMessage::System(s) => match &s.content {
            ChatCompletionRequestSystemMessageContent::Text(t) => Some(format!("System: {}", t)),
            _ => None,
        },
        _ => None,
    }
}

/// Ask the model to predict what the user will type next, based on the recent
/// conversation history. Returns the prediction or None on error/timeout.
async fn predict_next_input(config: &Config, recent: &str) -> Option<String> {
    let url = format!("{}/chat/completions", config.base_url);
    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            {
                "role": "system",
                "content": "Based on the conversation, predict what the user will say next in under 100 characters. Return ONLY the prediction, no explanation, no quotes, no punctuation."
            },
            {
                "role": "user",
                "content": format!("Recent conversation:\n{}", recent)
            }
        ],
        "max_tokens": 500,
        "temperature": 1,
        "stream": false
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let data: Value = resp.json().await.ok()?;
    let text = data["choices"][0]["message"]["content"]
        .as_str()?
        .trim()
        .to_string();
    // Use char count, not byte count — Chinese/Japanese text needs 3 bytes/char
    if text.is_empty() || text.chars().count() > 40 {
        return None;
    }
    Some(text)
}
