use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use unicode_width::UnicodeWidthChar;

use super::{BG, CYAN, RED, TEXT, TEXT_DIM, YELLOW};

pub const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show help information"),
    ("/clear", "Clear chat history"),
    ("/model", "Show or change model"),
    ("/skills", "List loaded skills"),
    ("/skill:", "Load a skill (Tab for names)"),
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

/// Fallback hints shown when no AI prediction is available and input is empty.
const EMPTY_HINTS: &[&str] = &[
    "解释代码...",
    "修改这个功能...",
    "添加注释或文档...",
    "修复问题...",
    "重构代码...",
    "添加测试...",
    "解释这段代码的工作原理",
    "帮我优化一下",
    "添加新功能...",
];

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
    /// Rotates through EMPTY_HINTS on each render for a fresh feel.
    hint_index: usize,
    /// AI-predicted next user input, shown as gray text when buffer is empty.
    predicted_input: Option<String>,
    /// Skill names for `/skill:` tab completion.
    skill_names: Vec<String>,
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
            hint_index: 0,
            predicted_input: None,
            skill_names: Vec::new(),
        }
    }

    /// Set available skill names for `/skill:` completion.
    pub fn set_skill_names(&mut self, names: Vec<String>) {
        self.skill_names = names;
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
        self.tab_suggestion = None;
    }

    /// Insert a string at the cursor position (handles pasted text).
    /// Treats `\n` as a literal newline character, avoiding spurious submission.
    pub fn insert_text(&mut self, text: &str) {
        self.buffer.insert_str(self.cursor_pos, text);
        self.cursor_pos += text.len();
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

    /// Set cursor to the buffer position closest to a mouse click at the given
    /// (row, col) within the input's content area (0-indexed, excluding borders).
    pub fn set_cursor_by_click(&mut self, row: u16, col: u16, content_width: u16) {
        if content_width == 0 {
            return;
        }
        let cw = content_width as usize;
        let target_line = row as usize;
        let target_col = col as usize;

        let mut visual_line = 0usize;
        let mut visual_col = 0usize;

        for (byte_idx, c) in self.buffer.char_indices() {
            if visual_line > target_line {
                self.cursor_pos = byte_idx;
                self.tab_suggestion = None;
                return;
            }
            if visual_line == target_line && visual_col >= target_col {
                self.cursor_pos = byte_idx;
                self.tab_suggestion = None;
                return;
            }
            match c {
                '\n' => {
                    if visual_line == target_line {
                        self.cursor_pos = byte_idx;
                        self.tab_suggestion = None;
                        return;
                    }
                    visual_line += 1;
                    visual_col = 0;
                }
                _ => {
                    let w = c.width().unwrap_or(0);
                    if visual_col > 0 && visual_col + w > cw {
                        visual_line += 1;
                        visual_col = w;
                    } else {
                        visual_col += w;
                    }
                }
            }
        }

        self.cursor_pos = self.buffer.len();
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

    /// Store the AI-predicted next user input (shown as gray text when empty).
    pub fn set_prediction(&mut self, prediction: String) {
        self.predicted_input = Some(prediction);
    }

    pub fn get_input(&self) -> String {
        self.buffer.clone()
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

    /// Navigate backwards in the input history. Returns true if the buffer
    /// was changed.
    pub fn history_up(&mut self) -> bool {
        if self.history_pos.is_none() {
            // First up press: push current input to history and go to last item.
            if !self.buffer.is_empty() {
                self.history.push(self.buffer.clone());
            }
            if let Some(last) = self.history.last() {
                self.buffer = last.clone();
                self.cursor_pos = self.buffer.len();
                self.history_pos = Some(self.history.len().saturating_sub(1));
                return true;
            }
            return false;
        }
        let pos = self.history_pos.unwrap_or(0);
        if pos > 0 {
            let new_pos = pos - 1;
            if let Some(item) = self.history.get(new_pos) {
                self.buffer = item.clone();
                self.cursor_pos = self.buffer.len();
                self.history_pos = Some(new_pos);
                return true;
            }
        }
        false
    }

    /// Navigate forwards in the input history. Returns true if the buffer
    /// was changed.
    pub fn history_down(&mut self) -> bool {
        if let Some(pos) = self.history_pos {
            if pos + 1 < self.history.len() {
                let new_pos = pos + 1;
                if let Some(item) = self.history.get(new_pos) {
                    self.buffer = item.clone();
                    self.cursor_pos = self.buffer.len();
                    self.history_pos = Some(new_pos);
                    return true;
                }
            } else {
                // End of history: clear input.
                self.buffer.clear();
                self.cursor_pos = 0;
                self.history_pos = None;
                return true;
            }
        }
        false
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

        // Empty buffer → fill ghost text (prediction or EMPTY_HINT)
        if input.is_empty()
            && let Some(s) = self.ghost_suffix_text()
        {
            self.buffer = s;
            self.cursor_pos = self.buffer.len();
            self.tab_suggestion = None;
            return;
        }

        let partial = input.to_lowercase();
        let matches: Vec<SuggestionItem> = if input.starts_with('/') {
            self.matching_commands()
                .into_iter()
                .filter(|s| s.label().to_lowercase().starts_with(&partial))
                .collect()
        } else {
            self.context_suggestions
                .iter()
                .filter(|s| s.label().to_lowercase().starts_with(&partial))
                .cloned()
                .collect()
        };

        self.apply_completion(matches);
    }

    /// Common tab-completion logic: given a list of filtered matches, fill the
    /// buffer with the single match, or the common prefix of multiple matches,
    /// or save the first match as a preview for next Tab press.
    fn apply_completion(&mut self, matches: Vec<SuggestionItem>) {
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

    /// Return all suggestions (slash commands + context items) that match the
    /// current input buffer. When the buffer is empty, all context suggestions
    /// are shown. When the buffer starts with '/', ONLY built-in slash commands
    /// matching the partial text are returned (context suggestions are excluded
    /// so the slash-command menu stays clean and predictable). Otherwise context
    /// suggestions whose label starts with the input (case-insensitive) are
    /// returned.
    pub fn matching_commands(&self) -> Vec<SuggestionItem> {
        let input = self.buffer.trim_start();
        if input.is_empty() {
            // Empty input: show all context suggestions (slash commands are
            // only shown when the user starts typing '/').
            return self.context_suggestions.clone();
        }

        if input.starts_with('/') {
            let partial = input.to_lowercase();
            let mut cmds: Vec<SuggestionItem> = SLASH_COMMANDS
                .iter()
                .filter(|(cmd, _)| cmd.starts_with(&partial))
                .map(|(cmd, desc)| SuggestionItem::SlashCommand(cmd, desc))
                .collect();

            // Dynamic /skill: completions from loaded skill names
            if partial.starts_with("/skill:") && !self.skill_names.is_empty() {
                let filter = partial.strip_prefix("/skill:").unwrap_or("");
                for name in &self.skill_names {
                    if name.to_lowercase().starts_with(filter) {
                        cmds.push(SuggestionItem::Context(format!("/skill:{}", name)));
                    }
                }
            }

            cmds
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

    /// Ghost suffix text shown after the buffer as a dim completion hint.
    /// When the buffer is non-empty, returns the remainder of the single best
    /// matching context suggestion. When the buffer is empty, returns the
    /// AI-predicted next input (if available), falling back to the current
    /// EMPTY_HINT text (with decoration stripped).
    fn ghost_suffix_text(&self) -> Option<String> {
        let buffer = &self.buffer;
        if buffer.starts_with('/') {
            return None;
        }
        if buffer.is_empty() {
            if self.predicted_input.is_some() {
                return self.predicted_input.clone();
            }
            let idx = (self.hint_index / 180) % EMPTY_HINTS.len();
            let hint = EMPTY_HINTS[idx];
            let clean = hint.strip_prefix("▸ ").unwrap_or(hint);
            return Some(clean.to_string());
        }
        let matches = self.matching_commands();
        if matches.len() != 1 {
            return None;
        }
        let label = matches[0].label();
        if label.len() > buffer.len() && label.to_lowercase().starts_with(&buffer.to_lowercase()) {
            Some(label[buffer.len()..].to_string())
        } else {
            None
        }
    }

    fn visual_line_count(text: &str, content_width: usize) -> u16 {
        if text.is_empty() || content_width == 0 {
            return 1;
        }
        let mut line_w = 0usize;
        let mut lines = 1u16;
        for c in text.chars() {
            match c {
                '\n' => {
                    // Explicit newline: start a fresh visual line.
                    lines += 1;
                    line_w = 0;
                }
                _ => {
                    let w = c.width().unwrap_or(0);
                    // Only wrap if there is already content on the current line
                    // and adding this character would exceed the content width.
                    // If the line is empty, allow wide characters (e.g. CJK) to
                    // overflow onto the current line, matching Ratatui's
                    // WordWrapper behavior which places the character on the
                    // current line even when it exceeds the width limit.
                    if line_w > 0 && line_w + w > content_width {
                        lines += 1;
                        line_w = w;
                    } else {
                        line_w += w;
                    }
                }
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
            match c {
                '\n' => {
                    line += 1;
                    col = 0;
                }
                _ => {
                    let w = c.width().unwrap_or(0);
                    // Only wrap if there is already content on the current line
                    // and adding this character would exceed the content width.
                    // If the line is empty, allow wide characters (e.g. CJK) to
                    // overflow onto the current line, matching Ratatui's
                    // WordWrapper behavior.
                    if col > 0 && col + w > cw {
                        line += 1;
                        col = w;
                    } else {
                        col += w;
                    }
                }
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
            } else {
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
            };

            let desc_fg = if selected { CYAN } else { TEXT_DIM };
            let desc_text = if is_context {
                " (from context)".to_string()
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

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let content_width = area.width.saturating_sub(2);
        let buffer = &self.buffer;
        let is_slash_command = buffer.starts_with('/');
        let ghost_suffix = self.ghost_suffix_text();

        let paragraph = if is_slash_command {
            // Slash commands are single-line, keep current rendering
            let input_lower = buffer.to_lowercase();
            let matched = SLASH_COMMANDS.iter().any(|(cmd, _)| cmd == &input_lower);
            let style = if matched {
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(RED).add_modifier(Modifier::BOLD)
            };
            let parts: Vec<&str> = buffer.splitn(2, ' ').collect();
            let mut spans = vec![Span::styled(parts[0].to_string(), style)];
            if parts.len() > 1 {
                spans.push(Span::styled(
                    format!(" {}", parts[1]),
                    Style::default().fg(TEXT),
                ));
            }
            Paragraph::new(Line::from(spans))
        } else if !buffer.is_empty() {
            // Split at \n so Ratatui renders hard line breaks
            let segments: Vec<&str> = buffer.split('\n').collect();
            let mut lines: Vec<Line> = segments
                .iter()
                .map(|s| Line::from(Span::styled(s.to_string(), Style::default().fg(TEXT))))
                .collect();
            // Append ghost suffix to the last line
            if let Some(suffix) = &ghost_suffix
                && let Some(last) = lines.last_mut()
            {
                last.push_span(Span::styled(suffix.clone(), Style::default().fg(TEXT_DIM)));
            }
            Paragraph::new(Text::from(lines))
        } else if let Some(predicted) = &self.predicted_input {
            Paragraph::new(Line::from(Span::styled(
                format!("▸ {}...", predicted),
                Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
            )))
        } else {
            let idx = (self.hint_index / 180) % EMPTY_HINTS.len();
            let hint = EMPTY_HINTS[idx];
            Paragraph::new(Line::from(Span::styled(
                hint,
                Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
            )))
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .border_style(Style::default().fg(if is_slash_command { CYAN } else { TEXT_DIM }));

        let paragraph = paragraph
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

        // Advance the frame counter; only rotate the displayed hint every
        // ~3 seconds (~180 frames at 60fps) so it doesn't flicker.
        if buffer.is_empty() {
            self.hint_index = self.hint_index.wrapping_add(1);
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
