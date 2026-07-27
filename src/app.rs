use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
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
use crate::ui::{BG, CYAN, SURFACE, TEXT, YELLOW};

pub enum AgentCommand {
    SendMessage(String),
    Shutdown,
}

struct PendingPermission {
    request: PermissionRequest,
    respond: oneshot::Sender<PermissionDecision>,
}

struct PendingChoice {
    choices: Vec<String>,
    input_buffer: String,
    respond: oneshot::Sender<ChoiceResult>,
}

pub struct App {
    chat: ChatArea,
    input: InputArea,
    #[allow(dead_code)]
    pub mcp: McpManager,
    pub running: bool,
    processing: bool,
    status_message: String,
    tool_count: usize,
    streaming_buffer: String,
    cmd_tx: mpsc::UnboundedSender<AgentCommand>,
    event_rx: mpsc::UnboundedReceiver<AgentEvent>,
    frame_count: u64,
    last_model: String,
    pending_permission: Option<PendingPermission>,
    pending_choice: Option<PendingChoice>,
    permission_rules: Vec<PermissionRule>,
    token_usage_total: u64,
    token_usage_session: u64,
    /// Message queue for user inputs arriving while the agent is processing.
    /// Inputs typed during processing are buffered and sent sequentially once
    /// the current turn is done.
    pending_messages: Vec<String>,
}

impl App {
    pub fn new(config: Config) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let skills = skill::load_all_skills();
        let agents_md = skill::load_agents_md();
        let system_prompt = skill::build_system_prompt(&skills, agents_md.as_deref());

        let last_model = config.model.clone();

        let agent = Agent::new(config, system_prompt, event_tx);
        let mcp = McpManager::new();

        tokio::spawn(run_agent_task(agent, cmd_rx));

