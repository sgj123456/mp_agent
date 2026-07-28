use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use tokio::sync::{mpsc, oneshot};

use crate::agent::skill;
use crate::agent::{Agent, AgentEvent, ChoiceResult};
use crate::config::Config;
use crate::mcp::McpManager;
use crate::permission::{PermissionDecision, PermissionRequest, PermissionRule, match_rule};
use crate::ui::chat::{ChatArea, ChatMessage};
use crate::ui::input::{InputArea, SuggestionItem};
use crate::ui::layout::{compute_choice_panel, compute_layout, compute_suggestion_panel};
use crate::ui::{BG, CYAN, SURFACE, TEXT, TEXT_DIM, YELLOW};

/// Represents a selected text range within the chat area.
/// `start_line` and `end_line` are absolute line indices (including all messages).
#[derive(Debug, Clone, Copy)]
pub struct SelectionRange {
    pub start_line: usize,
    pub end_line: usize,
}

pub enum AgentCommand {
    SendMessage(String),
    ConnectMcp,
    Shutdown,
    ClearHistory,
}

struct PendingPermission {
    request: PermissionRequest,
    respond: oneshot::Sender<PermissionDecision>,
}

struct PendingChoice {
    choices: Vec<String>,
    selected: usize,
    input_buffer: String,
    respond: oneshot::Sender<ChoiceResult>,
}

pub struct App {
    chat: ChatArea,
    input: InputArea,
    pub running: bool,
    processing: bool,
    status_message: String,
    tool_count: usize,
    mcp_tool_names: Vec<String>,
    streaming_buffer: String,
    cmd_tx: mpsc::UnboundedSender<AgentCommand>,
    event_rx: mpsc::UnboundedReceiver<AgentEvent>,
    frame_count: u64,
    last_model: String,
    pending_permission: Option<PendingPermission>,
    pending_choice: Vec<PendingChoice>,
    permission_rules: Vec<PermissionRule>,
    token_usage_total: u64,
    token_usage_session: u64,
    /// Prompt tokens for current session (context usage).
    prompt_tokens_session: u64,
    /// Number of tokens streamed in the current response (resets per turn).
    streamed_tokens: u64,
    /// Model's maximum context window (in tokens).
    context_limit: u64,
    /// Message queue for user inputs arriving while the agent is processing.
    /// Inputs typed during processing are buffered and sent sequentially once
    /// the current turn is done.
    pending_messages: Vec<String>,
    /// Currently selected text range in the chat area (None = no selection).
    selection: Option<SelectionRange>,
    /// Mouse button state: true if left button is currently held down.
    left_button_down: bool,
    /// Cached plain-text lines of the chat content, used for extracting
    /// selected text during drag-to-select copying.
    chat_plain_lines: Vec<String>,
    /// Persistent clipboard handle so clipboard managers on Linux have time
    /// to read the contents (avoid "dropped very quickly" warnings).
    clipboard: Option<arboard::Clipboard>,
}

impl App {
    pub async fn new(config: Config) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let skills = skill::load_all_skills();
        let agents_md = skill::load_agents_md();
        let system_prompt = skill::build_system_prompt(&skills, agents_md.as_deref());

        let last_model = config.model.clone();
        let context_limit = model_context_limit(&last_model);

