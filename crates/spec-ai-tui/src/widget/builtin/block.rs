//! Block widget with optional border and title

use crate::buffer::Buffer;
use crate::geometry::Rect;
use crate::style::Style;
use crate::widget::Widget;

/// Border type for blocks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderType {
    /// No border
    #[default]
    None,
    /// Single line border (─ │ ┌ ┐ └ ┘)
    Single,
    /// Double line border (═ ║ ╔ ╗ ╚ ╝)
    Double,
    /// Rounded corners (─ │ ╭ ╮ ╰ ╯)
    Rounded,
    /// Heavy/thick border (━ ┃ ┏ ┓ ┗ ┛)
    Heavy,
}

impl BorderType {
    /// Get the border characters for this type
    fn chars(&self) -> BorderChars {
        match self {
            BorderType::None => BorderChars {
                top: ' ',
                bottom: ' ',
                left: ' ',
                right: ' ',
                top_left: ' ',
                top_right: ' ',
                bottom_left: ' ',
                bottom_right: ' ',
            },
            BorderType::Single => BorderChars {
                top: '─',
                bottom: '─',
                left: '│',
                right: '│',
                top_left: '┌',
                top_right: '┐',
                bottom_left: '└',
                bottom_right: '┘',
            },
            BorderType::Double => BorderChars {
                top: '═',
                bottom: '═',
                left: '║',
                right: '║',
                top_left: '╔',
                top_right: '╗',
                bottom_left: '╚',
                bottom_right: '╝',
            },
            BorderType::Rounded => BorderChars {
                top: '─',
                bottom: '─',
                left: '│',
                right: '│',
                top_left: '╭',
                top_right: '╮',
                bottom_left: '╰',
                bottom_right: '╯',
            },
            BorderType::Heavy => BorderChars {
                top: '━',
                bottom: '━',
                left: '┃',
                right: '┃',
                top_left: '┏',
                top_right: '┓',
                bottom_left: '┗',
                bottom_right: '┛',
            },
        }
    }
}

struct BorderChars {
    top: char,
    bottom: char,
    left: char,
    right: char,
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
}

/// Block widget with optional border and title
#[derive(Debug, Clone, Default)]
pub struct Block {
    /// Title to display at the top
    title: Option<String>,
    /// Title alignment
    title_alignment: TitleAlignment,
    /// Border type
    border_type: BorderType,
    /// Border style
    border_style: Style,
    /// Title style
    title_style: Style,
}

/// Title alignment within the block border
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TitleAlignment {
    #[default]
    Left,
    Center,
    Right,
}

impl Block {
    /// Create a new block with no border
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a block with a single line border
    pub fn bordered() -> Self {
        Self {
            border_type: BorderType::Single,
            ..Default::default()
        }
    }

    /// Set the title
    pub fn title<S: Into<String>>(mut self, title: S) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the title alignment
    pub fn title_alignment(mut self, alignment: TitleAlignment) -> Self {
        self.title_alignment = alignment;
        self
    }

    /// Set the border type
    pub fn border_type(mut self, border_type: BorderType) -> Self {
        self.border_type = border_type;
        self
    }

    /// Set the border style
    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Set the title style
    pub fn title_style(mut self, style: Style) -> Self {
        self.title_style = style;
        self
    }

    /// Calculate the inner area (area minus borders)
    pub fn inner(&self, area: Rect) -> Rect {
        if self.border_type == BorderType::None {
            area
        } else {
            area.inner(1)
        }
    }

    /// Check if this block has a border
    pub fn has_border(&self) -> bool {
        self.border_type != BorderType::None
    }
}

impl Widget for Block {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        if self.border_type == BorderType::None {
            return;
        }

        let chars = self.border_type.chars();

        // Draw corners
        buf.set_string(area.x, area.y, &chars.top_left.to_string(), self.border_style);
        buf.set_string(area.right() - 1, area.y, &chars.top_right.to_string(), self.border_style);
        buf.set_string(area.x, area.bottom() - 1, &chars.bottom_left.to_string(), self.border_style);
        buf.set_string(area.right() - 1, area.bottom() - 1, &chars.bottom_right.to_string(), self.border_style);

        // Draw top and bottom borders
        for x in (area.x + 1)..(area.right() - 1) {
            buf.set_string(x, area.y, &chars.top.to_string(), self.border_style);
            buf.set_string(x, area.bottom() - 1, &chars.bottom.to_string(), self.border_style);
        }

        // Draw left and right borders
        for y in (area.y + 1)..(area.bottom() - 1) {
            buf.set_string(area.x, y, &chars.left.to_string(), self.border_style);
            buf.set_string(area.right() - 1, y, &chars.right.to_string(), self.border_style);
        }

        // Draw title if present
        if let Some(ref title) = self.title {
            let max_width = area.width.saturating_sub(4) as usize; // Leave space for borders and padding
            let title_display: String = if title.len() > max_width {
                format!("{}…", &title[..max_width.saturating_sub(1)])
            } else {
                title.clone()
            };

            let title_x = match self.title_alignment {
                TitleAlignment::Left => area.x + 2,
                TitleAlignment::Center => {
                    area.x + (area.width.saturating_sub(title_display.len() as u16)) / 2
                }
                TitleAlignment::Right => {
                    area.right().saturating_sub(title_display.len() as u16 + 2)
                }
            };

            buf.set_string(title_x, area.y, &title_display, self.title_style);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;

    #[test]
    fn test_block_bordered() {
        let block = Block::bordered();
        assert!(block.has_border());
        assert_eq!(block.border_type, BorderType::Single);
    }

    #[test]
    fn test_block_inner() {
        let block = Block::bordered();
        let area = Rect::new(0, 0, 20, 10);
        let inner = block.inner(area);

        assert_eq!(inner, Rect::new(1, 1, 18, 8));
    }

    #[test]
    fn test_block_no_border_inner() {
        let block = Block::new();
        let area = Rect::new(0, 0, 20, 10);
        let inner = block.inner(area);

        assert_eq!(inner, area); // No change
    }

    #[test]
    fn test_block_render() {
        let block = Block::bordered()
            .title("Test")
            .border_style(Style::new().fg(Color::Blue));

        let area = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::new(area);

        block.render(area, &mut buf);

        // Check corners
        assert_eq!(buf.get(0, 0).unwrap().symbol, "┌");
        assert_eq!(buf.get(9, 0).unwrap().symbol, "┐");
        assert_eq!(buf.get(0, 4).unwrap().symbol, "└");
        assert_eq!(buf.get(9, 4).unwrap().symbol, "┘");
    }
}
