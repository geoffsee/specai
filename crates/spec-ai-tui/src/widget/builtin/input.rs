//! Input widget for text entry

use crate::buffer::Buffer;
use crate::geometry::Rect;
use crate::style::{Color, Style};
use crate::widget::StatefulWidget;

/// State for input widget
#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// Current input text
    pub value: String,
    /// Cursor position (byte index)
    pub cursor: usize,
    /// Scroll offset for long text
    pub scroll: usize,
    /// Whether the input has focus
    pub focused: bool,
}

impl InputState {
    /// Create a new empty input state
    pub fn new() -> Self {
        Self {
            value: String::new(),
            cursor: 0,
            scroll: 0,
            focused: true,
        }
    }

    /// Create input state with initial value
    pub fn with_value<S: Into<String>>(value: S) -> Self {
        let value = value.into();
        let cursor = value.len();
        Self {
            value,
            cursor,
            scroll: 0,
            focused: true,
        }
    }

    /// Get the current value
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Set the value (and reset cursor to end)
    pub fn set_value<S: Into<String>>(&mut self, value: S) {
        self.value = value.into();
        self.cursor = self.value.len();
        self.scroll = 0;
    }

    /// Insert a character at the cursor position
    pub fn insert(&mut self, c: char) {
        self.value.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) {
        self.value.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            // Find the previous character boundary
            let prev = self.value[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.value.remove(prev);
            self.cursor = prev;
        }
    }

    /// Delete character at cursor (delete)
    pub fn delete(&mut self) {
        if self.cursor < self.value.len() {
            self.value.remove(self.cursor);
        }
    }

    /// Move cursor left by one character
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.value[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right by one character
    pub fn move_right(&mut self) {
        if self.cursor < self.value.len() {
            self.cursor = self.value[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.value.len());
        }
    }

    /// Move cursor to start of line
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end of line
    pub fn move_end(&mut self) {
        self.cursor = self.value.len();
    }

    /// Move cursor left by one word
    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        // Skip whitespace
        let before_cursor = &self.value[..self.cursor];
        let trimmed_end = before_cursor.trim_end();

        if trimmed_end.is_empty() {
            self.cursor = 0;
            return;
        }

        // Find word boundary
        self.cursor = trimmed_end
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
    }

    /// Move cursor right by one word
    pub fn move_word_right(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }

        let after_cursor = &self.value[self.cursor..];

        // Skip current word
        let skip_word = after_cursor
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after_cursor.len());

        // Skip whitespace
        let skip_space = after_cursor[skip_word..]
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(after_cursor.len() - skip_word);

        self.cursor = self.cursor + skip_word + skip_space;
    }

    /// Clear the input
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
        self.scroll = 0;
    }

    /// Take the value and clear the state
    pub fn take(&mut self) -> String {
        let value = std::mem::take(&mut self.value);
        self.cursor = 0;
        self.scroll = 0;
        value
    }

    /// Get the cursor position in characters (not bytes)
    pub fn cursor_char_pos(&self) -> usize {
        self.value[..self.cursor].chars().count()
    }
}

/// Input widget for text entry
#[derive(Debug, Clone, Default)]
pub struct Input {
    /// Base style for the input
    style: Style,
    /// Style for the cursor
    cursor_style: Style,
    /// Placeholder text when empty
    placeholder: Option<String>,
    /// Placeholder style
    placeholder_style: Style,
    /// Mask character for passwords
    mask: Option<char>,
}

impl Input {
    /// Create a new input widget
    pub fn new() -> Self {
        Self {
            style: Style::default(),
            cursor_style: Style::new().bg(Color::White).fg(Color::Black),
            placeholder: None,
            placeholder_style: Style::new().fg(Color::DarkGrey),
            mask: None,
        }
    }

    /// Set the base style
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the cursor style
    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }

    /// Set placeholder text
    pub fn placeholder<S: Into<String>>(mut self, placeholder: S) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Set placeholder style
    pub fn placeholder_style(mut self, style: Style) -> Self {
        self.placeholder_style = style;
        self
    }

    /// Enable password mode (mask with asterisks)
    pub fn password(mut self) -> Self {
        self.mask = Some('*');
        self
    }

    /// Set a custom mask character
    pub fn mask(mut self, c: char) -> Self {
        self.mask = Some(c);
        self
    }
}

impl StatefulWidget for Input {
    type State = InputState;

