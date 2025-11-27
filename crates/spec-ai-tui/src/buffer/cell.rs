//! A single terminal cell with content and style

use crate::style::{Color, Modifier, Style};

/// A single terminal cell with content and style
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The character(s) displayed (supports Unicode grapheme clusters)
    pub symbol: String,
    /// Foreground color
    pub fg: Color,
    /// Background color
    pub bg: Color,
    /// Style modifiers (bold, italic, etc.)
    pub modifier: Modifier,
}

impl Cell {
    /// Create a new empty cell (space character, default style)
    pub fn empty() -> Self {
        Self {
            symbol: " ".to_string(),
            fg: Color::Reset,
            bg: Color::Reset,
            modifier: Modifier::empty(),
        }
    }

    /// Create a cell with a single character
    pub fn new<S: Into<String>>(symbol: S) -> Self {
        Self {
            symbol: symbol.into(),
            fg: Color::Reset,
            bg: Color::Reset,
            modifier: Modifier::empty(),
        }
    }

    /// Set foreground color
    pub fn fg(mut self, color: Color) -> Self {
        self.fg = color;
        self
    }

    /// Set background color
    pub fn bg(mut self, color: Color) -> Self {
        self.bg = color;
        self
    }

    /// Set modifier
    pub fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// Apply a style to this cell
    pub fn style(mut self, style: Style) -> Self {
        self.fg = style.fg;
        self.bg = style.bg;
        self.modifier = style.modifier;
        self
    }

    /// Set the symbol
    pub fn set_symbol<S: Into<String>>(&mut self, symbol: S) {
        self.symbol = symbol.into();
    }

    /// Set the style from a Style struct
    pub fn set_style(&mut self, style: Style) {
        self.fg = style.fg;
        self.bg = style.bg;
        self.modifier = style.modifier;
    }

    /// Reset the cell to empty
    pub fn reset(&mut self) {
        self.symbol = " ".to_string();
        self.fg = Color::Reset;
        self.bg = Color::Reset;
        self.modifier = Modifier::empty();
    }

    /// Get the style as a Style struct
    pub fn get_style(&self) -> Style {
        Style {
            fg: self.fg,
            bg: self.bg,
            modifier: self.modifier,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::empty()
    }
}

impl From<char> for Cell {
    fn from(c: char) -> Self {
        Self::new(c.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_empty() {
        let cell = Cell::empty();
        assert_eq!(cell.symbol, " ");
        assert_eq!(cell.fg, Color::Reset);
        assert_eq!(cell.bg, Color::Reset);
        assert!(cell.modifier.is_empty());
    }

    #[test]
    fn test_cell_new() {
        let cell = Cell::new("X");
        assert_eq!(cell.symbol, "X");
    }

    #[test]
    fn test_cell_builder() {
        let cell = Cell::new("A")
            .fg(Color::Red)
            .bg(Color::Blue)
            .modifier(Modifier::BOLD);

        assert_eq!(cell.symbol, "A");
        assert_eq!(cell.fg, Color::Red);
        assert_eq!(cell.bg, Color::Blue);
        assert!(cell.modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_cell_reset() {
        let mut cell = Cell::new("X").fg(Color::Red);
        cell.reset();
        assert_eq!(cell.symbol, " ");
        assert_eq!(cell.fg, Color::Reset);
    }

    #[test]
    fn test_cell_style() {
        let style = Style::new().fg(Color::Green).bold();
        let cell = Cell::new("Y").style(style);

        assert_eq!(cell.fg, Color::Green);
        assert!(cell.modifier.contains(Modifier::BOLD));
    }
}
