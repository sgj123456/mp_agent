pub mod chat;
pub mod input;
#[allow(dead_code)]
pub mod layout;
pub mod markdown;

use ratatui::style::Color;

pub const BG: Color = Color::Rgb(18, 18, 20);
pub const SURFACE: Color = Color::Rgb(26, 26, 31);
pub const SURFACE_HOVER: Color = Color::Rgb(38, 38, 46);
pub const TEXT: Color = Color::Rgb(218, 218, 222);
pub const TEXT_DIM: Color = Color::Rgb(108, 110, 120);
pub const TEXT_MUTED: Color = Color::Rgb(72, 74, 84);
pub const TEXT_WHITE: Color = Color::Rgb(240, 240, 240);

pub const ACCENT: Color = Color::Rgb(86, 156, 214);
pub const ACCENT_DIM: Color = Color::Rgb(50, 90, 140);
pub const GREEN: Color = Color::Rgb(76, 185, 116);
pub const GREEN_DIM: Color = Color::Rgb(40, 110, 70);
pub const RED: Color = Color::Rgb(232, 82, 82);
pub const RED_DIM: Color = Color::Rgb(140, 50, 50);
pub const YELLOW: Color = Color::Rgb(235, 203, 73);
pub const YELLOW_DIM: Color = Color::Rgb(140, 120, 40);
pub const CYAN: Color = Color::Rgb(106, 196, 204);
pub const CYAN_DIM: Color = Color::Rgb(60, 120, 130);
pub const ORANGE: Color = Color::Rgb(235, 157, 67);
pub const ORANGE_DIM: Color = Color::Rgb(140, 90, 40);
pub const PURPLE: Color = Color::Rgb(175, 137, 238);
pub const MAGENTA: Color = Color::Rgb(204, 137, 215);

pub const CODE_BG: Color = Color::Rgb(22, 22, 27);
pub const CODE_BORDER: Color = Color::Rgb(44, 44, 54);
pub const DIFF_REMOVE: Color = Color::Rgb(232, 82, 82);
pub const DIFF_ADD: Color = Color::Rgb(76, 185, 116);
pub const DIFF_HEADER: Color = Color::Rgb(86, 156, 214);
pub const SEPARATOR: Color = Color::Rgb(38, 38, 46);
pub const BORDER: Color = Color::Rgb(44, 44, 54);
pub const BORDER_DIM: Color = Color::Rgb(32, 32, 40);