    fn render(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.is_empty() {
            return;
        }

        let width = area.width as usize;

        // Determine what to display
        let (display_text, display_style) = if state.value.is_empty() {
            // Show placeholder
            (
                self.placeholder.clone().unwrap_or_default(),
                self.placeholder_style,
            )
        } else if let Some(mask_char) = self.mask {
            // Show masked text
            (
                mask_char.to_string().repeat(state.value.chars().count()),
                self.style,
            )
        } else {
            // Show actual text
            (state.value.clone(), self.style)
        };

        // Calculate cursor position in characters
        let cursor_pos = if state.value.is_empty() {
            0
        } else if self.mask.is_some() {
            state.value[..state.cursor].chars().count()
        } else {
            unicode_width::UnicodeWidthStr::width(&state.value[..state.cursor])
        };

        // Adjust scroll to keep cursor visible
        if cursor_pos < state.scroll {
            state.scroll = cursor_pos;
        } else if cursor_pos >= state.scroll + width {
            state.scroll = cursor_pos - width + 1;
        }

        // Render the visible portion
        let display_chars: Vec<char> = display_text.chars().collect();
        let visible_start = state.scroll.min(display_chars.len());
        let visible_end = (state.scroll + width).min(display_chars.len());

        let mut x = area.x;
        for (i, c) in display_chars[visible_start..visible_end].iter().enumerate() {
            if x >= area.right() {
                break;
            }

            let char_pos = visible_start + i;
            let is_cursor = state.focused && !state.value.is_empty() && char_pos == cursor_pos;

            let style = if is_cursor {
                self.cursor_style
            } else {
                display_style
            };

            if let Some(cell) = buf.get_mut(x, area.y) {
                cell.symbol = c.to_string();
                cell.fg = style.fg;
                cell.bg = style.bg;
                cell.modifier = style.modifier;
            }

            let char_width = unicode_width::UnicodeWidthChar::width(*c).unwrap_or(1);
            x = x.saturating_add(char_width as u16);
        }

        // Draw cursor at end if cursor is at the end of text
        if state.focused && (state.value.is_empty() || cursor_pos >= display_chars.len()) {
            let cursor_x = area.x + (cursor_pos - state.scroll).min(width - 1) as u16;
            if cursor_x < area.right() {
                if let Some(cell) = buf.get_mut(cursor_x, area.y) {
                    if cell.symbol == " " || display_text.is_empty() {
                        cell.symbol = " ".to_string();
                    }
                    cell.fg = self.cursor_style.fg;
                    cell.bg = self.cursor_style.bg;
                    cell.modifier = self.cursor_style.modifier;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_state_new() {
        let state = InputState::new();
        assert!(state.value.is_empty());
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_input_state_insert() {
        let mut state = InputState::new();
        state.insert('H');
        state.insert('i');

        assert_eq!(state.value, "Hi");
        assert_eq!(state.cursor, 2);
    }

    #[test]
    fn test_input_state_backspace() {
        let mut state = InputState::with_value("Hello");
        state.backspace();

        assert_eq!(state.value, "Hell");
        assert_eq!(state.cursor, 4);
    }

    #[test]
    fn test_input_state_move() {
        let mut state = InputState::with_value("Hello");
        assert_eq!(state.cursor, 5);

        state.move_left();
        assert_eq!(state.cursor, 4);

        state.move_home();
        assert_eq!(state.cursor, 0);

        state.move_right();
        assert_eq!(state.cursor, 1);

        state.move_end();
        assert_eq!(state.cursor, 5);
    }

    #[test]
    fn test_input_state_unicode() {
        let mut state = InputState::with_value("日本語");
        assert_eq!(state.cursor, 9); // 3 chars * 3 bytes each

        state.move_left();
        assert_eq!(state.cursor, 6); // After "日本"

        state.move_left();
        assert_eq!(state.cursor, 3); // After "日"
    }

    #[test]
    fn test_input_state_take() {
        let mut state = InputState::with_value("Hello");
        let value = state.take();

        assert_eq!(value, "Hello");
        assert!(state.value.is_empty());
        assert_eq!(state.cursor, 0);
    }

    #[test]
    fn test_input_render() {
        let input = Input::new();
        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::new(area);
        let mut state = InputState::with_value("Hello");

        input.render(area, &mut buf, &mut state);

        assert_eq!(buf.get(0, 0).unwrap().symbol, "H");
        assert_eq!(buf.get(4, 0).unwrap().symbol, "o");
    }
}