        // Initialize MCP manager from configuration file if present
        let mcp_manager = if std::path::Path::new("mcp_servers.json").exists() {
            match McpManager::from_config("mcp_servers.json") {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Failed to load MCP config: {}", e);
                    McpManager::new()
                }
            }
        } else {
            McpManager::new()
        };

        // Determine if there are any configured MCP servers
        let has_mcp_servers = mcp_manager.config_has_servers();

        let agent = Agent::new(config, system_prompt, event_tx, mcp_manager);
        tokio::spawn(run_agent_task(agent, cmd_rx));

        // Queue background MCP connection if servers are configured
        if has_mcp_servers {
            let _ = cmd_tx.send(AgentCommand::ConnectMcp);
        }

        App {
            chat: ChatArea::new(),
            input: InputArea::new(),
            running: true,
            processing: false,
            status_message: if has_mcp_servers {
                "Connecting MCP servers...".to_string()
            } else {
                "Ready".to_string()
            },
            tool_count: crate::agent::tools::native_tool_names().len(),
            mcp_tool_names: Vec::new(),
            streaming_buffer: String::new(),
            cmd_tx,
            event_rx,
            frame_count: 0,
            last_model,
            pending_permission: None,
            pending_choice: Vec::new(),
            permission_rules: Vec::new(),
            token_usage_total: 0,
            token_usage_session: 0,
            prompt_tokens_session: 0,
            streamed_tokens: 0,
            context_limit,
            pending_messages: Vec::new(),
            selection: None,
            left_button_down: false,
            chat_plain_lines: Vec::new(),
            clipboard: arboard::Clipboard::new().ok(),
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        if !self.pending_choice.is_empty() {
            let handled = self.handle_choice_key(&key);
            if handled {
                return;
            }
        }

        if self.pending_permission.is_some() {
            let consume = matches!(
                key.code,
                KeyCode::Char('y')
                    | KeyCode::Char('Y')
                    | KeyCode::Char('n')
                    | KeyCode::Char('N')
                    | KeyCode::Char('a')
                    | KeyCode::Char('A')
                    | KeyCode::Char('d')
                    | KeyCode::Char('D')
                    | KeyCode::Esc
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
            return;
        }

        if key.code == KeyCode::Esc {
            if self.processing {
                self.processing = false;
                self.streaming_buffer.clear();
                self.streamed_tokens = 0;
                self.pending_messages.clear();
                self.status_message = "Cancelled".to_string();
                return;
            }
            self.running = false;
            let _ = self.cmd_tx.send(AgentCommand::Shutdown);
            return;
        }

        let is_scroll = matches!(
            key.code,
            KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown
        );

        if self.processing && is_scroll {
            match key.code {
                KeyCode::Up => self.chat.scroll_up(),
                KeyCode::Down => self.chat.scroll_down(),
                KeyCode::PageUp => self.chat.scroll_page_up(10),
                KeyCode::PageDown => self.chat.scroll_page_down(10),
                _ => {}
            }
        }
        // Allow the user to type into the input area while the agent is
        // processing; the message will be queued when Enter is pressed.
        // Don't return early — fall through to the key handling below.

        match key.code {
            KeyCode::Enter => {
                // Alt+Enter inserts a literal newline instead of submitting.
                // Shift+Enter requires Kitty keyboard protocol support which
                // many terminals lack; Alt is universally reliable.
                if key.modifiers.contains(KeyModifiers::ALT) {
                    self.input.insert_char('\n');
                } else {
                    let input = self.input.accept_selected_suggestion();
                    let input = if let Some(cmd) = input {
                        cmd
                    } else {
                        self.input.get_input()
                    };
                    if !input.trim().is_empty() {
                        if input.starts_with('/') {
                            self.handle_slash_command(&input);
                        } else {
                            if self.processing {
                                // Agent is still processing; buffer the message for later.
                                self.pending_messages.push(input.clone());
                                self.input.clear();
                                self.status_message = format!(
                                    "Processing... ({} queued)",
                                    self.pending_messages.len()
                                );
                            } else {
                                self.processing = true;
                                self.streaming_buffer.clear();
                                self.status_message = "Processing...".to_string();
                                self.chat.add_message(ChatMessage::User(input.clone()));
                                self.input.clear();
                                let _ = self.cmd_tx.send(AgentCommand::SendMessage(input));
                            }
                        }
                    } else {
                        self.input.clear();
                    }
                }
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'c' => {
                            self.running = false;
                            let _ = self.cmd_tx.send(AgentCommand::Shutdown);
                        }
                        'a' => self.input.move_cursor_home(),
                        'e' => self.input.move_cursor_end(),
                        'u' => self.input.clear(),
                        'd' => {
                            self.chat
                                .add_message(ChatMessage::System("Chat cleared".to_string()));
                        }
                        'l' => {
                            self.chat = ChatArea::new();
                        }
                        _ => {}
                    }
                } else if key.modifiers.contains(KeyModifiers::ALT) {
                } else {
                    self.input.insert_char(c);
                }
            }
            KeyCode::Backspace => self.input.delete_char(),
            KeyCode::Left => self.input.move_cursor_left(),
            KeyCode::Right => self.input.move_cursor_right(),
            KeyCode::Home => self.input.move_cursor_home(),
            KeyCode::End => self.input.move_cursor_end(),
            KeyCode::Up => {
                if self.input.get_input().starts_with('/') {
                    self.input.select_suggestion_up();
                } else {
                    self.input.history_up();
                }
            }
            KeyCode::Down => {
                if self.input.get_input().starts_with('/') {
                    self.input.select_suggestion_down();
                } else if !self.input.history_down() {
                    // Not in history navigation; fall back to chat scroll.
                    self.chat.scroll_down();
                }
            }
            KeyCode::PageUp => self.chat.scroll_page_up(10),
            KeyCode::PageDown => self.chat.scroll_page_down(10),
            KeyCode::Tab => self.input.tab_complete(),
            _ => {}
        }
    }

    pub fn handle_mouse_event(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                self.left_button_down = true;
                let row = mouse.row as usize;
                let scroll = self.chat.scroll_offset() as usize;
                let click_line = scroll + row;
                self.chat.toggle_fold_at_line(click_line);
                self.selection = Some(SelectionRange {
                    start_line: click_line,
                    end_line: click_line,
                });
            }
            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                if self.left_button_down {
                    let row = mouse.row as usize;
                    let scroll = self.chat.scroll_offset() as usize;
                    let click_line = scroll + row;
                    if let Some(ref mut sel) = self.selection {
                        sel.end_line = click_line;
                    }
                }
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                self.left_button_down = false;
                if let Some(sel) = self.selection {
                    let text = extract_selection_text(self, sel);
                    if !text.is_empty() {
                        if let Some(ref mut cb) = self.clipboard {
                            if let Err(e) = cb.set_text(text) {
                                tracing::error!("Clipboard error: {}", e);
                                self.status_message = format!("Clipboard error: {}", e);
                            }
                        } else {
                            self.status_message = "Clipboard not available".to_string();
                        }
                    }
                }
                self.selection = None;
            }
            MouseEventKind::ScrollUp => {
                self.chat.scroll_up();
            }
            MouseEventKind::ScrollDown => {
                self.chat.scroll_down();
            }
            _ => {}
        }
    }

    fn handle_choice_key(&mut self, key: &KeyEvent) -> bool {
        // Work on the topmost pending choice (most recent).
        let count = self.pending_choice.len();
        if count == 0 {
            return false;
        }
        let current = self.pending_choice.last_mut().unwrap();
        let n_choices = current.choices.len();

        match key.code {
            KeyCode::Enter => {
                let selected = current.selected;
                let custom = if current.input_buffer.is_empty() {
                    None
                } else {
                    Some(current.input_buffer.clone())
                };
                let pending = self.pending_choice.remove(count - 1);
                let _ = pending.respond.send(ChoiceResult {
                    selected_index: selected,
                    custom_text: custom,
                });
                true
            }
            KeyCode::Esc => {
                let pending = self.pending_choice.remove(count - 1);
                let _ = pending.respond.send(ChoiceResult {
                    selected_index: 0,
                    custom_text: Some("User cancelled the choice".to_string()),
                });
                true
            }
            KeyCode::Char(d) if d.is_ascii_digit() => {
                let n = d.to_digit(10).unwrap_or(0) as usize;
                if n >= 1 && n <= n_choices {
                    let pending = self.pending_choice.remove(count - 1);
                    let _ = pending.respond.send(ChoiceResult {
                        selected_index: n - 1,
                        custom_text: None,
                    });
                    return true;
                }
                if n == 0 && n_choices < 10 {
                    let pending = self.pending_choice.remove(count - 1);
                    let _ = pending.respond.send(ChoiceResult {
                        selected_index: 0,
                        custom_text: None,
                    });
                    return true;
                }
                false
            }
            KeyCode::Up => {
                if current.selected > 0 {
                    current.selected -= 1;
                    current.input_buffer.clear();
                }
                true
            }
            KeyCode::Down => {
                if current.selected < n_choices - 1 {
                    current.selected += 1;
                    current.input_buffer.clear();
                }
                true
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if key.modifiers.is_empty() {
                    // Press 'c' to submit a custom approach.
                    let custom = current.input_buffer.clone();
                    let selected = current.selected;
                    let pending = self.pending_choice.remove(count - 1);
                    let _ = pending.respond.send(ChoiceResult {
                        selected_index: selected,
                        custom_text: Some(custom),
                    });
                    return true;
                }
                false
            }
            KeyCode::Char(ch) => {
                if key.modifiers.is_empty() {
                    current.input_buffer.push(ch);
                }
                true
            }
            KeyCode::Backspace => {
                current.input_buffer.pop();
                true
            }
            _ => true,
        }
    }

    fn handle_slash_command(&mut self, input: &str) {
        let parts: Vec<&str> = input.trim().splitn(2, ' ').collect();
        let cmd = parts[0].to_lowercase();
        let args = parts.get(1).copied().unwrap_or("");

        match cmd.as_str() {
            "/help" => {
                let help_text = String::from(
                    "## Available Commands\n\n\
                     **Slash Commands:**\n\
                     - `/help` - Show this help\n\
                     - `/clear` - Clear chat history\n\
                     - `/model` - Show current model\n\
                     - `/tools` - List available tools\n\
                     - `/exit` - Exit the application\n\n\
                     **Keyboard Shortcuts:**\n\
                     - `Esc` - Cancel processing / Exit\n\
                     - `Ctrl+C` - Force exit\n\
                     - `Ctrl+A` - Go to start of input\n\
                     - `Ctrl+E` - Go to end of input\n\
                     - `Ctrl+U` - Clear input\n\
                     - `Ctrl+L` - Clear chat\n\
                     - `Alt+Enter` - Insert newline in input\n\
                     - `Tab` - Auto-complete slash commands\n\
                     - `↑/↓` - Input history / Scroll chat\n\n\
                     **Streaming Input:**\n\
                     You can type and press Enter while the agent is thinking. Your\n\
                     messages are queued and sent automatically in order once the\n\
                     current response is complete. Press Esc to cancel processing\n\
                     and clear any queued messages.\n\n\
                     **Markdown Support:**\n\
                     Assistant messages support **bold**, *italic*, `code`, \
                     ```code blocks```, headers, lists, and blockquotes.",
                );
                self.chat.add_message(ChatMessage::System(help_text));
                self.input.clear();
            }
            "/clear" => {
                self.chat = ChatArea::new();
                self.chat
                    .add_message(ChatMessage::System("Chat cleared".to_string()));
                self.input.clear();
                let _ = self.cmd_tx.send(AgentCommand::ClearHistory);
            }
            "/model" => {
                let msg = if args.is_empty() {
                    format!("Current model: `{}`", self.last_model)
                } else {
                    format!(
                        "Model change not supported at runtime. Current model: `{}`",
                        self.last_model
                    )
                };
                self.chat.add_message(ChatMessage::System(msg));
                self.input.clear();
            }
            "/tools" => {
                let native_tools = crate::agent::tools::native_tool_names();
                let total = native_tools.len() + self.mcp_tool_names.len();
                let mut msg = format!("**Available Tools ({} total):**\n", total);
                msg.push_str("\n**Native tools:**\n");
                for tool in native_tools {
                    msg.push_str(&format!("- `{}`\n", tool));
                }
                if !self.mcp_tool_names.is_empty() {
                    msg.push_str("\n**MCP tools:**\n");
                    for tool in &self.mcp_tool_names {
                        msg.push_str(&format!("- `{}` (MCP)\n", tool));
                    }
                }
                self.chat.add_message(ChatMessage::System(msg));
                self.input.clear();
            }
            "/exit" => {
                self.running = false;
                let _ = self.cmd_tx.send(AgentCommand::Shutdown);
            }
            "/history" => {
                self.chat.add_message(ChatMessage::System(
                    "Type ↑/↓ to navigate input history".to_string(),
                ));
                self.input.clear();
            }
            _ => {
                self.chat.add_message(ChatMessage::Error(format!(
                    "Unknown command: {}. Type /help for available commands.",
                    cmd
                )));
                self.input.clear();
            }
        }
    }

    /// Refresh the input area's context suggestions based on the current chat history.
    fn update_context_suggestions(&mut self) {
        let suggestions = extract_context_suggestions(&self.chat);
        self.input.set_context_suggestions(suggestions);
    }

    pub fn process_agent_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AgentEvent::Token(token) => {
                    self.streaming_buffer.push_str(&token);
                    self.streamed_tokens += 1;
                    self.status_message = format!("Streaming... {} tokens", self.streamed_tokens);
                }
                AgentEvent::MessageComplete(text) => {
                    self.chat.add_message(ChatMessage::Assistant(text));
                    self.streaming_buffer.clear();
                    self.streamed_tokens = 0;
                    self.processing = false;
                    self.update_context_suggestions();
                    self.drain_pending_messages();
                }
                AgentEvent::ToolCallStart { name, args } => {
                    self.chat.add_message(ChatMessage::ToolCall { name, args });
                }
                AgentEvent::ToolCallResult { name, result } => {
                    self.chat
                        .add_message(ChatMessage::ToolResult { name, result });
                }
                AgentEvent::Done => {
                    if self.processing && !self.streaming_buffer.is_empty() {
                        self.chat
                            .add_message(ChatMessage::Assistant(self.streaming_buffer.clone()));
                        self.streaming_buffer.clear();
                    }
                    self.streamed_tokens = 0;
                    self.processing = false;
                    self.drain_pending_messages();
                }
                AgentEvent::Error(msg) => {
                    self.chat.add_message(ChatMessage::Error(msg));
                    self.processing = false;
                    self.streaming_buffer.clear();
                    self.streamed_tokens = 0;
                    self.status_message = "Error".to_string();
                }
                AgentEvent::Status(msg) => {
                    if msg == "Thinking..." {
                        self.streaming_buffer.clear();
                    }
                    self.status_message = msg;
                }
                AgentEvent::PermissionRequired { request, respond } => {
                    if let Some(decision) =
                        match_rule(&self.permission_rules, &request.op, &request.path)
                    {
                        let _ = respond.send(decision);
                    } else {
                        self.pending_permission = Some(PendingPermission { request, respond });
                    }
                }
                AgentEvent::ChoiceRequired { choices, respond } => {
                    self.pending_choice.push(PendingChoice {
                        choices,
                        selected: 0,
                        input_buffer: String::new(),
                        respond,
                    });
                }
                AgentEvent::TokenUsage { prompt, completion } => {
                    self.token_usage_total += prompt + completion;
                    self.token_usage_session += prompt + completion;
                    self.prompt_tokens_session = prompt;
                    self.streamed_tokens = 0;
                }
                AgentEvent::McpServerConnected {
                    server_name,
                    prefixed_tools,
                } => {
                    for tool_name in prefixed_tools {
                        if !self.mcp_tool_names.contains(&tool_name) {
                            self.mcp_tool_names.push(tool_name);
                        }
                    }
                    self.tool_count =
                        crate::agent::tools::native_tool_names().len() + self.mcp_tool_names.len();
                    self.status_message = format!("MCP '{}' connected", server_name);
                }
                AgentEvent::McpServerFailed { server_name, error } => {
                    self.chat.add_message(ChatMessage::Error(format!(
                        "MCP server '{}' failed: {}",
                        server_name, error
                    )));
                }
                AgentEvent::McpConnectionsDone => {
                    if self.status_message.contains("MCP") {
                        self.status_message = "Ready".to_string();
                    }
                }
            }
        }
    }

    /// Drain the pending message queue: if there are any user inputs that were
    /// buffered while the agent was processing, send the first one to the agent
    /// now that the current turn is complete. The message is added to the chat
    /// as a normal `User` message at this point.
    fn drain_pending_messages(&mut self) {
        if self.pending_messages.is_empty() {
            self.status_message = "Ready".to_string();
            return;
        }
        let next = self.pending_messages.remove(0);
        self.processing = true;
        self.streaming_buffer.clear();
        let queued = self.pending_messages.len();
        self.status_message = if queued > 0 {
            format!("Processing... ({} queued)", queued)
        } else {
            "Processing...".to_string()
        };
        self.chat.add_message(ChatMessage::User(next.clone()));
        let _ = self.cmd_tx.send(AgentCommand::SendMessage(next));
    }

    pub fn draw(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    ) -> color_eyre::Result<()> {
        self.frame_count += 1;
        let pending: Vec<String> = if !self.pending_messages.is_empty() {
            self.pending_messages.clone()
        } else {
            Vec::new()
        };
        let cwd = std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(Clear, area);
            frame.render_widget(Paragraph::new("").style(Style::default().bg(BG)), area);

            let input_content_width = area.width.saturating_sub(2);
            let input_wrapped = self.input.wrapped_height(input_content_width);
            let input_height = input_wrapped.saturating_add(2).clamp(3, 10);

            let layout = compute_layout(area, input_height);
            let chat_area = layout.chat;
            let input_area = layout.input;
            let status_area = layout.status;

            if !self.streaming_buffer.is_empty() {
                self.chat
                    .render_with_preview(frame, chat_area, &self.streaming_buffer, &pending);
            } else {
                self.chat.render(frame, chat_area, &pending);
            }

            // Cache the plain-text lines of the chat content so that the
            // mouse-selection handler can extract the selected text.
            self.chat_plain_lines = self.chat.plain_text_lines();

            if let Some(sel) = self.selection {
                let start = sel.start_line.min(sel.end_line);
                let end = sel.start_line.max(sel.end_line);
                let scroll = self.chat.scroll_offset() as usize;
                let visible_start = start.saturating_sub(scroll);
                let visible_end = end.saturating_sub(scroll);
                if visible_start < chat_area.height as usize && chat_area.height > 0 {
                    let row_start = (chat_area.y + visible_start as u16)
                        .min(chat_area.y + chat_area.height.saturating_sub(1));
                    let row_end = (chat_area.y + visible_end as u16)
                        .min(chat_area.y + chat_area.height.saturating_sub(1));
                    let highlight_height = row_end.saturating_sub(row_start) + 1;
                    if highlight_height > 0 {
                        let highlight_area = Rect {
                            x: chat_area.x + 2,
                            y: row_start,
                            width: chat_area.width.saturating_sub(4),
                            height: highlight_height,
                        };
                        let highlight = Paragraph::new("")
                            .style(Style::default().bg(CYAN).add_modifier(Modifier::DIM));
                        frame.render_widget(highlight, highlight_area);
                    }
                }
            }

            self.input.render(frame, input_area);

            // Render the suggestion overlay on top of the input area if there
            // are any matching slash commands or context suggestions. Only
            // shown when no pending choice panel is active (the two overlays
            // would conflict).
            //
            // The panel is rendered as an overlay on the upper portion of the
            // input area (anchored to the top of the input box and floating
            // downward) so the command / tab-completion panel is actually
            // visible — the alternative (rendering below the input box) would
            // place the panel inside the status bar area which is only 1 line
            // tall. Using Clear ensures the panel is not obscured by the chat
            // content behind it.
            if self.pending_choice.is_empty() && self.input.get_input().starts_with('/') {
                let matches = self.input.matching_commands();
                if let Some(suggestion_area) = compute_suggestion_panel(input_area, matches.len()) {
                    frame.render_widget(Clear, suggestion_area);
                    self.input.render_suggestions(frame, suggestion_area);
                }
            }

            if !self.pending_choice.is_empty() {
                // Render a dedicated choice panel that overlays the chat area.
                // The topmost pending choice is shown; if there are multiple,
                // a small indicator shows the queue depth.
                let current = self.pending_choice.last().unwrap();
                let n = current.choices.len();
                let selected = current.selected;
                let custom_buffer = &current.input_buffer;

                let panel_area = compute_choice_panel(chat_area, n);

                let mut lines = Vec::new();
                // Title bar
                let title = if self.pending_choice.len() > 1 {
                    format!(
                        "▌ Choice ({} pending) — Pick an approach:",
                        self.pending_choice.len()
                    )
                } else {
                    "▌ Pick an approach:".to_string()
                };
                lines.push(Line::from(Span::styled(
                    title,
                    Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));

                // Option list with selection highlight
                for (i, choice) in current.choices.iter().enumerate() {
                    let is_selected = i == selected;
                    let prefix = if is_selected { "  ▸ " } else { "    " };
                    let num = i + 1;
                    let style = if is_selected {
                        Style::default()
                            .fg(BG)
                            .bg(CYAN)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(TEXT)
                    };
                    lines.push(Line::from(Span::styled(
                        format!("{}{}. {}", prefix, num, choice),
                        style,
                    )));
                }
                lines.push(Line::from(""));

                // Custom input line
                let custom_prompt = if custom_buffer.is_empty() {
                    "  Type custom approach or press [c] to confirm custom…"
                } else {
                    &format!("  Custom: {}", custom_buffer)
                };
                lines.push(Line::from(Span::styled(
                    custom_prompt,
                    Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
                )));

                // Hint line
                let hint = format!(
                    "  [↑↓] navigate  [1-{}] select  [Enter] confirm  [Esc] cancel  [c] custom",
                    n
                );
                lines.push(Line::from(Span::styled(
                    hint,
                    Style::default().fg(TEXT_DIM),
                )));

                let widget = Paragraph::new(lines)
                    .style(Style::default().bg(SURFACE).fg(TEXT))
                    .block(
                        ratatui::widgets::Block::default()
                            .borders(ratatui::widgets::Borders::ALL)
                            .title(" Choice ")
                            .border_style(Style::default().fg(CYAN)),
                    );
                frame.render_widget(widget, panel_area);
            } else if let Some(ref pending) = self.pending_permission {
                let op = crate::permission::op_label(&pending.request.op);
                let path = &pending.request.path;
                let desc = &pending.request.description;
                let prompt = format!(
                    " 【Permission】{} {} | {}  [y]es [a]lways [n]o [d]eny [Esc]",
                    op,
                    crate::permission::truncate(path, 60),
                    desc
                );
                let status = Paragraph::new(Line::from(Span::styled(
                    prompt,
                    Style::default()
                        .fg(YELLOW)
                        .bg(SURFACE)
                        .add_modifier(Modifier::BOLD),
                )));
                frame.render_widget(status, status_area);
            } else {
                let symbol = if self.processing {
                    let spinner = spinner_char(self.frame_count);
                    format!("{} ", spinner)
                } else {
                    String::new()
                };

                let ctx_text = if self.context_limit > 0 {
                    let pct = self.prompt_tokens_session as f64 / self.context_limit as f64 * 100.0;
                    format!(
                        " ctx {}",
                        format_token_count(self.prompt_tokens_session, self.context_limit, pct)
                    )
                } else {
                    String::new()
                };
                let cwd_short = if cwd.len() > 30 {
                    format!("..{}", &cwd[cwd.len().saturating_sub(28)..])
                } else {
                    cwd.clone()
                };
                let status_text = format!(
                    "▌ {} │ {} │ {} tools{} │  {}{}",
                    self.last_model, cwd_short, self.tool_count, ctx_text, symbol, self.status_message
                );
                let status = Paragraph::new(Line::from(Span::styled(
                    status_text.trim(),
                    Style::default().fg(TEXT).bg(SURFACE),
                )));
                frame.render_widget(status, status_area);
            }
        })?;
        Ok(())
    }
}

