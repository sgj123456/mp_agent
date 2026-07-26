use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use super::{CODE_BG, CYAN, SURFACE, TEXT, TEXT_DIM, YELLOW};

pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(text, options);
    let renderer = MarkdownRenderer::new().feed(parser);
    renderer.finish()
}

pub fn strip_markdown(text: &str) -> String {
    let mut result = String::new();
    let parser = pulldown_cmark::Parser::new(text);
    for event in parser {
        match event {
            Event::Text(t) => result.push_str(&t),
            Event::Code(t) => result.push_str(&t),
            Event::HardBreak | Event::SoftBreak => result.push(' '),
            _ => {}
        }
    }
    result
}

#[derive(Default, Clone)]
struct StyleModifiers {
    bold: bool,
    italic: bool,
    strikethrough: bool,
    heading: Option<HeadingLevel>,
}

struct MarkdownRenderer {
    lines: Vec<Line<'static>>,
    spans: Vec<Span<'static>>,
    style_stack: Vec<StyleModifiers>,
    list_counters: Vec<u64>,
    in_blockquote: bool,
    in_code_block: bool,
    pending_newline: bool,
    code_block_lang: String,
    code_buffer: String,
    in_table: bool,
    in_table_cell: bool,
    is_header_row: bool,
    current_row: Vec<String>,
    table_rows: Vec<(Vec<String>, bool)>,
    cell_buffer: String,
}

impl MarkdownRenderer {
    fn new() -> Self {
        MarkdownRenderer {
            lines: Vec::new(),
            spans: Vec::new(),
            style_stack: Vec::new(),
            list_counters: Vec::new(),
            in_blockquote: false,
            in_code_block: false,
            pending_newline: false,
            code_block_lang: String::new(),
            code_buffer: String::new(),
            in_table: false,
            in_table_cell: false,
            is_header_row: false,
            current_row: Vec::new(),
            table_rows: Vec::new(),
            cell_buffer: String::new(),
        }
    }

    fn current_style(&self) -> Style {
        let mut mods = Modifier::empty();
        for s in &self.style_stack {
            if s.bold {
                mods |= Modifier::BOLD;
            }
            if s.italic {
                mods |= Modifier::ITALIC;
            }
            if s.strikethrough {
                mods |= Modifier::CROSSED_OUT;
            }
        }
        Style::default().fg(TEXT).add_modifier(mods)
    }

    fn push_span(&mut self, text: String) {
        if self.in_code_block || self.in_table {
            return;
        }
        self.spans.push(Span::styled(text, self.current_style()));
    }

    fn flush_paragraph(&mut self) {
        if self.spans.is_empty() {
            return;
        }
        if self.pending_newline && !self.lines.is_empty() {
            self.lines.push(Line::from(""));
        }
        if self.in_blockquote {
            let mut quoted = vec![Span::styled(
                "  ▍ ",
                Style::default()
                    .fg(Color::Rgb(120, 120, 120))
                    .add_modifier(Modifier::DIM),
            )];
            quoted.append(&mut self.spans);
            self.lines.push(Line::from(quoted));
        } else {
            self.lines
                .push(Line::from(self.spans.drain(..).collect::<Vec<_>>()));
        }
        self.pending_newline = true;
    }

