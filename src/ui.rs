pub mod chat;
pub mod input;
pub mod layout;
pub mod markdown;

use ratatui::style::Color;

pub const BG: Color = Color::Rgb(18, 18, 20);
pub const SURFACE: Color = Color::Rgb(26, 26, 31);
pub const TEXT: Color = Color::Rgb(218, 218, 222);
pub const TEXT_DIM: Color = Color::Rgb(108, 110, 120);

pub const ACCENT: Color = Color::Rgb(86, 156, 214);
pub const GREEN: Color = Color::Rgb(76, 185, 116);
pub const RED: Color = Color::Rgb(232, 82, 82);
pub const YELLOW: Color = Color::Rgb(235, 203, 73);
pub const CYAN: Color = Color::Rgb(106, 196, 204);

pub const DIFF_REMOVE: Color = Color::Rgb(232, 82, 82);
pub const DIFF_ADD: Color = Color::Rgb(76, 185, 116);
pub const DIFF_HEADER: Color = Color::Rgb(86, 156, 214);
