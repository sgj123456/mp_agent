use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Layout rectangles for the main application UI.
pub struct AppLayout {
    /// Chat message area (scrollable, occupies remaining vertical space).
    pub chat: Rect,
    /// Input area (command line with tab-completion overlay).
    pub input: Rect,
    /// Status bar (single line at the bottom showing tool count, spinner, status).
    pub status: Rect,
}

/// Compute the main application layout given the total area and the dynamic
/// height of the input area (based on wrapped content).
///
/// The input height is computed externally from `InputArea::wrapped_height()`
/// so that multi-line input is properly accommodated. The status bar is always
/// exactly 1 line tall. The chat area takes all remaining vertical space.
pub fn compute_layout(area: Rect, input_height: u16) -> AppLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),               // Chat area
            Constraint::Length(input_height), // Input area (dynamic)
            Constraint::Length(1),            // Status bar
        ])
        .split(area);

    AppLayout {
        chat: chunks[0],
        input: chunks[1],
        status: chunks[2],
    }
}

/// Compute the rectangular area for the choice panel that overlays the chat
/// area when the agent presents multiple approaches for the user to pick from.
///
/// The panel is anchored inside the chat area with a 2-line horizontal margin
/// and 1-line vertical margin. Its height is capped by the available chat
/// height so it never overflows.
pub fn compute_choice_panel(chat_area: Rect, n_choices: usize) -> Rect {
    let panel_width = chat_area.width.saturating_sub(4);
    let panel_height = (n_choices as u16 + 4).min(chat_area.height.saturating_sub(2));
    let panel_x = chat_area.x + 2;
    let panel_y = chat_area.y + 1;

    Rect {
        x: panel_x,
        y: panel_y,
        width: panel_width,
        height: panel_height,
    }
}

/// Compute the rectangular area for the slash-command / tab-completion
/// suggestion overlay. The panel floats upward from the top of the input area
/// so it doesn't collide with the 1-line status bar below.
///
/// Returns `None` if there are no matches or if there isn't enough vertical
/// space to display the panel.
pub fn compute_suggestion_panel(input_area: Rect, n_matches: usize) -> Option<Rect> {
    if n_matches == 0 {
        return None;
    }

    let suggestion_height = (n_matches as u16 + 2).clamp(3, 10);
    let suggested_y = input_area.y.saturating_sub(suggestion_height);
    let available = input_area.y + input_area.height - suggested_y;

    if available == 0 {
        return None;
    }

    Some(Rect {
        x: input_area.x,
        y: suggested_y,
        width: input_area.width,
        height: suggestion_height.min(available),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_layout_splits_vertically() {
        let area = Rect::new(0, 0, 80, 30);
        let layout = compute_layout(area, 5);

        assert_eq!(layout.chat.y, 0);
        assert_eq!(layout.chat.height, 24); // 30 - 5 - 1
        assert_eq!(layout.input.y, 24);
        assert_eq!(layout.input.height, 5);
        assert_eq!(layout.status.y, 29);
        assert_eq!(layout.status.height, 1);
    }

    #[test]
    fn test_compute_layout_splits_small_area() {
        // When total height is small (8 lines) and the constraints request
        // Min(5) + Length(5) + Length(1) = 11 lines, ratatui's layout engine
        // shrinks the areas proportionally. The exact heights are determined
        // by ratatui, so we only verify the structural invariants.
        let area = Rect::new(0, 0, 80, 8);
        let layout = compute_layout(area, 5);

        // The three rectangles must exactly tile the total area vertically.
        assert_eq!(layout.chat.y, 0);
        assert_eq!(layout.input.y, layout.chat.height);
        assert_eq!(layout.status.y, layout.chat.height + layout.input.height);
        assert_eq!(
            layout.chat.height + layout.input.height + layout.status.height,
            area.height
        );
        // Status bar is always exactly 1 line.
        assert_eq!(layout.status.height, 1);
        // Input area is requested as 5 but may be shrunk by ratatui.
        assert!(layout.input.height > 0);
        assert!(layout.chat.height > 0);
    }

    #[test]
    fn test_compute_choice_panel_fits_inside_chat() {
        let chat = Rect::new(0, 0, 80, 24);
        let panel = compute_choice_panel(chat, 5);

        assert!(panel.x >= chat.x);
        assert!(panel.y >= chat.y);
        assert!(panel.right() <= chat.right());
        assert!(panel.bottom() <= chat.bottom());
    }

    #[test]
    fn test_compute_suggestion_panel_floats_upward() {
        let input = Rect::new(0, 25, 80, 5);
        let panel = compute_suggestion_panel(input, 4).unwrap();

        assert_eq!(panel.x, input.x);
        assert_eq!(panel.width, input.width);
        assert!(panel.y < input.y); // floats above input
        assert!(panel.bottom() <= input.bottom());
    }

    #[test]
    fn test_compute_suggestion_panel_none_when_empty() {
        let input = Rect::new(0, 25, 80, 5);
        assert!(compute_suggestion_panel(input, 0).is_none());
    }
}
