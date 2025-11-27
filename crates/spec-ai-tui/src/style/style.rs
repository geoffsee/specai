//! Style combining foreground, background, and modifiers

use super::{Color, Modifier};

/// A complete style with foreground, background, and modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Style {
    /// Foreground color
    pub fg: Color,
    /// Background color
    pub bg: Color,
    /// Style modifiers
    pub modifier: Modifier,
}

impl Style {
    /// Create a new default style
    pub const fn new() -> Self {
        Self {
            fg: Color::Reset,
            bg: Color::Reset,
            modifier: Modifier::NONE,
        }
    }

    /// Create a style with no color changes (reset)
    pub const fn reset() -> Self {
        Self::new()
    }

    /// Set foreground color
    pub const fn fg(mut self, color: Color) -> Self {
        self.fg = color;
        self
    }

    /// Set background color
    pub const fn bg(mut self, color: Color) -> Self {
        self.bg = color;
        self
    }

    /// Set modifier
    pub const fn modifier(mut self, modifier: Modifier) -> Self {
        self.modifier = modifier;
        self
    }

    /// Add bold modifier
    pub const fn bold(mut self) -> Self {
        self.modifier = self.modifier.union(Modifier::BOLD);
        self
    }

    /// Add dim modifier
    pub const fn dim(mut self) -> Self {
        self.modifier = self.modifier.union(Modifier::DIM);
        self
    }

    /// Add italic modifier
    pub const fn italic(mut self) -> Self {
        self.modifier = self.modifier.union(Modifier::ITALIC);
        self
    }

    /// Add underline modifier
    pub const fn underlined(mut self) -> Self {
        self.modifier = self.modifier.union(Modifier::UNDERLINED);
        self
    }

    /// Add strikethrough modifier
    pub const fn crossed_out(mut self) -> Self {
        self.modifier = self.modifier.union(Modifier::CROSSED_OUT);
        self
    }

    /// Add reversed modifier (swap fg/bg)
    pub const fn reversed(mut self) -> Self {
        self.modifier = self.modifier.union(Modifier::REVERSED);
        self
    }

    /// Combine this style with another, with other taking precedence
    /// for non-default values
    pub fn patch(self, other: Style) -> Self {
        Self {
            fg: if other.fg == Color::Reset {
                self.fg
            } else {
                other.fg
            },
            bg: if other.bg == Color::Reset {
                self.bg
            } else {
                other.bg
            },
            modifier: self.modifier | other.modifier,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_style_default() {
        let s = Style::new();
        assert_eq!(s.fg, Color::Reset);
        assert_eq!(s.bg, Color::Reset);
        assert!(s.modifier.is_empty());
    }

    #[test]
    fn test_style_builder() {
        let s = Style::new().fg(Color::Red).bg(Color::Blue).bold().italic();

        assert_eq!(s.fg, Color::Red);
        assert_eq!(s.bg, Color::Blue);
        assert!(s.modifier.contains(Modifier::BOLD));
        assert!(s.modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_style_patch() {
        let base = Style::new().fg(Color::Red).bold();
        let patch = Style::new().fg(Color::Blue);

        let combined = base.patch(patch);
        assert_eq!(combined.fg, Color::Blue); // Overridden
        assert!(combined.modifier.contains(Modifier::BOLD)); // Preserved
    }

    #[test]
    fn test_style_patch_reset_preserved() {
        let base = Style::new().fg(Color::Red);
        let patch = Style::new(); // All reset

        let combined = base.patch(patch);
        assert_eq!(combined.fg, Color::Red); // Reset doesn't override
    }
}
