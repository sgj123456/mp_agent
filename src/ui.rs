pub mod chat;
pub mod input;
#[allow(dead_code)]
pub mod layout;
pub mod markdown;

use ratatui::style::Color;

pub const BG: Color = Color::Rgb(24, 24, 27);
pub const SURFACE: Color = Color::Rgb(30, 30, 35);
pub const TEXT: Color = Color::Rgb(220, 220, 220);
pub const TEXT_DIM: Color = Color::Rgb(130, 130, 140);
pub const TEXT_WHITE: Color = Color::Rgb(240, 240, 240);

pub const ACCENT: Color = Color::Rgb(86, 156, 214);
pub const GREEN: Color = Color::Rgb(80, 200, 120);
pub const RED: Color = Color::Rgb(220, 80, 80);
pub const YELLOW: Color = Color::Rgb(220, 180, 60);
pub const CYAN: Color = Color::Rgb(60, 190, 200);
pub const ORANGE: Color = Color::Rgb(220, 150, 60);

pub const CODE_BG: Color = Color::Rgb(30, 30, 35);
pub const DIFF_REMOVE: Color = Color::Rgb(220, 80, 80);
pub const DIFF_ADD: Color = Color::Rgb(80, 200, 120);
pub const DIFF_HEADER: Color = Color::Rgb(86, 156, 214);