fn spinner_char(frame: u64) -> char {
    match frame % 10 {
        0 => '⣾',
        1 => '⣽',
        2 => '⣻',
        3 => '⢿',
        4 => '⡿',
        5 => '⣟',
        6 => '⣯',
        7 => '⣷',
        _ => '⣾',
    }
}

fn dirname(path: &str) -> String {
    std::path::Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn format_token_count(used: u64, limit: u64, pct: f64) -> String {
    let used_fmt = if used >= 1_000_000 {
        format!("{:.1}M", used as f64 / 1_000_000.0)
    } else if used >= 1_000 {
        format!("{:.1}K", used as f64 / 1_000.0)
    } else {
        format!("{}", used)
    };
    let limit_fmt = if limit >= 1_000_000 {
        format!("{:.0}M", limit as f64 / 1_000_000.0)
    } else if limit >= 1_000 {
        format!("{:.0}K", limit as f64 / 1_000.0)
    } else {
        format!("{}", limit)
    };
    if pct < 0.1 {
        format!("{}/{}", used_fmt, limit_fmt)
    } else {
        format!("{}/{} ({:.0}%)", used_fmt, limit_fmt, pct)
    }
}

/// Return the known context window (in tokens) for a given model name.
fn model_context_limit(model: &str) -> u64 {
    let lower = model.to_lowercase();
    if lower.contains("gpt-4o") || lower.contains("gpt-4-turbo") {
        128_000
    } else if lower.contains("gpt-4") {
        8_192
    } else if lower.contains("gpt-3.5") || lower.contains("gpt-35") {
        16_385
    } else if lower.contains("claude-3") || lower.contains("claude") {
        200_000
    } else if lower.contains("deepseek") {
        64_000
    } else if lower.contains("qwen") {
        if lower.contains("qwen2.5") || lower.contains("qwen2_5") {
            128_000
        } else {
            32_000
        }
    } else if lower.contains("gemini") {
        1_000_000
    } else if lower.contains("llama-3") {
        8_192
    } else if lower.contains("llama-2") {
        4_096
    } else if lower.contains("mistral") || lower.contains("mixtral") {
        32_000
    } else if lower.contains("codestral") {
        256_000
    } else {
        128_000
    }
}

/// Extract the plain text covered by the selection range from the cached
/// chat plain-text lines. Returns the selected text as a single string.
fn extract_selection_text(app: &App, range: SelectionRange) -> String {
    if app.chat_plain_lines.is_empty() {
        return String::new();
    }
    let start = range.start_line.min(range.end_line);
    let end = range.start_line.max(range.end_line);
    if start >= app.chat_plain_lines.len() {
        return String::new();
    }
    let end = end.min(app.chat_plain_lines.len() - 1);
    let selected: Vec<&str> = app.chat_plain_lines[start..=end]
        .iter()
        .map(|s| s.as_str())
        .collect();
    selected.join("\n")
}

async fn run_agent_task(mut agent: Agent, mut cmd_rx: mpsc::UnboundedReceiver<AgentCommand>) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            AgentCommand::SendMessage(msg) => {
                agent.send_message(&msg).await;
            }
            AgentCommand::ConnectMcp => {
                agent.connect_mcp_servers().await;
            }
            AgentCommand::Shutdown => {
                break;
            }
            AgentCommand::ClearHistory => {
                agent.clear_history();
            }
        }
    }
}

