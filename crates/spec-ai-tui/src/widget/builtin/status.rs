//! Status bar widget

use crate::buffer::Buffer;
use crate::geometry::Rect;
use crate::style::{Color, Style};
use crate::widget::Widget;

/// A section in the status bar
#[derive(Debug, Clone)]
pub struct StatusSection {
    /// The text content
    pub content: String,
    /// The style for this section
    pub style: Style,
}

impl StatusSection {
    /// Create a new status section
    pub fn new<S: Into<String>>(content: S) -> Self {
        Self {
            content: content.into(),
            style: Style::default(),
        }
    }

    /// Set the style
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Get the width of this section
    pub fn width(&self) -> usize {
        unicode_width::UnicodeWidthStr::width(self.content.as_str())
    }
}

impl<S: Into<String>> From<S> for StatusSection {
    fn from(s: S) -> Self {
        Self::new(s)
    }
}

/// Status bar widget with left, center, and right sections
#[derive(Debug, Clone, Default)]
pub struct StatusBar {
    /// Left-aligned sections
    left: Vec<StatusSection>,
    /// Center-aligned sections
    center: Vec<StatusSection>,
    /// Right-aligned sections
    right: Vec<StatusSection>,
    /// Base style for the entire bar
    style: Style,
    /// Separator between sections
    separator: String,
}

impl StatusBar {
    /// Create a new empty status bar
    pub fn new() -> Self {
        Self {
            left: Vec::new(),
            center: Vec::new(),
            right: Vec::new(),
            style: Style::new().bg(Color::DarkGrey).fg(Color::White),
            separator: " │ ".to_string(),
        }
    }

    /// Set left-aligned sections
    pub fn left<I: IntoIterator<Item = StatusSection>>(mut self, sections: I) -> Self {
        self.left = sections.into_iter().collect();
        self
    }

    /// Set center-aligned sections
    pub fn center<I: IntoIterator<Item = StatusSection>>(mut self, sections: I) -> Self {
        self.center = sections.into_iter().collect();
        self
    }

    /// Set right-aligned sections
    pub fn right<I: IntoIterator<Item = StatusSection>>(mut self, sections: I) -> Self {
        self.right = sections.into_iter().collect();
        self
    }

    /// Set the base style
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the separator string
    pub fn separator<S: Into<String>>(mut self, separator: S) -> Self {
        self.separator = separator.into();
        self
    }

    /// Render sections and return the ending x position
    fn render_sections(
        &self,
        sections: &[StatusSection],
        x: u16,
        y: u16,
        buf: &mut Buffer,
        max_x: u16,
    ) -> u16 {
        let mut current_x = x;
        let sep_width = unicode_width::UnicodeWidthStr::width(self.separator.as_str()) as u16;

        for (i, section) in sections.iter().enumerate() {
            // Add separator before non-first sections
            if i > 0 {
                buf.set_string(current_x, y, &self.separator, self.style);
                current_x = current_x.saturating_add(sep_width);
            }

            if current_x >= max_x {
                break;
            }

            // Combine base style with section style
            let combined_style = self.style.patch(section.style);

            // Render section content
            for c in section.content.chars() {
                if current_x >= max_x {
                    break;
                }

                if let Some(cell) = buf.get_mut(current_x, y) {
                    cell.symbol = c.to_string();
                    cell.fg = combined_style.fg;
                    cell.bg = combined_style.bg;
                    cell.modifier = combined_style.modifier;
                }

                let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                current_x = current_x.saturating_add(char_width as u16);
            }
        }

        current_x
    }

    /// Calculate total width of sections including separators
    fn sections_width(&self, sections: &[StatusSection]) -> usize {
        if sections.is_empty() {
            return 0;
        }

        let content_width: usize = sections.iter().map(|s| s.width()).sum();
        let sep_width = unicode_width::UnicodeWidthStr::width(self.separator.as_str());
        let separators = sections.len().saturating_sub(1);

        content_width + (sep_width * separators)
    }
}

impl Widget for StatusBar {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || area.height == 0 {
            return;
        }

        // Fill background
        for x in area.x..area.right() {
            if let Some(cell) = buf.get_mut(x, area.y) {
                cell.symbol = " ".to_string();
                cell.fg = self.style.fg;
                cell.bg = self.style.bg;
                cell.modifier = self.style.modifier;
            }
        }

        // Render left sections
        self.render_sections(&self.left, area.x, area.y, buf, area.right());

        // Render right sections (right-aligned)
        let right_width = self.sections_width(&self.right) as u16;
        let right_x = area.right().saturating_sub(right_width);
        self.render_sections(&self.right, right_x, area.y, buf, area.right());

        // Render center sections (centered)
        let center_width = self.sections_width(&self.center) as u16;
        let center_x = area.x + (area.width.saturating_sub(center_width)) / 2;
        self.render_sections(&self.center, center_x, area.y, buf, area.right());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_section() {
        let section = StatusSection::new("Test").style(Style::new().fg(Color::Red));
        assert_eq!(section.content, "Test");
        assert_eq!(section.width(), 4);
    }

    #[test]
    fn test_status_bar_left() {
        let bar = StatusBar::new().left([StatusSection::new("Left")]);

        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::new(area);

        bar.render(area, &mut buf);

        assert_eq!(buf.get(0, 0).unwrap().symbol, "L");
        assert_eq!(buf.get(3, 0).unwrap().symbol, "t");
    }

    #[test]
    fn test_status_bar_right() {
        let bar = StatusBar::new().right([StatusSection::new("Right")]);

        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::new(area);

        bar.render(area, &mut buf);

        // "Right" is 5 chars, so starts at 15 (20 - 5)
        assert_eq!(buf.get(15, 0).unwrap().symbol, "R");
        assert_eq!(buf.get(19, 0).unwrap().symbol, "t");
    }

    #[test]
    fn test_status_bar_multiple_sections() {
        let bar = StatusBar::new().left([StatusSection::new("A"), StatusSection::new("B")]);

        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::new(area);

        bar.render(area, &mut buf);

        // "A │ B"
        assert_eq!(buf.get(0, 0).unwrap().symbol, "A");
        // Separator starts at 1
        assert_eq!(buf.get(4, 0).unwrap().symbol, "B");
    }

    #[test]
    fn test_status_bar_style() {
        let bar = StatusBar::new()
            .style(Style::new().bg(Color::Blue).fg(Color::White))
            .left([StatusSection::new("X")]);

        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::new(area);

        bar.render(area, &mut buf);

        assert_eq!(buf.get(0, 0).unwrap().bg, Color::Blue);
        assert_eq!(buf.get(0, 0).unwrap().fg, Color::White);
    }
}
