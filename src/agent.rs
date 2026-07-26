pub mod skill;
pub mod tools;

use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::chat::*;
use eventsource_stream::EventStream;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};

use crate::config::Config;
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
    PermissionRequired {
        request: PermissionRequest,
        respond: oneshot::Sender<PermissionDecision>,
    },
    ChoiceRequired {
        choices: Vec<String>,
        respond: oneshot::Sender<ChoiceResult>,
    },
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

#[allow(dead_code)]
pub struct Agent {
    client: Client<OpenAIConfig>,
    http_client: reqwest::Client,
    config: Config,
    messages: Vec<ChatCompletionRequestMessage>,
    system_prompt: String,
    event_tx: mpsc::UnboundedSender<AgentEvent>,
    todo_store: TodoStore,
}

impl Agent {
    pub fn new(
        config: Config,
        system_prompt: String,
        event_tx: mpsc::UnboundedSender<AgentEvent>,
    ) -> Self {
        let openai_config = OpenAIConfig::new()
            .with_api_key(&config.api_key)
            .with_api_base(&config.base_url);

        let client = Client::with_config(openai_config);

        let messages = vec![ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(system_prompt.clone()),
                name: None,
            },
        )];

        Agent {
            client,
            http_client: reqwest::Client::new(),
            config,
            messages,
            system_prompt,
            event_tx,
            todo_store: TodoStore::new(),
        }
    }

    fn get_tools(&self) -> Vec<ChatCompletionTools> {
        let native_tools = tools::native_tool_definitions();
        native_tools
            .into_iter()
            .filter_map(|tool_def| {
                let func_obj: FunctionObject =
                    serde_json::from_value(tool_def["function"].clone()).ok()?;
                Some(ChatCompletionTools::Function(ChatCompletionTool {
                    function: func_obj,
                }))
            })
            .collect()
    }

    pub async fn send_message(&mut self, user_message: &str) -> String {
        self.messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(user_message.to_string()),
                name: None,
            },
        ));

        let mut full_response = String::new();
        let max_iterations = 20;

        for _iteration in 0..max_iterations {
            let _ = self.event_tx.send(AgentEvent::Status("Thinking...".into()));

            let tools = self.get_tools();
            let mut request = CreateChatCompletionRequestArgs::default()
                .model(&self.config.model)
                .messages(self.messages.clone())
                .tools(tools)
                .stream(true)
                .build()
                .unwrap();

            if let Some(max_tokens) = self.config.max_tokens {
                request.max_completion_tokens = Some(max_tokens);
            }

            let url = format!("{}/chat/completions", self.config.base_url);
            let http_response = self
                .http_client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .json(&request)
                .send()
                .await;

            match http_response {
                Ok(response) => {
                    if !response.status().is_success() {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        let error_msg = format!("API error: HTTP {} - {}", status, body);
                        let _ = self.event_tx.send(AgentEvent::Error(error_msg.clone()));
                        let _ = self.event_tx.send(AgentEvent::Done);
                        return error_msg;
                    }

                    let mut content_buffer = String::new();
                    let mut tool_calls: Vec<ToolCallState> = Vec::new();
                    let byte_stream = response
                        .bytes_stream()
                        .map(|r| r.map_err(std::io::Error::other));
                    let mut event_stream = std::pin::pin!(EventStream::new(byte_stream));

                    loop {
                        let timeout_dur = if content_buffer.is_empty() {
                            std::time::Duration::from_secs(30)
                        } else {
                            std::time::Duration::from_secs(8)
                        };

                        let event_result =
                            match tokio::time::timeout(timeout_dur, event_stream.next()).await {
                                Ok(result) => result,
                                Err(_) => {
                                    tracing::warn!("Stream idle timeout reached, ending stream");
                                    break;
                                }
                            };

                        match event_result {
                            Some(Ok(event)) => {
                                // tracing::info!("data: {}", event.data);
                                if event.data.trim() == "[DONE]" {
                                    break;
                                }
                                if event.event == "keepalive" {
                                    continue;
                                }

                                let mut value: Value = match serde_json::from_str(&event.data) {
                                    Ok(v) => v,
                                    Err(e) => {
                                        tracing::warn!("Failed to parse chunk JSON: {}", e);
                                        continue;
                                    }
                                };

                                fix_response_value(&mut value);

                                if let Some(usage) = value.get("usage") {
                                    let prompt = usage
                                        .get("prompt_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);
                                    let completion = usage
                                        .get("completion_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);
                                    let total = usage
                                        .get("total_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);
                                    let _ = self.event_tx.send(AgentEvent::Token(
                                        format!("\n\n--- Tokens: {} prompt + {} completion = {} total ---\n",
                                            prompt, completion, total)
                                    ));
                                }

                                let response: CreateChatCompletionStreamResponse =
                                    match serde_json::from_value(value) {
                                        Ok(r) => r,
                                        Err(e) => {
                                            tracing::warn!("Failed to deserialize chunk: {}", e);
                                            continue;
                                        }
                                    };

                                if let Some(choice) = response.choices.first() {
                                    tracing::info!("choice: {:?}", choice);
                                    tracing::info!("choice.delta: {:#?}", choice.delta);
                                    let delta = &choice.delta;

                                    if let Some(ref text) = delta.content {
                                        content_buffer.push_str(text);
                                        let _ = self.event_tx.send(AgentEvent::Token(text.clone()));
                                    }

                                    if let Some(ref tool_call_deltas) = delta.tool_calls {
                                        for tc_delta in tool_call_deltas {
                                            let idx = tc_delta.index as usize;

                                            while tool_calls.len() <= idx {
                                                tool_calls.push(ToolCallState {
                                                    id: String::new(),
                                                    name: String::new(),
                                                    arguments: String::new(),
                                                });
                                            }

                                            if let Some(ref id) = tc_delta.id
                                                && !id.is_empty()
                                            {
                                                tool_calls[idx].id = id.clone();
                                            }
                                            if let Some(ref func) = tc_delta.function {
                                                if let Some(ref name) = func.name
                                                    && !name.is_empty()
                                                {
                                                    tracing::info!("func.name: {}", name);
                                                    tool_calls[idx].name = name.clone();
                                                }
                                                if let Some(ref args) = func.arguments {
                                                    tool_calls[idx].arguments.push_str(args);
                                                }
                                            }
                                        }
                                    }
                                    tracing::info!("tool_calls: {:?}", tool_calls);
                                    if choice.finish_reason.is_some() {
                                        break;
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                let _ = self
                                    .event_tx
                                    .send(AgentEvent::Error(format!("Stream error: {}", e)));
                                return full_response;
                            }
                            None => {
                                break;
                            }
                        }
                    }
                    // If we have tool calls, execute them and continue
                    if !tool_calls.is_empty() && !tool_calls.iter().any(|tc| tc.name.is_empty()) {
                        let assistant_tool_calls: Vec<ChatCompletionMessageToolCalls> = tool_calls
                            .iter()
                            .map(|tc| {
                                ChatCompletionMessageToolCalls::Function(
                                    ChatCompletionMessageToolCall {
                                        id: tc.id.clone(),
                                        function: FunctionCall {
                                            name: tc.name.clone(),
                                            arguments: tc.arguments.clone(),
                                        },
                                    },
                                )
                            })
                            .collect();

                        self.messages.push(ChatCompletionRequestMessage::Assistant(
                            ChatCompletionRequestAssistantMessage {
                                content: if content_buffer.is_empty() {
                                    None
                                } else {
                                    Some(ChatCompletionRequestAssistantMessageContent::Text(
                                        content_buffer.clone(),
                                    ))
                                },
                                tool_calls: Some(assistant_tool_calls),
                                ..Default::default()
                            },
                        ));

                        for tc in &tool_calls {
                            if tc.name.is_empty() {
                                continue;
                            }
                            tracing::info!("tc.name: {}", tc.name);

                            let _ = self.event_tx.send(AgentEvent::ToolCallStart {
                                name: tc.name.clone(),
                                args: tc.arguments.clone(),
                            });

                            let args: Value =
                                serde_json::from_str(&tc.arguments).unwrap_or(json!({}));

                            let start = std::time::Instant::now();

                            let result = match tc.name.as_str() {
                                "add_todo" | "update_todo" | "list_todos" | "remove_todo" => {
                                    self.execute_todo_tool(&tc.name, &args).await
                                }
                                "present_choices" => self.execute_choices_tool(&args).await,
                                _ => {
                                    if let Some((op, path)) =
                                        crate::permission::needs_permission(&tc.name, &args)
                                    {
                                        let decision =
                                            self.request_permission(op, &path, &tc.name).await;
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

                        continue;
                    }

                    if !content_buffer.is_empty() {
                        full_response = content_buffer.clone();
                        self.messages.push(ChatCompletionRequestMessage::Assistant(
                            ChatCompletionRequestAssistantMessage {
                                content: Some(ChatCompletionRequestAssistantMessageContent::Text(
                                    content_buffer,
                                )),
                                ..Default::default()
                            },
                        ));
                    }

                    let _ = self
                        .event_tx
                        .send(AgentEvent::MessageComplete(full_response.clone()));
                    let _ = self.event_tx.send(AgentEvent::Done);
                    return full_response;
                }
                Err(e) => {
                    let error_msg = format!("API error: {}", e);
                    let _ = self.event_tx.send(AgentEvent::Error(error_msg.clone()));
                    let _ = self.event_tx.send(AgentEvent::Done);
                    return error_msg;
                }
            }
        }

        let _ = self
            .event_tx
            .send(AgentEvent::Status("Max iterations reached".into()));
        let _ = self.event_tx.send(AgentEvent::Done);
        full_response
    }

    #[allow(dead_code)]
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
}

fn fix_response_value(value: &mut Value) {
    let Some(choices) = value.get_mut("choices") else {
        return;
    };
    let Some(arr) = choices.as_array_mut() else {
        return;
    };
    for choice in arr {
        if let Some(fr) = choice.get_mut("finish_reason")
            && fr == ""
        {
            *fr = Value::Null;
        }
        let Some(delta) = choice.get_mut("delta") else {
            continue;
        };
        let Some(tool_calls) = delta.get_mut("tool_calls") else {
            continue;
        };
        let Some(arr2) = tool_calls.as_array_mut() else {
            continue;
        };
        for tc in arr2 {
            if let Some(t) = tc.get_mut("type")
                && t == ""
            {
                *t = Value::String("function".to_string());
            }
        }
    }
}

#[derive(Debug)]
struct ToolCallState {
    id: String,
    name: String,
    arguments: String,
}
