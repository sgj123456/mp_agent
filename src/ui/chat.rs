use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::LazyLock;
use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

use super::{
    ACCENT, BG, CYAN, DIFF_ADD, DIFF_HEADER, DIFF_REMOVE, GREEN, RED, TEXT, TEXT_DIM, YELLOW,
    markdown,
};

/// Simple LRU cache for markdown rendering results.
/// Avoids re-parsing the same assistant/system messages on every frame.
struct MarkdownCache {
    entries: Vec<(u64, Vec<Line<'static>>)>,
}

impl MarkdownCache {
    fn new(capacity: usize) -> Self {
        MarkdownCache {
            entries: Vec::with_capacity(capacity),
        }
    }

    fn get(&mut self, text: &str) -> Option<Vec<Line<'static>>> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let key = hasher.finish();

        for i in (0..self.entries.len()).rev() {
            if self.entries[i].0 == key {
                return Some(self.entries.remove(i).1);
            }
        }
        None
    }

    fn put(&mut self, text: &str, lines: Vec<Line<'static>>) {
        const CAPACITY: usize = 64;
        if self.entries.len() >= CAPACITY {
            self.entries.remove(0);
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let key = hasher.finish();
        self.entries.push((key, lines));
    }
}

/// Global markdown cache shared across the application.
static MARKDOWN_CACHE: LazyLock<std::sync::Mutex<RefCell<MarkdownCache>>> =
    LazyLock::new(|| std::sync::Mutex::new(RefCell::new(MarkdownCache::new(64))));

fn render_markdown_cached(text: &str) -> Vec<Line<'static>> {
    if text.is_empty() {
        return vec![Line::from("")];
    }
    {
        let cache = MARKDOWN_CACHE.lock().unwrap();
        let mut cache_ref = cache.borrow_mut();
        if let Some(cached) = cache_ref.get(text) {
            return cached;
        }
    }
    let rendered = markdown::render_markdown(text);
    {
        let cache = MARKDOWN_CACHE.lock().unwrap();
        let mut cache_ref = cache.borrow_mut();
        cache_ref.put(text, rendered.clone());
    }
    rendered
}

#[derive(Debug, Clone)]
pub enum ChatMessage {
    User(String),
    Assistant(String),
    ToolCall { name: String, args: String },
    ToolResult { name: String, result: String },
    Error(String),
    System(String),
}

pub struct ChatArea {
    messages: Vec<(ChatMessage, Instant)>,
    scroll_offset: u16,
    auto_scroll: bool,
    area_height: u16,
    folded: HashSet<usize>,
}

impl ChatArea {
    pub fn new() -> Self {
        ChatArea {
            messages: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            area_height: 20,
            folded: HashSet::new(),
        }
    }

    pub fn add_message(&mut self, msg: ChatMessage) {
        let idx = self.messages.len();
        if let ChatMessage::ToolResult { result, .. } = &msg {
            let line_count = result.lines().count();
            if line_count > 12 {
                self.folded.insert(idx);
            }
        }
        self.messages.push((msg, Instant::now()));
        self.scroll_to_bottom();
    }