    fn feed(mut self, parser: pulldown_cmark::Parser<'_>) -> Self {
        for event in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Heading { level, .. } => {
                        self.flush_paragraph();
                        let prefix = match level {
                            HeadingLevel::H1 => "# ",
                            HeadingLevel::H2 => "## ",
                            HeadingLevel::H3 => "### ",
                            _ => "#### ",
                        };
                        self.spans
                            .push(Span::styled(prefix, Style::default().fg(TEXT_DIM)));
                        self.style_stack.push(StyleModifiers {
                            heading: Some(level),
                            ..Default::default()
                        });
                    }
                    Tag::BlockQuote(_) => {
                        self.flush_paragraph();
                        self.in_blockquote = true;
                    }
                    Tag::List(start) => {
                        self.flush_paragraph();
                        self.list_counters.push(start.unwrap_or(0));
                    }
                    Tag::Item => {
                        self.flush_paragraph();
                        if let Some(counter) = self.list_counters.last_mut() {
                            let prefix = if *counter > 0 {
                                *counter += 1;
                                format!(" {}. ", *counter - 1)
                            } else {
                                " • ".to_string()
                            };
                            self.spans.push(Span::styled(
                                prefix,
                                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
                            ));
                        }
                    }
                    Tag::CodeBlock(kind) => {
                        self.flush_paragraph();
                        self.in_code_block = true;
                        self.code_block_lang = match kind {
                            CodeBlockKind::Fenced(lang) => lang.to_string(),
                            CodeBlockKind::Indented => String::new(),
                        };
                        self.code_buffer = String::new();
                    }
                    Tag::Emphasis => {
                        self.style_stack.push(StyleModifiers {
                            italic: true,
                            ..Default::default()
                        });
                    }
                    Tag::Strong => {
                        self.style_stack.push(StyleModifiers {
                            bold: true,
                            ..Default::default()
                        });
                    }
                    Tag::Strikethrough => {
                        self.style_stack.push(StyleModifiers {
                            strikethrough: true,
                            ..Default::default()
                        });
                    }
                    Tag::Table(_) => {
                        self.flush_paragraph();
                        self.in_table = true;
                        self.table_rows.clear();
                        self.current_row.clear();
                        self.is_header_row = false;
                    }
                    Tag::TableHead => {
                        self.is_header_row = true;
                    }
                    Tag::TableRow => {
                        self.current_row.clear();
                    }
                    Tag::TableCell => {
                        self.in_table_cell = true;
                        self.cell_buffer.clear();
                    }
                    _ => {}
                },
                Event::End(tag_end) => match tag_end {
                    TagEnd::Paragraph => {
                        self.flush_paragraph();
                    }
                    TagEnd::Heading(level) => {
                        self.flush_paragraph();
                        self.style_stack.pop();
                        let line_style = Style::default().fg(TEXT_DIM).add_modifier(Modifier::DIM);
                        match level {
                            HeadingLevel::H1 => {
                                self.lines.push(Line::from(Span::styled(
                                    " ──────────────────────────────────",
                                    line_style,
                                )));
                            }
                            HeadingLevel::H2 => {
                                self.lines.push(Line::from(Span::styled(
                                    " ────────────────────────────────",
                                    line_style,
                                )));
                            }
                            _ => {}
                        }
                        self.lines.push(Line::from(""));
                    }
                    TagEnd::BlockQuote(_) => {
                        self.flush_paragraph();
                        self.in_blockquote = false;
                    }
                    TagEnd::List(_) => {
                        self.list_counters.pop();
                        self.pending_newline = true;
                    }
                    TagEnd::Item => {
                        self.flush_paragraph();
                    }
                    TagEnd::CodeBlock => {
                        self.in_code_block = false;
                        let code = std::mem::take(&mut self.code_buffer);
                        let lang = std::mem::take(&mut self.code_block_lang);
                        if !code.is_empty() {
                            self.lines.extend(code_block_to_lines(&code, &lang));
                            self.lines.push(Line::from(""));
                        }
                    }
                    TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                        self.style_stack.pop();
                    }
                    TagEnd::Table => {
                        self.flush_table();
                        self.in_table = false;
                    }
                    TagEnd::TableHead => {
                        self.is_header_row = false;
                    }
                    TagEnd::TableRow => {
                        let row = std::mem::take(&mut self.current_row);
                        let header = self.is_header_row;
                        self.table_rows.push((row, header));
                    }
                    TagEnd::TableCell => {
                        let cell = std::mem::take(&mut self.cell_buffer);
                        self.current_row.push(cell);
                        self.in_table_cell = false;
                    }
                    _ => {}
                },
                Event::Text(text) => {
                    if self.in_code_block {
                        self.code_buffer.push_str(&text);
                    } else if self.in_table_cell {
                        self.cell_buffer.push_str(&text);
                    } else {
                        self.push_span(text.to_string());
                    }
                }
                Event::Code(text) => {
                    if self.in_code_block {
                    } else if self.in_table_cell {
                        self.cell_buffer.push_str(&text);
                    } else {
                        self.spans.push(Span::styled(
                            text.to_string(),
                            Style::default().fg(Color::Rgb(206, 145, 120)).bg(SURFACE),
                        ));
                    }
                }
                Event::SoftBreak | Event::HardBreak => {
                    if self.in_code_block {
                        self.code_buffer.push('\n');
                    } else if self.in_table_cell {
                        self.cell_buffer.push(' ');
                    } else {
                        self.push_span(" ".to_string());
                    }
                }
                _ => {}
            }
        }
        self.flush_paragraph();
        if self.lines.is_empty() {
            self.lines.push(Line::from(""));
        }
        self
    }

    fn flush_table(&mut self) {
        if self.table_rows.is_empty() {
            return;
        }

        let col_count = self
            .table_rows
            .iter()
            .map(|(r, _)| r.len())
            .max()
            .unwrap_or(0);
        if col_count == 0 {
            return;
        }

        let mut col_widths = vec![0usize; col_count];
        for (row, _) in &self.table_rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    let w = cell.width();
                    col_widths[i] = col_widths[i].max(w);
                }
            }
        }
        for w in &mut col_widths {
            *w = (*w + 2).max(3);
        }

        let border = Style::default().fg(TEXT_DIM);

        let top = format!(
            "┌{}┐",
            col_widths
                .iter()
                .map(|w| "─".repeat(*w))
                .collect::<Vec<_>>()
                .join("┬")
        );
        self.lines.push(Line::from(Span::styled(top, border)));

        for (row_idx, (row, is_header)) in self.table_rows.iter().enumerate() {
            let mut line = String::from("│");
            for (i, col_w) in col_widths.iter().enumerate() {
                let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                let disp = cell.width();
                let right_pad = col_w.saturating_sub(1 + disp);
                line.push_str(&format!(" {}{}│", cell, " ".repeat(right_pad)));
            }
            let style = if *is_header {
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            self.lines.push(Line::from(Span::styled(line, style)));

            if *is_header && row_idx + 1 < self.table_rows.len() {
                let sep = format!(
                    "├{}┤",
                    col_widths
                        .iter()
                        .map(|w| "─".repeat(*w))
                        .collect::<Vec<_>>()
                        .join("┼")
                );
                self.lines.push(Line::from(Span::styled(sep, border)));
            }
        }

        let bottom = format!(
            "└{}┘",
            col_widths
                .iter()
                .map(|w| "─".repeat(*w))
                .collect::<Vec<_>>()
                .join("┴")
        );
        self.lines.push(Line::from(Span::styled(bottom, border)));
        self.lines.push(Line::from(""));
    }

    fn finish(self) -> Vec<Line<'static>> {
        self.lines
    }
}

