//! Terminal colors

use crossterm::style::Color as CrosstermColor;

/// Terminal color
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Color {
    /// Reset to terminal default
    #[default]
    Reset,
    /// Black
    Black,
    /// Dark grey
    DarkGrey,
    /// Red
    Red,
    /// Dark red
    DarkRed,
    /// Green
    Green,
    /// Dark green
    DarkGreen,
    /// Yellow
    Yellow,
    /// Dark yellow
    DarkYellow,
    /// Blue
    Blue,
    /// Dark blue
    DarkBlue,
    /// Magenta
    Magenta,
    /// Dark magenta
    DarkMagenta,
    /// Cyan
    Cyan,
    /// Dark cyan
    DarkCyan,
    /// White
    White,
    /// Grey
    Grey,
    /// Light red
    LightRed,
    /// Light green
    LightGreen,
    /// Light yellow
    LightYellow,
    /// Light blue
    LightBlue,
    /// Light magenta
    LightMagenta,
    /// Light cyan
    LightCyan,
    /// RGB color
    Rgb(u8, u8, u8),
    /// ANSI 256-color palette index
    Indexed(u8),
}

impl Color {
    /// Create an RGB color
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::Rgb(r, g, b)
    }

    /// Create an indexed color
    pub const fn indexed(index: u8) -> Self {
        Self::Indexed(index)
    }

    /// Parse a hex color string (e.g., "#ff0000" or "ff0000")
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if hex.len() != 6 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

        Some(Self::Rgb(r, g, b))
    }
}

impl From<Color> for CrosstermColor {
    fn from(color: Color) -> Self {
        match color {
            Color::Reset => CrosstermColor::Reset,
            Color::Black => CrosstermColor::Black,
            Color::DarkGrey => CrosstermColor::DarkGrey,
            Color::Red => CrosstermColor::Red,
            Color::DarkRed => CrosstermColor::DarkRed,
            Color::Green => CrosstermColor::Green,
            Color::DarkGreen => CrosstermColor::DarkGreen,
            Color::Yellow => CrosstermColor::Yellow,
            Color::DarkYellow => CrosstermColor::DarkYellow,
            Color::Blue => CrosstermColor::Blue,
            Color::DarkBlue => CrosstermColor::DarkBlue,
            Color::Magenta => CrosstermColor::Magenta,
            Color::DarkMagenta => CrosstermColor::DarkMagenta,
            Color::Cyan => CrosstermColor::Cyan,
            Color::DarkCyan => CrosstermColor::DarkCyan,
            Color::White => CrosstermColor::White,
            Color::Grey => CrosstermColor::Grey,
            Color::LightRed => CrosstermColor::Red, // Crossterm doesn't have light variants
            Color::LightGreen => CrosstermColor::Green,
            Color::LightYellow => CrosstermColor::Yellow,
            Color::LightBlue => CrosstermColor::Blue,
            Color::LightMagenta => CrosstermColor::Magenta,
            Color::LightCyan => CrosstermColor::Cyan,
            Color::Rgb(r, g, b) => CrosstermColor::Rgb { r, g, b },
            Color::Indexed(i) => CrosstermColor::AnsiValue(i),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_rgb() {
        let c = Color::rgb(255, 128, 0);
        assert_eq!(c, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn test_color_from_hex() {
        assert_eq!(Color::from_hex("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(Color::from_hex("00ff00"), Some(Color::Rgb(0, 255, 0)));
        assert_eq!(Color::from_hex("invalid"), None);
    }

    #[test]
    fn test_color_to_crossterm() {
        let c: CrosstermColor = Color::Red.into();
        assert_eq!(c, CrosstermColor::Red);
    }
}
