use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use unicode_width::UnicodeWidthChar;

use super::{BG, CYAN, RED, TEXT, TEXT_DIM, YELLOW};

pub const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show help information"),
    ("/clear", "Clear chat history"),
    ("/model", "Show or change model"),
    ("/tools", "List available tools"),
    ("/exit", "Exit the application"),
    ("/history", "Show command history"),
];

#[derive(Debug, Clone)]
pub enum SuggestionItem {
    SlashCommand(&'static str, &'static str),
    Context(String),
}

impl SuggestionItem {
    pub fn label(&self) -> String {
        match self {
            SuggestionItem::SlashCommand(cmd, _) => cmd.to_string(),
            SuggestionItem::Context(s) => s.clone(),
        }
    }

    pub fn description(&self) -> String {
        match self {
            SuggestionItem::SlashCommand(_, desc) => desc.to_string(),
            SuggestionItem::Context(_) => String::new(),
        }
    }

    pub fn is_context(&self) -> bool {
        matches!(self, SuggestionItem::Context(_))
    }
}

pub struct InputArea {
    buffer: String,
    cursor_pos: usize,
    history: Vec<String>,
    history_pos: Option<usize>,
    tab_suggestion: Option<String>,
    /// Index of the currently selected suggestion in the filtered list.
    suggestion_cursor: Option<usize>,
    /// Dynamic suggestions extracted from chat context (file paths, commands,
    /// todo descriptions, etc.) used for tab-completion when the input is
    /// non-empty and does not start with '/'.
    context_suggestions: Vec<SuggestionItem>,
}

impl InputArea {
    pub fn new() -> Self {
        InputArea {
            buffer: String::new(),
            cursor_pos: 0,
            history: Vec::new(),
            history_pos: None,
            tab_suggestion: None,
            suggestion_cursor: None,
            context_suggestions: Vec::new(),
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
        self.tab_suggestion = None;
    }

    pub fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            let prev = self.buffer[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.buffer.drain(prev..self.cursor_pos);
            self.cursor_pos = prev;
        }
        self.tab_suggestion = None;
    }

    pub fn clear(&mut self) {
        if !self.buffer.is_empty() {
            self.history.push(self.buffer.clone());
        }
        self.buffer.clear();
        self.cursor_pos = 0;
        self.history_pos = None;
        self.tab_suggestion = None;
    }

    /// Set the dynamic context suggestions extracted from chat history.
    pub fn set_context_suggestions(&mut self, suggestions: Vec<SuggestionItem>) {
        self.context_suggestions = suggestions;
        self.suggestion_cursor = None;
    }

    /// Clear all dynamic context suggestions.
    pub fn clear_context_suggestions(&mut self) {
        self.context_suggestions.clear();
    }