fn code_block_to_lines(code: &str, lang: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let border = Style::default().fg(TEXT_DIM).add_modifier(Modifier::DIM);
    let bg = Style::default().bg(CODE_BG);

    if lang.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ┌─ code ─────────────────────────",
            border,
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!("  ┌─ {} ─────────────────────────", lang),
            border,
        )));
    }

    for line in code.lines() {
        let code_spans = highlight_line(line, lang);
        let prefix = Span::styled("  │ ", Style::default().fg(TEXT_DIM).bg(CODE_BG));
        let mut line_spans = vec![prefix];
        for s in code_spans {
            line_spans.push(Span::styled(s.content, s.style.patch(bg)));
        }
        lines.push(Line::from(line_spans));
    }

    lines.push(Line::from(Span::styled(
        "  └────────────────────────────",
        border,
    )));
    lines
}

fn highlight_line(line: &str, lang: &str) -> Vec<Span<'static>> {
    let keywords = match lang {
        "rust" | "rs" => KEYWORDS_RUST,
        "python" | "py" => KEYWORDS_PYTHON,
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" => KEYWORDS_JS,
        "go" | "golang" => KEYWORDS_GO,
        "java" => KEYWORDS_JAVA,
        "c" | "cpp" | "c++" | "h" | "hpp" => KEYWORDS_CPP,
        "bash" | "sh" | "shell" | "zsh" | "fish" => KEYWORDS_BASH,
        _ => &[],
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut pos = 0;
    let bytes = line.as_bytes();

    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            spans.push(Span::styled(line[pos..pos + 1].to_string(), default_fg()));
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }

        if let Some(end) = try_comment(line, pos) {
            spans.push(Span::styled(line[pos..end].to_string(), comment_style()));
            if end == line.len() {
                break;
            }
            pos = end;
            continue;
        }

        if let Some(len) = try_string(line, pos) {
            spans.push(Span::styled(
                line[pos..pos + len].to_string(),
                string_style(),
            ));
            pos += len;
            continue;
        }

        if let Some(len) = try_number(line, pos) {
            spans.push(Span::styled(
                line[pos..pos + len].to_string(),
                number_style(),
            ));
            pos += len;
            continue;
        }

        if let Some(len) = try_word(line, pos) {
            let word = &line[pos..pos + len];
            let kw_lower = word.to_lowercase();
            if keywords.contains(&kw_lower.as_str()) || keywords.contains(&word) {
                spans.push(Span::styled(word.to_string(), keyword_style()));
            } else if pos > 0 {
                let before = &line[..pos];
                let trimmed = before.trim_end();
                if trimmed.ends_with('.') {
                    spans.push(Span::styled(word.to_string(), field_style()));
                } else {
                    spans.push(Span::styled(word.to_string(), default_fg()));
                }
            } else {
                spans.push(Span::styled(word.to_string(), default_fg()));
            }

            let after_pos = pos + len;
            let after = &line[after_pos..];
            let after_trimmed = after.trim_start();
            if after_trimmed.starts_with('(') {
                spans.pop();
                spans.push(Span::styled(word.to_string(), function_style()));
            }

            pos = after_pos;
            continue;
        }

        let c = line[pos..].chars().next().unwrap();
        let c_len = c.len_utf8();
        let s = &line[pos..pos + c_len];
        let style = if SYMBOLS.contains(&s) {
            punctuation_style()
        } else {
            default_fg()
        };
        spans.push(Span::styled(s.to_string(), style));
        pos += c_len;
    }

    spans
}