    pub fn scroll_up(&mut self) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        let max = self.max_scroll();
        if self.scroll_offset < max {
            self.scroll_offset += 1;
        }
        if self.scroll_offset >= max {
            self.auto_scroll = true;
        }
    }

    pub fn scroll_page_up(&mut self, page_size: u16) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(page_size);
    }

    pub fn scroll_page_down(&mut self, page_size: u16) {
        let max = self.max_scroll();
        self.scroll_offset = (self.scroll_offset + page_size).min(max);
        if self.scroll_offset >= max {
            self.auto_scroll = true;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.max_scroll();
        self.auto_scroll = true;
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn toggle_fold_at_scroll(&mut self) {
        self.toggle_fold_at_line(self.scroll_offset as usize);
    }

    pub fn toggle_fold_at_line(&mut self, absolute_line: usize) {
        let mut pos = 0usize;
        for (idx, (msg, _)) in self.messages.iter().enumerate() {
            let h = self.message_lines(msg, idx);
            if absolute_line >= pos && absolute_line < pos + h {
                if let ChatMessage::ToolResult { .. } = msg {
                    if self.folded.contains(&idx) {
                        self.folded.remove(&idx);
                    } else {
                        let total = match msg {
                            ChatMessage::ToolResult { result, .. } => result.lines().count(),
                            _ => 0,
                        };
                        if total > 12 {
                            self.folded.insert(idx);
                        }
                    }
                }
                break;
            }
            pos += h;
        }
    }

    fn max_scroll(&self) -> u16 {
        let total_lines: usize = self
            .messages
            .iter()
            .enumerate()
            .map(|(i, (m, _))| self.message_lines(m, i))
            .sum();
        let visible = self.area_height.max(1) as usize;
        if total_lines > visible {
            (total_lines - visible) as u16
        } else {
            0
        }
    }

    fn message_lines(&self, msg: &ChatMessage, idx: usize) -> usize {
        match msg {
            ChatMessage::User(text) => text.lines().count() + 2,
            ChatMessage::Assistant(text) => {
                let rendered = render_markdown_cached(text);
                // header + rendered content lines + trailing empty
                rendered.len() + 2
            }
            ChatMessage::ToolCall { name, args } => {
                if name == "edit_file" {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
                        let old_s = val.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
                        let new_s = val.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
                        // header + remove-box-header + old-lines + add-box-header + new-lines + box-footer
                        1 + 3 + old_s.lines().count() + new_s.lines().count()
                    } else {
                        2
                    }
                } else {
                    // header + summary body line (no trailing empty)
                    2
                }
            }
            ChatMessage::ToolResult { result, .. } => {
                let result_lines: Vec<&str> = result.lines().collect();
                let total = result_lines.len();
                let is_diff = result_lines
                    .iter()
                    .any(|l| l.starts_with("--- ") || l.starts_with("+++ "));
                let is_error = result.contains("Error:") || result.contains("⛔");
                let show_fold = total > 12 && !is_error && !is_diff;
                let is_folded = show_fold && self.folded.contains(&idx);
                let shown = if is_folded { 12 } else { total };
                if is_diff {
                    // header + diff-box-header + shown content + diff-box-footer + trailing empty
                    shown + 4
                } else {
                    // header + shown content + trailing empty (+ fold indicator if folded)
                    let fold = if is_folded { 1 } else { 0 };
                    shown + 2 + fold
                }
            }
            ChatMessage::Error(text) => text.lines().count() + 2,
            ChatMessage::System(text) => {
                let rendered = render_markdown_cached(text);
                rendered.len() + 2
            }
        }
    }

    fn card_header(name: &str, fg: Color) -> Line<'static> {
        Line::from(vec![
            Span::styled(" ── ", Style::default().fg(fg).add_modifier(Modifier::BOLD)),
            Span::styled(
                name.to_string(),
                Style::default().fg(fg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default().fg(fg)),
        ])
    }

    fn card_body_line(content: &str, color: Color, dim: bool) -> Line<'static> {
        let mut style = Style::default().fg(color);
        if dim {
            style = style.add_modifier(Modifier::DIM);
        }
        Line::from(vec![
            Span::styled(
                " │ ",
                Style::default().fg(color).add_modifier(Modifier::DIM),
            ),
            Span::styled(content.to_string(), style),
        ])
    }

    fn wrap_markdown_lines(rendered: Vec<Line<'static>>, color: Color) -> Vec<Line<'static>> {
        let mut out = Vec::with_capacity(rendered.len());
        let bar_style = Style::default().fg(color).add_modifier(Modifier::DIM);
        for line in rendered {
            let text = Self::line_to_plain(&line);
            if text.starts_with('┌')
                || text.starts_with('│')
                || text.starts_with('└')
                || text.starts_with('├')
                || text.starts_with('┠')
                || text.starts_with('┷')
                || text.starts_with('━')
                || text.starts_with("──")
                || text.starts_with('▍')
            {
                out.push(line);
            } else {
                let mut spans = vec![Span::styled(" │ ", bar_style)];
                spans.extend(line);
                out.push(Line::from(spans));
            }
        }
        out
    }

    fn line_to_plain(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn build_lines(&self, preview: Option<&str>) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();

        for (idx, (msg, _time)) in self.messages.iter().enumerate() {
            match msg {
                ChatMessage::User(text) => {
                    lines.push(Self::card_header("User", GREEN));
                    for line in text.lines() {
                        lines.push(Self::card_body_line(line, TEXT, false));
                    }
                    lines.push(Line::from(""));
                }
                ChatMessage::Assistant(text) => {
                    lines.push(Self::card_header("Assistant", CYAN));
                    let rendered = render_markdown_cached(text);
                    for line in Self::wrap_markdown_lines(rendered, CYAN) {
                        lines.push(line);
                    }
                    lines.push(Line::from(""));
                }
                ChatMessage::ToolCall { name, args } => {
                    if name == "edit_file" {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(args) {
                            let path = val.get("path").and_then(|v| v.as_str()).unwrap_or("?");
                            let old_s =
                                val.get("old_string").and_then(|v| v.as_str()).unwrap_or("");
                            let new_s =
                                val.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
                            lines.push(Self::card_header(&format!("edit_file — {}", path), YELLOW));
                            let border = Style::default().fg(TEXT_DIM).add_modifier(Modifier::DIM);
                            lines.push(Line::from(Span::styled(
                                "  ┌─ remove ────────────────────",
                                border,
                            )));
                            for line in old_s.lines() {
                                lines.push(Line::from(Span::styled(
                                    format!("  │ {}", line),
                                    Style::default().fg(DIFF_REMOVE),
                                )));
                            }
                            lines.push(Line::from(Span::styled(
                                "  ├─ add ───────────────────────",
                                border,
                            )));
                            for line in new_s.lines() {
                                lines.push(Line::from(Span::styled(
                                    format!("  │ {}", line),
                                    Style::default().fg(DIFF_ADD),
                                )));
                            }
                            lines.push(Line::from(Span::styled(
                                "  └────────────────────────────",
                                border,
                            )));
                        } else {
                            lines.push(Self::card_header(name, YELLOW));
                            lines.push(Self::card_body_line(&truncate(args, 80), TEXT_DIM, true));
                        }
                    } else {
                        let summary = summarize_tool_call(name, args);
                        lines.push(Self::card_header(name, YELLOW));
                        lines.push(Self::card_body_line(&summary, TEXT_DIM, true));
                    }
                }
                ChatMessage::ToolResult { name, result } => {
                    let is_error = result.contains("Error:") || result.contains("⛔");
                    let (symbol, color) = if is_error {
                        ("✗", RED)
                    } else {
                        ("✔", GREEN)
                    };
                    lines.push(Self::card_header(&format!("{} {}", symbol, name), color));
                    let mut result_lines: Vec<String> = Vec::new();
                    let mut is_diff = false;
                    for line in result.lines() {
                        result_lines.push(line.to_string());
                        if line.starts_with("--- ") || line.starts_with("+++ ") {
                            is_diff = true;
                        }
                    }
                    let total = result_lines.len();
                    let fold_threshold = 12;
                    let show_fold = total > fold_threshold && !is_error && !is_diff;
                    let is_folded = show_fold && self.folded.contains(&idx);
                    let shown = if is_folded { fold_threshold } else { total };

                    if is_diff {
                        let dim = Style::default().fg(TEXT_DIM).add_modifier(Modifier::DIM);
                        lines.push(Line::from(Span::styled(
                            "  ┌─ diff ────────────────────────",
                            dim,
                        )));
                        for line in result_lines.iter().take(shown) {
                            if line.starts_with("--- ") || line.starts_with("+++ ") {
                                lines.push(Line::from(Span::styled(
                                    format!("  │{}", line),
                                    Style::default().fg(DIFF_HEADER),
                                )));
                            } else if line.starts_with("@@ ") {
                                lines.push(Line::from(Span::styled(
                                    format!("  │{}", line),
                                    Style::default().fg(TEXT_DIM),
                                )));
                            } else if line.starts_with('-') {
                                lines.push(Line::from(Span::styled(
                                    format!("  │{}", line),
                                    Style::default().fg(DIFF_REMOVE),
                                )));
                            } else if line.starts_with('+') {
                                lines.push(Line::from(Span::styled(
                                    format!("  │{}", line),
                                    Style::default().fg(DIFF_ADD),
                                )));
                            } else {
                                lines.push(Line::from(Span::styled(
                                    format!("  │{}", line),
                                    Style::default().fg(TEXT_DIM),
                                )));
                            }
                        }
                        lines.push(Line::from(Span::styled(
                            "  └────────────────────────────",
                            dim,
                        )));
                    } else if is_error {
                        for line in result_lines.iter().take(shown) {
                            lines.push(Self::card_body_line(line, RED, false));
                        }
                    } else {
                        for line in result_lines.iter().take(shown) {
                            lines.push(Self::card_body_line(line, TEXT_DIM, true));
                        }
                    }
                    if is_folded {
                        let remaining = total - fold_threshold;
                        lines.push(Line::from(Span::styled(
                            format!("  … {} lines folded (click to expand)", remaining),
                            Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
                        )));
                    }
                    lines.push(Line::from(""));
                }
                ChatMessage::Error(text) => {
                    lines.push(Self::card_header("Error", RED));
                    for line in text.lines() {
                        lines.push(Self::card_body_line(line, RED, false));
                    }
                    lines.push(Line::from(""));
                }
                ChatMessage::System(text) => {
                    lines.push(Self::card_header("System", ACCENT));
                    let rendered = markdown::render_markdown(text);
                    for line in Self::wrap_markdown_lines(rendered, ACCENT) {
                        lines.push(line);
                    }
                    lines.push(Line::from(""));
                }
            }
        }

        if let Some(preview_text) = preview {
            if preview_text.is_empty() {
                return lines;
            }
            lines.push(Self::card_header("Assistant (streaming)", CYAN));
            let rendered = markdown::render_markdown(preview_text);
            for line in Self::wrap_markdown_lines(rendered, CYAN) {
                lines.push(line);
            }
            lines.push(Line::from(Span::styled(" │ ▋", Style::default().fg(CYAN))));
        }

        lines
    }

    fn compute_scroll(&mut self, lines: &[Line<'static>], area_height: u16) {
        self.area_height = area_height;
        let total = lines.len();
        let visible = (area_height as usize).max(1);
        let max_scroll = total.saturating_sub(visible);
        if self.auto_scroll || self.scroll_offset > max_scroll as u16 {
            self.scroll_offset = max_scroll as u16;
        }
    }

    fn render_lines(&mut self, frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
        let total = lines.len();
        let paragraph = Paragraph::new(lines)
            .scroll((self.scroll_offset, 0))
            .style(Style::default().bg(BG));
        frame.render_widget(paragraph, area);
        let mut state = ScrollbarState::new(total).position(self.scroll_offset as usize);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓")),
            area,
            &mut state,
        );
    }

    pub fn render_with_preview(&mut self, frame: &mut Frame, area: Rect, preview: &str) {
        let lines = self.build_lines(Some(preview));
        self.compute_scroll(&lines, area.height);
        self.render_lines(frame, area, lines);
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let lines = self.build_lines(None);
        self.compute_scroll(&lines, area.height);
        self.render_lines(frame, area, lines);
    }
}