    pub fn get_input(&self) -> String {
        self.buffer.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            let prev = self.buffer[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor_pos = prev;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.buffer.len() {
            let next = self.buffer[self.cursor_pos..]
                .char_indices()
                .next()
                .map(|(i, c)| self.cursor_pos + i + c.len_utf8())
                .unwrap_or(self.buffer.len());
            self.cursor_pos = next;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor_pos = self.buffer.len();
    }

    pub fn navigate_history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let pos = match self.history_pos {
            Some(p) if p > 0 => p - 1,
            None => self.history.len().saturating_sub(1),
            _ => return,
        };
        self.history_pos = Some(pos);
        self.buffer = self.history[pos].clone();
        self.cursor_pos = self.buffer.len();
    }

    pub fn navigate_history_down(&mut self) {
        match self.history_pos {
            Some(p) if p + 1 < self.history.len() => {
                self.history_pos = Some(p + 1);
                self.buffer = self.history[p + 1].clone();
                self.cursor_pos = self.buffer.len();
            }
            Some(_) => {
                self.history_pos = None;
                self.buffer.clear();
                self.cursor_pos = 0;
            }
            None => {}
        }
    }

    pub fn select_suggestion_up(&mut self) {
        let matches = self.matching_commands();
        if matches.is_empty() {
            return;
        }
        let max = matches.len().saturating_sub(1);
        let idx = match self.suggestion_cursor {
            Some(i) if i > 0 => (i - 1).min(max),
            Some(_) | None => max,
        };
        self.suggestion_cursor = Some(idx);
    }

    pub fn select_suggestion_down(&mut self) {
        let matches = self.matching_commands();
        if matches.is_empty() {
            return;
        }
        let max = matches.len().saturating_sub(1);
        let idx = match self.suggestion_cursor {
            Some(i) if i < max => i + 1,
            Some(_) => 0,
            None => 0,
        };
        self.suggestion_cursor = Some(idx);
    }

    pub fn accept_selected_suggestion(&mut self) -> Option<String> {
        let idx = self.suggestion_cursor?;
        let matches = self.matching_commands();
        let item = matches.get(idx)?;
        let label = item.label();
        self.buffer = label.clone();
        self.cursor_pos = self.buffer.len();
        self.suggestion_cursor = None;
        Some(label.clone())
    }

    pub fn tab_complete(&mut self) {
        let input = self.buffer.trim_start();

        if let Some(suggestion) = &self.tab_suggestion {
            self.buffer = suggestion.clone();
            self.cursor_pos = self.buffer.len();
            self.tab_suggestion = None;
            return;
        }

        if let Some(idx) = self.suggestion_cursor {
            let matches = self.matching_commands();
            if let Some(item) = matches.get(idx) {
                self.buffer = item.label();
                self.cursor_pos = self.buffer.len();
                return;
            }
        }

        if input.is_empty() || input.starts_with('/') {
            let partial = input.to_lowercase();
            let matches: Vec<SuggestionItem> = self
                .matching_commands()
                .into_iter()
                .filter(|s| s.label().to_lowercase().starts_with(&partial))
                .collect();

            if matches.len() == 1 {
                self.buffer = matches[0].label();
                self.cursor_pos = self.buffer.len();
            } else if !matches.is_empty() {
                let labels: Vec<String> = matches.iter().map(|s| s.label()).collect();
                let common_prefix = common_prefix_str(&labels);
                if common_prefix.len() > self.buffer.len() {
                    self.buffer = common_prefix;
                    self.cursor_pos = self.buffer.len();
                } else {
                    self.tab_suggestion = Some(matches[0].label());
                }
            }
        } else {
            // Non-slash input: tab-complete against context suggestions.
            let partial = input.to_lowercase();
            let matches: Vec<SuggestionItem> = self
                .context_suggestions
                .iter()
                .filter(|s| s.label().to_lowercase().starts_with(&partial))
                .cloned()
                .collect();

            if matches.len() == 1 {
                self.buffer = matches[0].label();
                self.cursor_pos = self.buffer.len();
            } else if !matches.is_empty() {
                let labels: Vec<String> = matches.iter().map(|s| s.label()).collect();
                let common_prefix = common_prefix_str(&labels);
                if common_prefix.len() > self.buffer.len() {
                    self.buffer = common_prefix;
                    self.cursor_pos = self.buffer.len();
                } else {
                    self.tab_suggestion = Some(matches[0].label());
                }
            }
        }
    }

    /// Return all suggestions (slash commands + context items) that match the
    /// current input buffer. When the buffer is empty, all context suggestions
    /// are shown. When the buffer starts with '/', only slash commands matching
    /// the partial text are returned. Otherwise context suggestions whose label
    /// starts with the input (case-insensitive) are returned.
    pub fn matching_commands(&self) -> Vec<SuggestionItem> {
        let input = self.buffer.trim_start();
        if input.is_empty() {
            // Empty input: show all context suggestions (slash commands are
            // only shown when the user starts typing '/').
            return self.context_suggestions.clone();
        }

        if input.starts_with('/') {
            let partial = input.to_lowercase();
            let mut matches: Vec<SuggestionItem> = SLASH_COMMANDS
                .iter()
                .filter(|(cmd, _)| cmd.starts_with(&partial))
                .map(|(cmd, desc)| SuggestionItem::SlashCommand(*cmd, *desc))
                .collect();
            // Also include context suggestions that start with the partial
            // text, so context items are always discoverable.
            let ctx_matches: Vec<SuggestionItem> = self
                .context_suggestions
                .iter()
                .filter(|s| s.label().to_lowercase().starts_with(&partial))
                .cloned()
                .collect();
            matches.extend(ctx_matches);
            matches
        } else {
            // Non-slash input: match against context suggestions only.
            let partial = input.to_lowercase();
            self.context_suggestions
                .iter()
                .filter(|s| s.label().to_lowercase().starts_with(&partial))
                .cloned()
                .collect()
        }
    }

    pub fn suggestion_count(&self) -> usize {
        self.matching_commands().len()
    }

    /// Ghost suffix text shown after the buffer as a dim completion hint.
    fn ghost_suffix_text(&self) -> Option<String> {
        let buffer = &self.buffer;
        if buffer.is_empty() || buffer.starts_with('/') {
            return None;
        }
        self.matching_commands().first().and_then(|item| {
            let label = item.label();
            if label.len() > buffer.len()
                && label.to_lowercase().starts_with(&buffer.to_lowercase())
            {
                Some(label[buffer.len()..].to_string())
            } else {
                None
            }
        })
    }

    fn visual_line_count(text: &str, content_width: usize) -> u16 {
        if text.is_empty() || content_width == 0 {
            return 1;
        }
        let mut line_w = 0usize;
        let mut lines = 1u16;
        for c in text.chars() {
            let w = c.width().unwrap_or(0);
            if line_w + w > content_width {
                lines += 1;
                line_w = w;
            } else {
                line_w += w;
            }
        }
        lines
    }

    /// Estimate how many visual lines the full input (buffer + ghost suffix)
    /// occupies when wrapped at `content_width` columns. At least 1.
    pub fn wrapped_height(&self, content_width: u16) -> u16 {
        let combined = match self.ghost_suffix_text() {
            Some(ghost) => format!("{}{}", self.buffer, ghost),
            None => self.buffer.clone(),
        };
        Self::visual_line_count(&combined, content_width as usize)
    }

    /// Compute (row, col) of the cursor inside the wrapped content area.
    fn cursor_line_col(&self, content_width: u16) -> (u16, u16) {
        if content_width == 0 {
            return (0, 0);
        }
        let cw = content_width as usize;
        let text_before = &self.buffer[..self.cursor_pos];
        let mut line = 0u16;
        let mut col = 0usize;
        for c in text_before.chars() {
            let w = c.width().unwrap_or(0);
            if col + w > cw {
                line += 1;
                col = w;
            } else {
                col += w;
            }
        }
        (line, col as u16)
    }

    pub fn render_suggestions(&self, frame: &mut Frame, area: Rect) {
        let matches = self.matching_commands();
        if matches.is_empty() {
            return;
        }

        let mut lines = Vec::new();
        for (i, item) in matches.iter().enumerate() {
            let selected = self.suggestion_cursor == Some(i);
            let bullet = if selected { " ▶ " } else { "   " };
            let is_context = item.is_context();
            let label = item.label();
            let desc = item.description();

            let label_style = if selected {
                Style::default()
                    .fg(BG)
                    .bg(CYAN)
                    .add_modifier(Modifier::BOLD)
            } else if is_context {
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
            };

            let desc_fg = if selected { CYAN } else { TEXT_DIM };
            let desc_text = if is_context {
                format!(" (from context)")
            } else {
                format!(" {}", desc)
            };

            lines.push(Line::from(vec![
                Span::styled(
                    bullet,
                    Style::default().fg(if selected { CYAN } else { TEXT_DIM }),
                ),
                Span::styled(format!("{:<14}", label), label_style),
                Span::styled(desc_text, Style::default().fg(desc_fg)),
            ]));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Suggestions ")
            .border_style(Style::default().fg(CYAN));
        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let content_width = area.width.saturating_sub(2);
        let buffer = &self.buffer;
        let is_slash_command = buffer.starts_with('/');

        let mut spans = Vec::new();
        let ghost_suffix = self.ghost_suffix_text();

        if is_slash_command {
            let input_lower = buffer.to_lowercase();
            let matched = SLASH_COMMANDS.iter().any(|(cmd, _)| cmd == &input_lower);

            let style = if matched {
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(RED).add_modifier(Modifier::BOLD)
            };

            let parts: Vec<&str> = buffer.splitn(2, ' ').collect();
            spans.push(Span::styled(parts[0].to_string(), style));
            if parts.len() > 1 {
                spans.push(Span::styled(
                    format!(" {}", parts[1]),
                    Style::default().fg(TEXT),
                ));
            }
        } else if !buffer.is_empty() {
            spans.push(Span::styled(buffer.clone(), Style::default().fg(TEXT)));
            if let Some(suffix) = &ghost_suffix {
                spans.push(Span::styled(suffix.clone(), Style::default().fg(TEXT_DIM)));
            }
        } else if !self.context_suggestions.is_empty() {
            let hints: Vec<String> = self
                .context_suggestions
                .iter()
                .take(3)
                .map(|s| s.label())
                .collect();
            spans.push(Span::styled(
                hints.join("  ·  "),
                Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
            ));
        } else {
            spans.push(Span::styled(
                " ▸ Type a message... (/help for commands)",
                Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
            ));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .border_style(Style::default().fg(if is_slash_command { CYAN } else { TEXT_DIM }));

        let paragraph = Paragraph::new(Line::from(spans))
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(BG));
        frame.render_widget(paragraph, area);

        let (cursor_line, cursor_col) = self.cursor_line_col(content_width);
        let cursor_y = area.y + 1 + cursor_line;
        let cursor_x = area.x + 1 + cursor_col;
        if cursor_y < area.y + area.height - 1 && cursor_x < area.x + area.width - 1 {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

fn common_prefix_str(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = &strings[0];
    let mut prefix_len = first.len();
    for s in &strings[1..] {
        prefix_len = first
            .chars()
            .zip(s.chars())
            .take_while(|(a, b)| a == b)
            .count();
    }
    first[..prefix_len].to_string()
}
