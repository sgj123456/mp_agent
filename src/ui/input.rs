use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

use super::{BG, CYAN, RED, TEXT, TEXT_DIM, YELLOW};

pub const SLASH_COMMANDS: &[(&str, &str)] = &[
    ("/help", "Show help information"),
    ("/clear", "Clear chat history"),
    ("/model", "Show or change model"),
    ("/tools", "List available tools"),
    ("/exit", "Exit the application"),
    ("/history", "Show command history"),
];

pub struct InputArea {
    buffer: String,
    cursor_pos: usize,
    history: Vec<String>,
    history_pos: Option<usize>,
    tab_suggestion: Option<String>,
    suggestion_cursor: Option<usize>,
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
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.buffer.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
        self.tab_suggestion = None;
        self.suggestion_cursor = None;
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
        self.suggestion_cursor = None;
    }

    pub fn clear(&mut self) {
        if !self.buffer.is_empty() {
            self.history.push(self.buffer.clone());
        }
        self.buffer.clear();
        self.cursor_pos = 0;
        self.history_pos = None;
        self.tab_suggestion = None;
        self.suggestion_cursor = None;
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
        let (cmd, _) = *matches.get(idx)?;
        self.buffer = cmd.to_string();
        self.cursor_pos = self.buffer.len();
        self.suggestion_cursor = None;
        Some(cmd.to_string())
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
            if let Some((cmd, _)) = matches.get(idx) {
                self.buffer = cmd.to_string();
                self.cursor_pos = self.buffer.len();
                return;
            }
        }

        if input.starts_with('/') {
            let partial = input.to_lowercase();
            let matches: Vec<&str> = SLASH_COMMANDS
                .iter()
                .map(|(cmd, _)| *cmd)
                .filter(|cmd| cmd.starts_with(&partial))
                .collect();

            if matches.len() == 1 {
                self.buffer = matches[0].to_string();
                self.cursor_pos = self.buffer.len();
            } else if !matches.is_empty() {
                let common_prefix = common_prefix(&matches);
                if common_prefix.len() > self.buffer.len() {
                    self.buffer = common_prefix.to_string();
                    self.cursor_pos = self.buffer.len();
                } else if !matches.is_empty() {
                    self.tab_suggestion = Some(matches[0].to_string());
                }
            }
        }
    }

    pub fn matching_commands(&self) -> Vec<(&'static str, &'static str)> {
        let input = self.buffer.trim_start();
        if !input.starts_with('/') {
            return Vec::new();
        }
        let partial = input.to_lowercase();
        SLASH_COMMANDS
            .iter()
            .filter(|(cmd, _)| cmd.starts_with(&partial))
            .copied()
            .collect()
    }

    pub fn suggestion_count(&self) -> usize {
        if !self.buffer.starts_with('/') {
            return 0;
        }
        self.matching_commands().len()
    }

    pub fn render_suggestions(&self, frame: &mut Frame, area: Rect) {
        let matches = self.matching_commands();
        if matches.is_empty() {
            return;
        }

        let mut lines = Vec::new();
        for (i, (cmd, desc)) in matches.iter().enumerate() {
            let selected = self.suggestion_cursor == Some(i);
            let bullet = if selected { " ▶ " } else { "   " };
            lines.push(Line::from(vec![
                Span::styled(
                    bullet,
                    Style::default().fg(if selected { CYAN } else { TEXT_DIM }),
                ),
                Span::styled(
                    format!("{:<12}", cmd),
                    Style::default()
                        .fg(if selected { CYAN } else { YELLOW })
                        .add_modifier(if selected {
                            Modifier::BOLD | Modifier::UNDERLINED
                        } else {
                            Modifier::BOLD
                        }),
                ),
                Span::styled(format!(" {}", desc), Style::default().fg(TEXT_DIM)),
            ]));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Commands ")
            .border_style(Style::default().fg(YELLOW));
        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let mut spans = Vec::new();
        let buffer = &self.buffer;
        let is_slash_command = buffer.starts_with('/');

        if buffer.is_empty() {
            spans.push(Span::styled(
                " Type a message... (/help for commands)",
                Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
            ));
        } else if is_slash_command {
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
        } else {
            spans.push(Span::styled(buffer.clone(), Style::default().fg(TEXT)));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Input ")
            .border_style(Style::default().fg(if is_slash_command { YELLOW } else { TEXT_DIM }));

        let paragraph = Paragraph::new(Line::from(spans))
            .block(block)
            .style(Style::default().bg(BG));
        frame.render_widget(paragraph, area);

        let text_before_cursor = &self.buffer[..self.cursor_pos];
        let cursor_col = text_before_cursor.width() as u16 + 1;
        if cursor_col < area.width - 1 {
            frame.set_cursor_position((area.x + cursor_col, area.y + 1));
        }
    }
}

fn common_prefix(strings: &[&str]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    let first = strings[0];
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