fn safe_truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max_chars {
        let truncated: String = chars[..max_chars].iter().collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

fn summarize_tool_call(name: &str, args_json: &str) -> String {
    let args: serde_json::Value =
        serde_json::from_str(args_json).unwrap_or(serde_json::Value::Null);
    match name {
        "read_file" | "write_file" | "edit_file" => {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
                    format!("{} ({} chars)", path, content.len())
                } else if args.get("old_string").is_some() {
                    let old = args["old_string"].as_str().unwrap_or("");
                    let new = args["new_string"].as_str().unwrap_or("");
                    format!(
                        "{} (replace {} → {})",
                        path,
                        truncate(old, 30),
                        truncate(new, 30)
                    )
                } else if let Some(limit) = args.get("limit").and_then(|v| v.as_u64()) {
                    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0);
                    format!("{} (lines {}-{})", path, offset, offset + limit)
                } else {
                    path.to_string()
                }
            } else {
                truncate(args_json, 80)
            }
        }
        "bash" => {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                let truncated = truncate(cmd, 80);
                if let Some(dir) = args.get("workdir").and_then(|v| v.as_str()) {
                    format!("{} (in {})", truncated, dir)
                } else {
                    truncated
                }
            } else {
                truncate(args_json, 80)
            }
        }
        "glob" => {
            if let Some(pattern) = args.get("pattern").and_then(|v| v.as_str()) {
                pattern.to_string()
            } else {
                truncate(args_json, 80)
            }
        }
        "grep" => {
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
            let include = args.get("include").and_then(|v| v.as_str()).unwrap_or("*");
            format!("'{}' in {}", truncate(pattern, 40), include)
        }
        "list_directory" => {
            if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                path.to_string()
            } else {
                truncate(args_json, 80)
            }
        }
        _ => truncate(args_json, 80),
    }
}

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() > max {
        let truncated: String = chars[..max].iter().collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}
