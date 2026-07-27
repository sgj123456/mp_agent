use ratatui::text::{Line, Span};
use tui_markdown::from_str;

/// Render Markdown text into a list of Ratatui lines.
///
/// The rendered output is wrapped so that each line is prefixed with a card-style
/// bar when consumed by the chat area. Code blocks keep their syntax highlighting
/// from `tui-markdown`'s built-in theme.
pub fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let rendered = from_str(text);
    rendered
        .into_iter()
        .map(|line| {
            Line::from(
                line.spans
                    .into_iter()
                    .map(|span| {
                        Span::styled(
                            span.content.to_string(),
                            span.style,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// Strip Markdown markup and return plain text.
///
/// We render the Markdown through `tui-markdown` and then read the resulting
/// `ratatui::Text` back as a plain string, which gives us a reasonably clean
/// textual representation without pulling in `pulldown-cmark` directly.
#[allow(dead_code)]
pub fn strip_markdown(text: &str) -> String {
    from_str(text).to_string()
}