fn try_comment(line: &str, pos: usize) -> Option<usize> {
    let s = &line[pos..];
    if s.starts_with("//") {
        return Some(line.len());
    }
    if s.starts_with("--") {
        return Some(line.len());
    }
    if s.starts_with("/*") {
        if let Some(end) = s.find("*/") {
            return Some(pos + end + 2);
        }
        return None;
    }
    if s.starts_with('#') {
        let before = line[..pos].trim_end();
        if before.is_empty() || before.ends_with('\n') {
            return Some(line.len());
        }
    }
    None
}

fn try_string(line: &str, pos: usize) -> Option<usize> {
    let s = &line[pos..];
    let quote = s.chars().next()?;
    if quote != '"' && quote != '\'' && quote != '`' {
        return None;
    }

    let mut i = 1;
    let mut escaped = false;
    while i < s.len() {
        let c = s[i..].chars().next().unwrap();
        let c_len = c.len_utf8();
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote {
            i += c_len;
            return Some(i);
        }
        i += c_len;
    }

    Some(s.len())
}

fn try_number(line: &str, pos: usize) -> Option<usize> {
    let s = &line[pos..];
    let bytes = s.as_bytes();
    let mut i = 0;
    if i < bytes.len() && bytes[i] == b'-' {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'0' && i + 1 < bytes.len() {
        let next = bytes[i + 1];
        if next == b'x' || next == b'X' {
            i += 2;
            while i < bytes.len() && (bytes[i].is_ascii_hexdigit() || bytes[i] == b'_') {
                i += 1;
            }
            return Some(i);
        }
        if next == b'b' || next == b'B' {
            i += 2;
            while i < bytes.len() && (bytes[i] == b'0' || bytes[i] == b'1' || bytes[i] == b'_') {
                i += 1;
            }
            return Some(i);
        }
        if next == b'o' || next == b'O' {
            i += 2;
            while i < bytes.len() && (bytes[i] >= b'0' && bytes[i] <= b'7' || bytes[i] == b'_') {
                i += 1;
            }
            return Some(i);
        }
    }
    let mut has_dot = false;
    let mut has_exp = false;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_digit() || b == b'_' {
            i += 1;
        } else if b == b'.' && !has_dot && !has_exp {
            has_dot = true;
            i += 1;
        } else if (b == b'e' || b == b'E') && !has_exp {
            has_exp = true;
            i += 1;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
        } else {
            break;
        }
    }
    if i > 0 { Some(i) } else { None }
}

fn try_word(line: &str, pos: usize) -> Option<usize> {
    let s = &line[pos..];
    let mut i = 0;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    if i > 0 { Some(i) } else { None }
}

const SYMBOLS: &[&str] = &[
    "(", ")", "{", "}", "[", "]", ",", ";", ":", ".", "+", "-", "*", "/", "%", "=", "!", "<", ">",
    "&", "|", "^", "~", "?", "+=", "-=", "*=", "/=", "%=", "==", "!=", "<=", ">=", "&&", "||",
    "->", "=>", "..", "...",
];

const KEYWORDS_RUST: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "try", "typeof", "unsized", "virtual", "yield",
];

const KEYWORDS_PYTHON: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

const KEYWORDS_JS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "of",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "implements",
    "interface",
    "package",
    "private",
    "protected",
    "public",
    "static",
    "from",
    "as",
];

const KEYWORDS_GO: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
    "true",
    "false",
    "nil",
];

const KEYWORDS_JAVA: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "true",
    "try",
    "void",
    "volatile",
    "while",
];