/// Extract context suggestions from the current chat messages.
/// Scans user, assistant, tool-call and tool-result messages for file paths,
/// shell commands, tool names and todo descriptions that the user may want to
/// reuse, and returns a de-duplicated list of SuggestionItem.
fn extract_context_suggestions(chat: &ChatArea) -> Vec<SuggestionItem> {
    use std::collections::HashSet;

    let mut seen = HashSet::new();
    let mut suggestions = Vec::new();

    for (msg, _time) in chat.messages() {
        match msg {
            ChatMessage::User(text) => {
                for candidate in extract_candidates(text) {
                    if seen.insert(candidate.clone()) {
                        suggestions.push(SuggestionItem::Context(candidate));
                    }
                }
            }
            ChatMessage::Assistant(text) => {
                for candidate in extract_candidates(text) {
                    if seen.insert(candidate.clone()) {
                        suggestions.push(SuggestionItem::Context(candidate));
                    }
                }
            }
            ChatMessage::ToolCall { name, args } => {
                if seen.insert(name.clone()) {
                    suggestions.push(SuggestionItem::Context(format!("/tools {}", name)));
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
                    for s in extract_json_strings(&val) {
                        if seen.insert(s.clone()) {
                            suggestions.push(SuggestionItem::Context(s));
                        }
                    }
                }
            }
            ChatMessage::ToolResult { name, result } => {
                if seen.insert(name.clone()) {
                    suggestions.push(SuggestionItem::Context(format!("/tools {}", name)));
                }
                for candidate in extract_candidates(result) {
                    if seen.insert(candidate.clone()) {
                        suggestions.push(SuggestionItem::Context(candidate));
                    }
                }
            }
            ChatMessage::Error(text) | ChatMessage::System(text) => {
                for candidate in extract_candidates(text) {
                    if seen.insert(candidate.clone()) {
                        suggestions.push(SuggestionItem::Context(candidate));
                    }
                }
            }
        }
    }

    let max = 20;
    if suggestions.len() > max {
        suggestions.truncate(max);
    }

    suggestions
}

