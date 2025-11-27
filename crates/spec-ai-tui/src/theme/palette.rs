//! Color palettes and themes

use crate::style::{Color, Style};

/// A color palette for consistent theming
#[derive(Debug, Clone)]
pub struct Palette {
    /// Primary accent color
    pub primary: Color,
    /// Secondary accent color
    pub secondary: Color,
    /// Background color
    pub background: Color,
    /// Surface color (slightly lighter than background)
    pub surface: Color,
    /// Text color
    pub text: Color,
    /// Muted text color
    pub text_muted: Color,
    /// Success color
    pub success: Color,
    /// Warning color
    pub warning: Color,
    /// Error color
    pub error: Color,
    /// Border color
    pub border: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

impl Palette {
    /// Create a dark theme palette
    pub fn dark() -> Self {
        Self {
            primary: Color::Cyan,
            secondary: Color::Magenta,
            background: Color::Reset,
            surface: Color::DarkGrey,
            text: Color::White,
            text_muted: Color::Grey,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            border: Color::DarkGrey,
        }
    }

    /// Create a light theme palette
    pub fn light() -> Self {
        Self {
            primary: Color::Blue,
            secondary: Color::Magenta,
            background: Color::White,
            surface: Color::Grey,
            text: Color::Black,
            text_muted: Color::DarkGrey,
            success: Color::DarkGreen,
            warning: Color::DarkYellow,
            error: Color::DarkRed,
            border: Color::Grey,
        }
    }
}

/// A theme combining palette with component styles
#[derive(Debug, Clone)]
pub struct Theme {
    /// The color palette
    pub palette: Palette,
    /// Style for status bar
    pub status_bar: Style,
    /// Style for input fields
    pub input: Style,
    /// Style for input cursor
    pub input_cursor: Style,
    /// Style for borders
    pub border: Style,
    /// Style for focused borders
    pub border_focused: Style,
    /// Style for headers
    pub header: Style,
    /// Style for selection highlight
    pub selection: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Create a dark theme
    pub fn dark() -> Self {
        let palette = Palette::dark();
        Self {
            status_bar: Style::new().bg(palette.surface).fg(palette.text),
            input: Style::new().fg(palette.text),
            input_cursor: Style::new().bg(palette.text).fg(palette.background),
            border: Style::new().fg(palette.border),
            border_focused: Style::new().fg(palette.primary),
            header: Style::new().fg(palette.primary).bold(),
            selection: Style::new().bg(palette.primary).fg(palette.background),
            palette,
        }
    }

    /// Create a light theme
    pub fn light() -> Self {
        let palette = Palette::light();
        Self {
            status_bar: Style::new().bg(palette.surface).fg(palette.text),
            input: Style::new().fg(palette.text),
            input_cursor: Style::new().bg(palette.text).fg(palette.background),
            border: Style::new().fg(palette.border),
            border_focused: Style::new().fg(palette.primary),
            header: Style::new().fg(palette.primary).bold(),
            selection: Style::new().bg(palette.primary).fg(palette.background),
            palette,
        }
    }
}
