//! Styled text types: Span, Line, and Text

use super::Style;
use unicode_width::UnicodeWidthStr;

/// A span of styled text (single style, single string)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Span {
    /// The text content
    pub content: String,
    /// The style applied to this span
    pub style: Style,
}

impl Span {
    /// Create a new span with default style
    pub fn raw<S: Into<String>>(content: S) -> Self {
        Self {
            content: content.into(),
            style: Style::default(),
        }
    }

    /// Create a new span with a specific style
    pub fn styled<S: Into<String>>(content: S, style: Style) -> Self {
        Self {
            content: content.into(),
            style,
        }
    }

    /// Get the display width of this span in terminal cells
    pub fn width(&self) -> usize {
        UnicodeWidthStr::width(self.content.as_str())
    }

    /// Check if the span is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Set the style
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

impl<S: Into<String>> From<S> for Span {
    fn from(s: S) -> Self {
        Self::raw(s)
    }
}

/// A line of styled text (multiple spans)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    /// The spans that make up this line
    pub spans: Vec<Span>,
}

impl Line {
    /// Create an empty line
    pub fn empty() -> Self {
        Self { spans: Vec::new() }
    }

    /// Create a line from a single raw string
    pub fn raw<S: Into<String>>(content: S) -> Self {
        Self {
            spans: vec![Span::raw(content)],
        }
    }

    /// Create a line from a single styled string
    pub fn styled<S: Into<String>>(content: S, style: Style) -> Self {
        Self {
            spans: vec![Span::styled(content, style)],
        }
    }

    /// Create a line from multiple spans
    pub fn from_spans<I: IntoIterator<Item = Span>>(spans: I) -> Self {
        Self {
            spans: spans.into_iter().collect(),
        }
    }

    /// Get the total display width of this line
    pub fn width(&self) -> usize {
        self.spans.iter().map(|s| s.width()).sum()
    }

    /// Check if the line is empty
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty() || self.spans.iter().all(|s| s.is_empty())
    }

    /// Add a span to this line
    pub fn push(&mut self, span: Span) {
        self.spans.push(span);
    }

    /// Apply a style to all spans (patched with existing styles)
    pub fn style(mut self, style: Style) -> Self {
        for span in &mut self.spans {
            span.style = style.patch(span.style);
        }
        self
    }

    /// Iterate over characters with their styles
    pub fn styled_chars(&self) -> impl Iterator<Item = (char, Style)> + '_ {
        self.spans
            .iter()
            .flat_map(|span| span.content.chars().map(move |c| (c, span.style)))
    }
}

impl<S: Into<String>> From<S> for Line {
    fn from(s: S) -> Self {
        Self::raw(s)
    }
}

impl FromIterator<Span> for Line {
    fn from_iter<I: IntoIterator<Item = Span>>(iter: I) -> Self {
        Self::from_spans(iter)
    }
}

/// Multiple lines of styled text
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Text {
    /// The lines
    pub lines: Vec<Line>,
}

impl Text {
    /// Create empty text
    pub fn empty() -> Self {
        Self { lines: Vec::new() }
    }

    /// Create text from a raw string (splits on newlines)
    pub fn raw<S: AsRef<str>>(content: S) -> Self {
        Self {
            lines: content
                .as_ref()
                .lines()
                .map(|l| Line::raw(l.to_string()))
                .collect(),
        }
    }

    /// Create text from a styled string (splits on newlines)
    pub fn styled<S: AsRef<str>>(content: S, style: Style) -> Self {
        Self {
            lines: content
                .as_ref()
                .lines()
                .map(|l| Line::styled(l.to_string(), style))
                .collect(),
        }
    }

    /// Create text from multiple lines
    pub fn from_lines<I: IntoIterator<Item = Line>>(lines: I) -> Self {
        Self {
            lines: lines.into_iter().collect(),
        }
    }

    /// Get the height in lines
    pub fn height(&self) -> usize {
        self.lines.len()
    }

    /// Get the maximum width of any line
    pub fn width(&self) -> usize {
        self.lines.iter().map(|l| l.width()).max().unwrap_or(0)
    }

    /// Check if the text is empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() || self.lines.iter().all(|l| l.is_empty())
    }

    /// Add a line
    pub fn push(&mut self, line: Line) {
        self.lines.push(line);
    }

    /// Apply a style to all lines
    pub fn style(mut self, style: Style) -> Self {
        self.lines = self.lines.into_iter().map(|l| l.style(style)).collect();
        self
    }
}

impl<S: AsRef<str>> From<S> for Text {
    fn from(s: S) -> Self {
        Self::raw(s)
    }
}

impl FromIterator<Line> for Text {
    fn from_iter<I: IntoIterator<Item = Line>>(iter: I) -> Self {
        Self::from_lines(iter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;

    #[test]
    fn test_span_width() {
        let s = Span::raw("hello");
        assert_eq!(s.width(), 5);

        // Unicode width
        let s = Span::raw("日本語"); // 3 characters, 6 cells wide
        assert_eq!(s.width(), 6);
    }

    #[test]
    fn test_line_width() {
        let line = Line::from_spans(vec![
            Span::raw("hello"),
            Span::raw(" "),
            Span::raw("world"),
        ]);
        assert_eq!(line.width(), 11);
    }

    #[test]
    fn test_text_raw() {
        let text = Text::raw("line1\nline2\nline3");
        assert_eq!(text.height(), 3);
        assert_eq!(text.lines[0].spans[0].content, "line1");
        assert_eq!(text.lines[1].spans[0].content, "line2");
        assert_eq!(text.lines[2].spans[0].content, "line3");
    }

    #[test]
    fn test_line_style() {
        let line = Line::raw("test").style(Style::new().fg(Color::Red));
        assert_eq!(line.spans[0].style.fg, Color::Red);
    }

    #[test]
    fn test_styled_chars() {
        let line = Line::from_spans(vec![
            Span::styled("ab", Style::new().fg(Color::Red)),
            Span::styled("cd", Style::new().fg(Color::Blue)),
        ]);

        let chars: Vec<_> = line.styled_chars().collect();
        assert_eq!(chars.len(), 4);
        assert_eq!(chars[0], ('a', Style::new().fg(Color::Red)));
        assert_eq!(chars[1], ('b', Style::new().fg(Color::Red)));
        assert_eq!(chars[2], ('c', Style::new().fg(Color::Blue)));
        assert_eq!(chars[3], ('d', Style::new().fg(Color::Blue)));
    }
}