/// Heuristic candidate extraction from free-form text.
/// Looks for file-path-like strings, quoted commands, and other reusable tokens.
fn extract_candidates(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();

    for word in text.split_whitespace() {
        let trimmed = word.trim_matches(|c: char| c.is_ascii_punctuation());
        if trimmed.is_empty() || trimmed.len() < 3 {
            continue;
        }

        if trimmed.contains('/') || (trimmed.contains('.') && trimmed.len() > 4) {
            candidates.push(trimmed.to_string());
        }

        if trimmed.starts_with("cargo ")
            || trimmed.starts_with("git ")
            || trimmed.starts_with("ls ")
            || trimmed.starts_with("cat ")
        {
            candidates.push(trimmed.to_string());
        }
    }

    let mut in_quote = false;
    let mut quote_char = '"';
    let mut current = String::new();
    for ch in text.chars() {
        if !in_quote && (ch == '"' || ch == '\'') {
            in_quote = true;
            quote_char = ch;
            current.clear();
        } else if in_quote && ch == quote_char {
            in_quote = false;
            if current.len() >= 3 {
                candidates.push(current.clone());
            }
        } else if in_quote {
            current.push(ch);
        }
    }

    candidates
}

/// Walk a JSON value and collect every string leaf into a vector.
fn extract_json_strings(value: &serde_json::Value) -> Vec<String> {
    let mut result = Vec::new();
    match value {
        serde_json::Value::String(s) => {
            if s.len() >= 3 {
                result.push(s.clone());
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                result.extend(extract_json_strings(item));
            }
        }
        serde_json::Value::Object(obj) => {
            for (_, v) in obj {
                result.extend(extract_json_strings(v));
            }
        }
        _ => {}
    }
    result
}
