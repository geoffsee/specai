//! Paragraph widget for displaying text

use crate::buffer::Buffer;
use crate::geometry::Rect;
use crate::style::{Line, Style, Text};
use crate::widget::Widget;
use unicode_width::UnicodeWidthStr;

/// Text alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
}

/// Text wrapping mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wrap {
    /// No wrapping - text is clipped at the edge
    #[default]
    None,
    /// Wrap at word boundaries
    Word,
    /// Wrap at character boundaries
    Char,
}

/// Paragraph widget for displaying styled text
#[derive(Debug, Clone, Default)]
pub struct Paragraph {
    /// The text to display
    text: Text,
    /// Base style applied to all text
    style: Style,
    /// Text alignment
    alignment: Alignment,
    /// Text wrapping mode
    wrap: Wrap,
}

impl Paragraph {
    /// Create a new paragraph from text
    pub fn new(text: Text) -> Self {
        Self {
            text,
            style: Style::default(),
            alignment: Alignment::Left,
            wrap: Wrap::None,
        }
    }

    /// Create a paragraph from a raw string
    pub fn raw<S: AsRef<str>>(content: S) -> Self {
        Self::new(Text::raw(content))
    }

    /// Set the base style
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the alignment
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Set the wrap mode
    pub fn wrap(mut self, wrap: Wrap) -> Self {
        self.wrap = wrap;
        self
    }

    /// Wrap a line to fit within the given width
    fn wrap_line(&self, line: &Line, width: usize) -> Vec<Line> {
        match self.wrap {
            Wrap::None => vec![line.clone()],
            Wrap::Word => self.wrap_words(line, width),
            Wrap::Char => self.wrap_chars(line, width),
        }
    }

    /// Wrap at word boundaries
    fn wrap_words(&self, line: &Line, width: usize) -> Vec<Line> {
        if line.width() <= width {
            return vec![line.clone()];
        }

        let mut wrapped = Vec::new();
        let mut current_line = Line::empty();
        let mut current_width = 0usize;

        for span in &line.spans {
            let words: Vec<&str> = span.content.split_inclusive(|c: char| c.is_whitespace())
                .collect();

            for word in words {
                let word_width = UnicodeWidthStr::width(word);

                if current_width + word_width > width && current_width > 0 {
                    // Start a new line
                    wrapped.push(std::mem::take(&mut current_line));
                    current_width = 0;
                }

                // Add word to current line
                if word_width > 0 {
                    current_line.spans.push(crate::style::Span::styled(
                        word.to_string(),
                        span.style,
                    ));
                    current_width += word_width;
                }
            }
        }

        if !current_line.is_empty() {
            wrapped.push(current_line);
        }

        if wrapped.is_empty() {
            vec![Line::empty()]
        } else {
            wrapped
        }
    }

    /// Wrap at character boundaries
    fn wrap_chars(&self, line: &Line, width: usize) -> Vec<Line> {
        if line.width() <= width {
            return vec![line.clone()];
        }

        let mut wrapped = Vec::new();
        let mut current_line = Line::empty();
        let mut current_width = 0usize;

        for span in &line.spans {
            let mut current_span = String::new();

            for c in span.content.chars() {
                let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);

                if current_width + char_width > width && current_width > 0 {
                    // Save current span and start new line
                    if !current_span.is_empty() {
                        current_line.spans.push(crate::style::Span::styled(
                            std::mem::take(&mut current_span),
                            span.style,
                        ));
                    }
                    wrapped.push(std::mem::take(&mut current_line));
                    current_width = 0;
                }

                current_span.push(c);
                current_width += char_width;
            }

            if !current_span.is_empty() {
                current_line.spans.push(crate::style::Span::styled(
                    current_span,
                    span.style,
                ));
            }
        }

        if !current_line.is_empty() {
            wrapped.push(current_line);
        }

        if wrapped.is_empty() {
            vec![Line::empty()]
        } else {
            wrapped
        }
    }

    /// Calculate the x offset for alignment
    fn alignment_offset(&self, line_width: usize, area_width: u16) -> u16 {
        match self.alignment {
            Alignment::Left => 0,
            Alignment::Center => {
                (area_width as usize).saturating_sub(line_width) as u16 / 2
            }
            Alignment::Right => {
                (area_width as usize).saturating_sub(line_width) as u16
            }
        }
    }
}

impl Widget for Paragraph {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let width = area.width as usize;
        let mut y = area.y;

        for line in &self.text.lines {
            let wrapped_lines = self.wrap_line(line, width);

            for wrapped_line in wrapped_lines {
                if y >= area.bottom() {
                    return;
                }

                let line_width = wrapped_line.width();
                let x_offset = self.alignment_offset(line_width, area.width);
                let mut x = area.x + x_offset;

                for span in &wrapped_line.spans {
                    // Combine base style with span style
                    let combined_style = self.style.patch(span.style);

                    for c in span.content.chars() {
                        if x >= area.right() {
                            break;
                        }

                        if let Some(cell) = buf.get_mut(x, y) {
                            cell.symbol = c.to_string();
                            cell.fg = combined_style.fg;
                            cell.bg = combined_style.bg;
                            cell.modifier = combined_style.modifier;
                        }

                        let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                        x = x.saturating_add(char_width as u16);
                    }
                }

                y += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;

    #[test]
    fn test_paragraph_raw() {
        let para = Paragraph::raw("Hello, World!");
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::new(area);

        para.render(area, &mut buf);

        assert_eq!(buf.get(0, 0).unwrap().symbol, "H");
        assert_eq!(buf.get(7, 0).unwrap().symbol, "W");
    }

    #[test]
    fn test_paragraph_multiline() {
        let para = Paragraph::raw("Line 1\nLine 2");
        let area = Rect::new(0, 0, 20, 5);
        let mut buf = Buffer::new(area);

        para.render(area, &mut buf);

        assert_eq!(buf.get(0, 0).unwrap().symbol, "L");
        assert_eq!(buf.get(5, 0).unwrap().symbol, "1");
        assert_eq!(buf.get(0, 1).unwrap().symbol, "L");
        assert_eq!(buf.get(5, 1).unwrap().symbol, "2");
    }

    #[test]
    fn test_paragraph_center_alignment() {
        let para = Paragraph::raw("Hi").alignment(Alignment::Center);
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::new(area);

        para.render(area, &mut buf);

        // "Hi" is 2 chars, area is 10, so offset should be 4
        assert_eq!(buf.get(4, 0).unwrap().symbol, "H");
        assert_eq!(buf.get(5, 0).unwrap().symbol, "i");
    }

    #[test]
    fn test_paragraph_wrap_word() {
        let para = Paragraph::raw("Hello World Test")
            .wrap(Wrap::Word);
        let area = Rect::new(0, 0, 8, 5);
        let mut buf = Buffer::new(area);

        para.render(area, &mut buf);

        // "Hello " should be on first line, "World " on second, "Test" on third
        assert_eq!(buf.get(0, 0).unwrap().symbol, "H");
    }

    #[test]
    fn test_paragraph_style() {
        let para = Paragraph::raw("Test")
            .style(Style::new().fg(Color::Red));
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::new(area);

        para.render(area, &mut buf);

        assert_eq!(buf.get(0, 0).unwrap().fg, Color::Red);
    }
}