const KEYWORDS_CPP: &[&str] = &[
    "alignas",
    "alignof",
    "auto",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "constexpr",
    "continue",
    "decltype",
    "default",
    "delete",
    "do",
    "double",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "false",
    "float",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "nullptr",
    "operator",
    "override",
    "private",
    "protected",
    "public",
    "register",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "typeid",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
    "include",
    "define",
    "ifdef",
    "ifndef",
    "endif",
    "pragma",
    "import",
    "module",
    "export",
];

const KEYWORDS_BASH: &[&str] = &[
    "if", "then", "else", "elif", "fi", "case", "esac", "for", "while", "until", "do", "done",
    "in", "function", "select", "time", "return", "exit", "break", "continue", "declare", "local",
    "export", "readonly", "unset", "set", "unset", "trap", "exec", "source", "shift",
];

fn comment_style() -> Style {
    Style::default()
        .fg(Color::Rgb(98, 148, 98))
        .add_modifier(Modifier::ITALIC)
}

fn string_style() -> Style {
    Style::default().fg(Color::Rgb(206, 145, 120))
}

fn number_style() -> Style {
    Style::default().fg(Color::Rgb(181, 137, 227))
}

fn keyword_style() -> Style {
    Style::default()
        .fg(Color::Rgb(86, 156, 214))
        .add_modifier(Modifier::BOLD)
}

fn function_style() -> Style {
    Style::default().fg(Color::Rgb(220, 220, 170))
}

fn punctuation_style() -> Style {
    Style::default().fg(Color::Rgb(212, 212, 212))
}

fn field_style() -> Style {
    Style::default().fg(Color::Rgb(156, 220, 254))
}

fn default_fg() -> Style {
    Style::default().fg(Color::Rgb(212, 208, 200))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span_text_and_style<'a>(spans: &'a [Span<'static>]) -> Vec<(&'a str, Style)> {
        spans
            .iter()
            .map(|s| (s.content.as_ref(), s.style))
            .collect()
    }

    #[test]
    fn test_keyword_highlighting() {
        let spans = highlight_line("fn foo()", "rust");
        let entries = span_text_and_style(&spans);
        assert_eq!(entries.len(), 5, "spans: {:?}", spans);
        assert_eq!(entries[0].0, "fn");
        assert_eq!(entries[1].0, " ");
        assert_eq!(entries[2].0, "foo");
        assert_eq!(entries[3].0, "(");
        assert_eq!(entries[4].0, ")");
        // fn is a keyword
        assert_eq!(entries[0].1.fg, Some(Color::Rgb(86, 156, 214)));
        // foo() => function call
        assert_eq!(entries[2].1.fg, Some(Color::Rgb(220, 220, 170)));
    }

    #[test]
    fn test_string_highlighting() {
        let spans = highlight_line(r#"x = "hello""#, "python");
        let entries = span_text_and_style(&spans);
        assert!(entries.len() >= 5);
        assert_eq!(entries[4].0, r#""hello""#);
        assert_eq!(entries[4].1.fg, Some(Color::Rgb(206, 145, 120)));
    }

    #[test]
    fn test_comment_highlighting() {
        let spans = highlight_line("// this is a comment", "rust");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "// this is a comment");
        assert_eq!(spans[0].style.fg, Some(Color::Rgb(98, 148, 98)));
    }

    #[test]
    fn test_number_highlighting() {
        let spans = highlight_line("x = 42", "rust");
        let entries = span_text_and_style(&spans);
        assert!(entries.len() >= 5);
        assert_eq!(entries[4].0, "42");
        assert_eq!(entries[4].1.fg, Some(Color::Rgb(181, 137, 227)));
    }

    #[test]
    fn test_string_contains_comment_syntax() {
        let spans = highlight_line(r#"url = "http://example.com""#, "python");
        let entries = span_text_and_style(&spans);
        // "http://example.com" should be a string, not a comment (//)
        assert!(
            entries
                .iter()
                .any(|(text, _)| text.contains("http://example.com"))
        );
        assert!(
            entries.iter().any(|(text, style)| text.contains("http")
                && style.fg == Some(Color::Rgb(206, 145, 120)))
        );
    }

    #[test]
    fn test_dot_field_access() {
        let spans = highlight_line("obj.field", "python");
        let entries = span_text_and_style(&spans);
        assert!(
            entries.iter().any(
                |(text, style)| *text == "field" && style.fg == Some(Color::Rgb(156, 220, 254))
            )
        );
    }
}
