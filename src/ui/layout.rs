use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub struct AppLayout {
    pub chat: Rect,
    pub input: Rect,
    pub status: Rect,
}

pub fn compute_layout(area: Rect) -> AppLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // Chat area
            Constraint::Length(3), // Input area
            Constraint::Length(1), // Status bar
        ])
        .split(area);

    AppLayout {
        chat: chunks[0],
        input: chunks[1],
        status: chunks[2],
    }
}