        App {
            chat: ChatArea::new(),
            input: InputArea::new(),
            mcp,
            running: true,
            processing: false,
            status_message: "Ready".to_string(),
            tool_count: crate::agent::tools::native_tool_names().len(),
            streaming_buffer: String::new(),
            cmd_tx,
            event_rx,
            frame_count: 0,
            last_model,
            pending_permission: None,
            pending_choice: None,
            permission_rules: Vec::new(),
            token_usage_total: 0,
            token_usage_session: 0,
            pending_messages: Vec::new(),
        }
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) {
        if self.pending_choice.is_some() {
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

        if self.processing {
            if is_scroll {
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
        }

        match key.code {
            KeyCode::Enter => {
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
                            self.status_message =
                                format!("Processing... ({} queued)", self.pending_messages.len());
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
                    self.chat.scroll_up();
                }
            }
            KeyCode::Down => {
                if self.input.get_input().starts_with('/') {
                    self.input.select_suggestion_down();
                } else {
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
                let chat_area_top = 1;
                let row = mouse.row as usize;
                if row > chat_area_top {
                    let relative_row = row - chat_area_top;
                    let scroll = self.chat.scroll_offset() as usize;
                    let click_line = scroll + relative_row;
                    self.chat.toggle_fold_at_line(click_line);
                }
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
        let Some(choice) = self.pending_choice.as_ref() else {
            return false;
        };
        let count = choice.choices.len();

        match key.code {
            KeyCode::Char(d) if d.is_ascii_digit() => {
                let n = d.to_digit(10).unwrap_or(0) as usize;
                if n >= 1 && n <= count {
                    let pending = self.pending_choice.take().unwrap();
                    let _ = pending.respond.send(ChoiceResult {
                        selected_index: n - 1,
                        custom_text: None,
                    });
                    return true;
                }
                if n == 0 && count < 10 {
                    let pending = self.pending_choice.take().unwrap();
                    let _ = pending.respond.send(ChoiceResult {
                        selected_index: 0,
                        custom_text: None,
                    });
                    return true;
                }
                false
            }
            KeyCode::Char('c') | KeyCode::Char('C') => {
                if let Some(choice) = self.pending_choice.as_mut() {
                    match key.modifiers {
                        m if m.is_empty() => {
                            let custom = choice.input_buffer.clone();
                            let pending = self.pending_choice.take().unwrap();
                            let _ = pending.respond.send(ChoiceResult {
                                selected_index: count,
                                custom_text: Some(custom),
                            });
                            return true;
                        }
                        _ => {}
                    }
                }
                false
            }
            KeyCode::Char(ch) => {
                if let Some(choice) = self.pending_choice.as_mut() {
                    choice.input_buffer.push(ch);
                }
                true
            }
            KeyCode::Backspace => {
                if let Some(choice) = self.pending_choice.as_mut() {
                    choice.input_buffer.pop();
                }
                true
            }
            KeyCode::Esc => {
                let pending = self.pending_choice.take().unwrap();
                let _ = pending.respond.send(ChoiceResult {
                    selected_index: 0,
                    custom_text: Some("User cancelled the choice".to_string()),
                });
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
                let tools = crate::agent::tools::native_tool_names();
                let mut msg = format!("**Available Tools ({}):**\n", tools.len());
                for tool in tools {
                    msg.push_str(&format!("- `{}`\n", tool));
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
                    let spinner = spinner_char(self.frame_count);
                    self.status_message = format!(
                        " {} Streaming... {} tokens",
                        spinner, self.token_usage_session
                    );
                }
                AgentEvent::MessageComplete(text) => {
                    self.chat.add_message(ChatMessage::Assistant(text));
                    self.streaming_buffer.clear();
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
                    self.processing = false;
                    self.drain_pending_messages();
                }
                AgentEvent::Error(msg) => {
                    self.chat.add_message(ChatMessage::Error(msg));
                    self.processing = false;
                    self.streaming_buffer.clear();
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
                    self.pending_choice = Some(PendingChoice {
                        choices,
                        input_buffer: String::new(),
                        respond,
                    });
                }
                AgentEvent::TokenUsage { prompt, completion } => {
                    self.token_usage_total += prompt + completion;
                    self.token_usage_session += prompt + completion;
                }
            }
        }
    }

    /// Drain the pending message queue: if there are any user inputs that were
    /// buffered while the agent was processing, send the first one to the agent
    /// now that the current turn is complete. The user will no longer be blocked
    /// from typing and the queued messages flow naturally into the conversation.
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

        terminal.draw(|frame| {
            let area = frame.area();
            frame.render_widget(Clear, area);
            frame.render_widget(Paragraph::new("").style(Style::default().bg(BG)), area);

            let sugg_count = self.input.suggestion_count() as u16;
            let sugg_height = if sugg_count > 0 { sugg_count + 2 } else { 0 };

            let mut constraints = vec![Constraint::Min(5)];
            if sugg_height > 0 {
                constraints.push(Constraint::Length(sugg_height));
            }
            constraints.push(Constraint::Length(3));
            constraints.push(Constraint::Length(1));

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(area);

            let chat_area = chunks[0];
            let input_area = if sugg_height > 0 {
                chunks[2]
            } else {
                chunks[1]
            };
            let status_area = if sugg_height > 0 {
                chunks[3]
            } else {
                chunks[2]
            };

            if !self.streaming_buffer.is_empty() {
                self.chat
                    .render_with_preview(frame, chat_area, &self.streaming_buffer);
            } else {
                self.chat.render(frame, chat_area);
            }

            if sugg_height > 0 {
                self.input.render_suggestions(frame, chunks[1]);
            }

            self.input.render(frame, input_area);

            if let Some(ref choice) = self.pending_choice {
                let options: Vec<String> = choice
                    .choices
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("  {}. {}", i + 1, c))
                    .collect();
                let custom_hint = if choice.input_buffer.is_empty() {
                    " or type your custom approach below"
                } else {
                    ""
                };
                let choice_text = format!(
                    "【Choice】Pick an option (1-{}){}: {}",
                    choice.choices.len(),
                    custom_hint,
                    choice.input_buffer
                );
                let height = (choice.choices.len() as u16 + 3).min(12);
                let choice_area = Rect {
                    x: status_area.x,
                    y: status_area.y.saturating_sub(height.saturating_sub(1)),
                    width: status_area.width,
                    height,
                };
                let mut choice_lines = Vec::new();
                for opt in &options {
                    choice_lines.push(Line::from(Span::styled(
                        opt.clone(),
                        Style::default().fg(CYAN),
                    )));
                }
                choice_lines.push(Line::from(""));
                choice_lines.push(Line::from(Span::styled(
                    choice_text,
                    Style::default()
                        .fg(YELLOW)
                        .bg(SURFACE)
                        .add_modifier(Modifier::BOLD),
                )));
                let widget = Paragraph::new(choice_lines).style(Style::default().bg(BG));
                frame.render_widget(widget, choice_area);
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
                    format!(" {} Processing", spinner)
                } else {
                    String::new()
                };

                let status_text = format!(
                    " mp_agent | {} tools | {} {}",
                    self.tool_count, self.status_message, symbol
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

async fn run_agent_task(mut agent: Agent, mut cmd_rx: mpsc::UnboundedReceiver<AgentCommand>) {
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            AgentCommand::SendMessage(msg) => {
                agent.send_message(&msg).await;
            }
            AgentCommand::Shutdown => {
                break;
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
