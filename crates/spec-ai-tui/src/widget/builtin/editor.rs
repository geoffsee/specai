//! Text editor widget with full editing support
//!
//! Implements standard text editing behavior across platforms:
//! - Selection with Shift+arrows
//! - Clipboard operations (copy, cut, paste)
//! - Undo/redo history
//! - Word-level navigation and deletion
//! - Platform-aware modifier keys (Cmd on macOS, Ctrl elsewhere)

use crate::buffer::Buffer;
use crate::event::{Event, KeyCode, KeyModifiers};
use crate::geometry::Rect;
use crate::style::{Color, Style};
use crate::widget::StatefulWidget;
use std::collections::VecDeque;

/// Detects if we're running on macOS
fn is_macos() -> bool {
    cfg!(target_os = "macos")
}

/// Check if the "command" modifier is pressed
/// In terminal apps, Cmd key isn't passed through - always use Ctrl
fn has_cmd_modifier(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL)
}

/// Check if the "word" modifier is pressed (Option/Alt on macOS, Ctrl elsewhere)
/// Option key works in terminals on macOS
fn has_word_modifier(modifiers: KeyModifiers) -> bool {
    if is_macos() {
        modifiers.contains(KeyModifiers::ALT)
    } else {
        modifiers.contains(KeyModifiers::CONTROL)
    }
}

/// Selection range in the text
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    /// Anchor position (where selection started)
    pub anchor: usize,
    /// Cursor position (where selection ends)
    pub cursor: usize,
}

impl Selection {
    /// Create a new selection
    pub fn new(anchor: usize, cursor: usize) -> Self {
        Self { anchor, cursor }
    }

    /// Create a collapsed selection (cursor only, no selection)
    pub fn cursor(pos: usize) -> Self {
        Self { anchor: pos, cursor: pos }
    }

    /// Check if there's an active selection
    pub fn is_empty(&self) -> bool {
        self.anchor == self.cursor
    }

    /// Get the start of the selection (lower bound)
    pub fn start(&self) -> usize {
        self.anchor.min(self.cursor)
    }

    /// Get the end of the selection (upper bound)
    pub fn end(&self) -> usize {
        self.anchor.max(self.cursor)
    }

    /// Get the length of the selection
    pub fn len(&self) -> usize {
        self.end() - self.start()
    }
}

/// Undo entry
#[derive(Debug, Clone)]
struct UndoEntry {
    text: String,
    selection: Selection,
}

/// State for the text editor
#[derive(Debug, Clone)]
pub struct EditorState {
    /// Current text content
    pub text: String,
    /// Current selection (cursor position and optional selection)
    pub selection: Selection,
    /// Scroll offset for long text
    pub scroll: usize,
    /// Whether the editor has focus
    pub focused: bool,
    /// Undo history
    undo_stack: VecDeque<UndoEntry>,
    /// Redo history
    redo_stack: VecDeque<UndoEntry>,
    /// Maximum undo history size
    max_undo: usize,
    /// Clipboard content (internal, since we can't access system clipboard easily)
    clipboard: String,
    /// Whether slash menu should be shown
    pub show_slash_menu: bool,
    /// Slash command being typed (after /)
    pub slash_query: String,
}

impl Default for EditorState {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorState {
    /// Create a new empty editor state
    pub fn new() -> Self {
        Self {
            text: String::new(),
            selection: Selection::cursor(0),
            scroll: 0,
            focused: true,
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_undo: 100,
            clipboard: String::new(),
            show_slash_menu: false,
            slash_query: String::new(),
        }
    }

    /// Create editor state with initial value
    pub fn with_value<S: Into<String>>(value: S) -> Self {
        let text = value.into();
        let cursor = text.len();
        Self {
            text,
            selection: Selection::cursor(cursor),
            ..Self::new()
        }
    }

    /// Get the current text
    pub fn value(&self) -> &str {
        &self.text
    }

    /// Get cursor position (byte index)
    pub fn cursor(&self) -> usize {
        self.selection.cursor
    }

    /// Check if there's an active selection
    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }

    /// Get the selected text
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.start()..self.selection.end()]
    }

    // ========== Undo/Redo ==========

    fn save_undo(&mut self) {
        let entry = UndoEntry {
            text: self.text.clone(),
            selection: self.selection,
        };
        self.undo_stack.push_back(entry);
        if self.undo_stack.len() > self.max_undo {
            self.undo_stack.pop_front();
        }
        self.redo_stack.clear();
    }

    /// Undo the last change
    pub fn undo(&mut self) {
        if let Some(entry) = self.undo_stack.pop_back() {
            // Save current state to redo
            self.redo_stack.push_back(UndoEntry {
                text: self.text.clone(),
                selection: self.selection,
            });
            self.text = entry.text;
            self.selection = entry.selection;
        }
    }

    /// Redo the last undone change
    pub fn redo(&mut self) {
        if let Some(entry) = self.redo_stack.pop_back() {
            self.undo_stack.push_back(UndoEntry {
                text: self.text.clone(),
                selection: self.selection,
            });
            self.text = entry.text;
            self.selection = entry.selection;
        }
    }

    // ========== Clipboard ==========

    /// Copy selected text to clipboard
    pub fn copy(&mut self) {
        if self.has_selection() {
            self.clipboard = self.selected_text().to_string();
        }
    }

    /// Cut selected text to clipboard
    pub fn cut(&mut self) {
        if self.has_selection() {
            self.copy();
            self.delete_selection();
        }
    }

    /// Paste from clipboard
    pub fn paste(&mut self) {
        if !self.clipboard.is_empty() {
            self.insert_str(&self.clipboard.clone());
        }
    }

    /// Get clipboard content
    pub fn clipboard(&self) -> &str {
        &self.clipboard
    }

    // ========== Selection ==========

    /// Select all text
    pub fn select_all(&mut self) {
        self.selection = Selection::new(0, self.text.len());
    }

    /// Collapse selection to cursor position
    pub fn collapse_selection(&mut self) {
        self.selection.anchor = self.selection.cursor;
    }

    /// Delete the selected text
    fn delete_selection(&mut self) {
        if self.has_selection() {
            self.save_undo();
            let start = self.selection.start();
            let end = self.selection.end();
            self.text.drain(start..end);
            self.selection = Selection::cursor(start);
            self.update_slash_state();
        }
    }

    // ========== Text Manipulation ==========

    /// Insert a character at cursor (replaces selection if any)
    pub fn insert(&mut self, c: char) {
        self.save_undo();

        // Delete selection first
        if self.has_selection() {
            let start = self.selection.start();
            let end = self.selection.end();
            self.text.drain(start..end);
            self.selection = Selection::cursor(start);
        }

        let pos = self.selection.cursor;
        self.text.insert(pos, c);
        self.selection = Selection::cursor(pos + c.len_utf8());

        self.update_slash_state();
    }

    /// Insert a string at cursor (replaces selection if any)
    pub fn insert_str(&mut self, s: &str) {
        self.save_undo();

        if self.has_selection() {
            let start = self.selection.start();
            let end = self.selection.end();
            self.text.drain(start..end);
            self.selection = Selection::cursor(start);
        }

        let pos = self.selection.cursor;
        self.text.insert_str(pos, s);
        self.selection = Selection::cursor(pos + s.len());

        self.update_slash_state();
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.has_selection() {
            self.delete_selection();
        } else if self.selection.cursor > 0 {
            self.save_undo();
            let prev = self.prev_char_boundary(self.selection.cursor);
            self.text.drain(prev..self.selection.cursor);
            self.selection = Selection::cursor(prev);
            self.update_slash_state();
        }
    }

    /// Delete character at cursor (delete)
    pub fn delete(&mut self) {
        if self.has_selection() {
            self.delete_selection();
        } else if self.selection.cursor < self.text.len() {
            self.save_undo();
            let next = self.next_char_boundary(self.selection.cursor);
            self.text.drain(self.selection.cursor..next);
            self.update_slash_state();
        }
    }

    /// Delete word before cursor
    pub fn delete_word_backward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
        } else {
            self.save_undo();
            let start = self.find_word_start(self.selection.cursor);
            self.text.drain(start..self.selection.cursor);
            self.selection = Selection::cursor(start);
            self.update_slash_state();
        }
    }

    /// Delete word after cursor
    pub fn delete_word_forward(&mut self) {
        if self.has_selection() {
            self.delete_selection();
        } else {
            self.save_undo();
            let end = self.find_word_end(self.selection.cursor);
            self.text.drain(self.selection.cursor..end);
            self.update_slash_state();
        }
    }

    /// Delete to start of line
    pub fn delete_to_start(&mut self) {
        if self.selection.cursor > 0 {
            self.save_undo();
            self.text.drain(0..self.selection.cursor);
            self.selection = Selection::cursor(0);
            self.update_slash_state();
        }
    }

    /// Delete to end of line
    pub fn delete_to_end(&mut self) {
        if self.selection.cursor < self.text.len() {
            self.save_undo();
            self.text.drain(self.selection.cursor..);
            self.update_slash_state();
        }
    }

    /// Clear all text
    pub fn clear(&mut self) {
        if !self.text.is_empty() {
            self.save_undo();
            self.text.clear();
            self.selection = Selection::cursor(0);
            self.scroll = 0;
            self.update_slash_state();
        }
    }

    /// Take the text and clear the editor
    pub fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        self.selection = Selection::cursor(0);
        self.scroll = 0;
        self.show_slash_menu = false;
        self.slash_query.clear();
        text
    }

    // ========== Movement ==========

    /// Move cursor left
    pub fn move_left(&mut self, extend_selection: bool) {
        if !extend_selection && self.has_selection() {
            // Collapse to start of selection
            self.selection = Selection::cursor(self.selection.start());
        } else if self.selection.cursor > 0 {
            let new_pos = self.prev_char_boundary(self.selection.cursor);
            if extend_selection {
                self.selection.cursor = new_pos;
            } else {
                self.selection = Selection::cursor(new_pos);
            }
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self, extend_selection: bool) {
        if !extend_selection && self.has_selection() {
            // Collapse to end of selection
            self.selection = Selection::cursor(self.selection.end());
        } else if self.selection.cursor < self.text.len() {
            let new_pos = self.next_char_boundary(self.selection.cursor);
            if extend_selection {
                self.selection.cursor = new_pos;
            } else {
                self.selection = Selection::cursor(new_pos);
            }
        }
    }

    /// Move cursor to start of line
    pub fn move_home(&mut self, extend_selection: bool) {
        if extend_selection {
            self.selection.cursor = 0;
        } else {
            self.selection = Selection::cursor(0);
        }
    }

    /// Move cursor to end of line
    pub fn move_end(&mut self, extend_selection: bool) {
        if extend_selection {
            self.selection.cursor = self.text.len();
        } else {
            self.selection = Selection::cursor(self.text.len());
        }
    }

    /// Move cursor left by one word
    pub fn move_word_left(&mut self, extend_selection: bool) {
        let new_pos = self.find_word_start(self.selection.cursor);
        if extend_selection {
            self.selection.cursor = new_pos;
        } else {
            self.selection = Selection::cursor(new_pos);
        }
    }

    /// Move cursor right by one word
    pub fn move_word_right(&mut self, extend_selection: bool) {
        let new_pos = self.find_word_end(self.selection.cursor);
        if extend_selection {
            self.selection.cursor = new_pos;
        } else {
            self.selection = Selection::cursor(new_pos);
        }
    }

    // ========== Slash Command ==========

    fn update_slash_state(&mut self) {
        // Find if we're in a slash command context
        if let Some(slash_pos) = self.text[..self.selection.cursor].rfind('/') {
            let after_slash = &self.text[slash_pos + 1..self.selection.cursor];
            // Only show menu if slash is at start or after whitespace
            let before_slash = &self.text[..slash_pos];
            if before_slash.is_empty() || before_slash.ends_with(char::is_whitespace) {
                // Check that the query doesn't contain spaces (single word command)
                if !after_slash.contains(' ') {
                    self.show_slash_menu = true;
                    self.slash_query = after_slash.to_string();
                    return;
                }
            }
        }
        self.show_slash_menu = false;
        self.slash_query.clear();
    }

    /// Close the slash menu
    pub fn close_slash_menu(&mut self) {
        self.show_slash_menu = false;
        self.slash_query.clear();
    }

    // ========== Helper Methods ==========

    fn prev_char_boundary(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        self.text[..pos]
            .char_indices()
            .last()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_char_boundary(&self, pos: usize) -> usize {
        if pos >= self.text.len() {
            return self.text.len();
        }
        self.text[pos..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| pos + i)
            .unwrap_or(self.text.len())
    }

    fn find_word_start(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }

        let before = &self.text[..pos];

        // Skip trailing whitespace
        let trimmed = before.trim_end();
        if trimmed.is_empty() {
            return 0;
        }

        // Find word boundary
        trimmed
            .rfind(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
            .map(|i| {
                // Move past the delimiter
                let boundary = &trimmed[i..];
                i + boundary.chars().next().map(|c| c.len_utf8()).unwrap_or(0)
            })
            .unwrap_or(0)
    }

    fn find_word_end(&self, pos: usize) -> usize {
        if pos >= self.text.len() {
            return self.text.len();
        }

        let after = &self.text[pos..];

        // Skip leading whitespace
        let trimmed_start = after.len() - after.trim_start().len();
        let trimmed = &after[trimmed_start..];

        if trimmed.is_empty() {
            return self.text.len();
        }

        // Find word boundary
        trimmed
            .find(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
            .map(|i| pos + trimmed_start + i)
            .unwrap_or(self.text.len())
    }

    /// Handle an event, returns an EditorAction
    pub fn handle_event(&mut self, event: &Event) -> EditorAction {
        match event {
            Event::Paste(text) => {
                return self.handle_paste(text);
            }
            Event::Key(key) => {
                return self.handle_key_inner(key);
            }
            _ => return EditorAction::Ignored,
        }
    }

    /// Handle pasted text - sanitize and insert
    fn handle_paste(&mut self, text: &str) -> EditorAction {
        if text.is_empty() {
            return EditorAction::Handled;
        }

        // Sanitize: replace newlines with spaces for single-line input
        let sanitized: String = text
            .chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .collect();

        // Collapse multiple spaces
        let collapsed = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");

        if !collapsed.is_empty() {
            self.insert_str(&collapsed);
        }

        EditorAction::Handled
    }

    /// Handle a key event (internal)
    fn handle_key_inner(&mut self, key: &crate::event::KeyEvent) -> EditorAction {

        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        let cmd = has_cmd_modifier(key.modifiers);
        let word = has_word_modifier(key.modifiers);

        match key.code {
            // Navigation
            KeyCode::Left => {
                if word {
                    self.move_word_left(shift);
                } else {
                    self.move_left(shift);
                }
                EditorAction::Handled
            }
            KeyCode::Right => {
                if word {
                    self.move_word_right(shift);
                } else {
                    self.move_right(shift);
                }
                EditorAction::Handled
            }
            KeyCode::Home => {
                self.move_home(shift);
                EditorAction::Handled
            }
            KeyCode::End => {
                self.move_end(shift);
                EditorAction::Handled
            }

            // Deletion
            KeyCode::Backspace => {
                if cmd {
                    self.delete_to_start();
                } else if word {
                    self.delete_word_backward();
                } else {
                    self.backspace();
                }
                EditorAction::Handled
            }
            KeyCode::Delete => {
                if cmd {
                    self.delete_to_end();
                } else if word {
                    self.delete_word_forward();
                } else {
                    self.delete();
                }
                EditorAction::Handled
            }

            // Shortcuts with Cmd/Ctrl
            KeyCode::Char('a') if cmd => {
                self.select_all();
                EditorAction::Handled
            }
            KeyCode::Char('c') if cmd => {
                self.copy();
                EditorAction::Handled
            }
            KeyCode::Char('x') if cmd => {
                self.cut();
                EditorAction::Handled
            }
            KeyCode::Char('v') if cmd => {
                self.paste();
                EditorAction::Handled
            }
            KeyCode::Char('z') if cmd && shift => {
                self.redo();
                EditorAction::Handled
            }
            KeyCode::Char('z') if cmd => {
                self.undo();
                EditorAction::Handled
            }
            KeyCode::Char('y') if cmd => {
                self.redo();
                EditorAction::Handled
            }

            // Word navigation via Alt+b / Alt+f (emacs/readline style)
            // macOS terminals often send these for Option+Arrow
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.move_word_left(shift);
                EditorAction::Handled
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.move_word_right(shift);
                EditorAction::Handled
            }
            // Alt+Backspace for delete word backward (also common on macOS)
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::ALT) => {
                self.delete_word_forward();
                EditorAction::Handled
            }

            // Character input
            KeyCode::Char(c) if !cmd && !key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT) => {
                self.insert(c);
                EditorAction::Handled
            }

            // Enter
            KeyCode::Enter => {
                if self.show_slash_menu {
                    EditorAction::SlashCommand(self.slash_query.clone())
                } else {
                    let text = self.take();
                    EditorAction::Submit(text)
                }
            }

            // Escape
            KeyCode::Esc => {
                if self.show_slash_menu {
                    self.close_slash_menu();
                    EditorAction::Handled
                } else if self.has_selection() {
                    self.collapse_selection();
                    EditorAction::Handled
                } else {
                    EditorAction::Escape
                }
            }

            // Tab
            KeyCode::Tab => {
                if self.show_slash_menu {
                    EditorAction::SlashMenuNext
                } else {
                    EditorAction::Ignored
                }
            }
            KeyCode::BackTab => {
                if self.show_slash_menu {
                    EditorAction::SlashMenuPrev
                } else {
                    EditorAction::Ignored
                }
            }

            _ => EditorAction::Ignored,
        }
    }
}

/// Result of handling a key in the editor
#[derive(Debug, Clone, PartialEq)]
pub enum EditorAction {
    /// Key was handled, no further action needed
    Handled,
    /// Key was not handled
    Ignored,
    /// User pressed Enter, submit the text
    Submit(String),
    /// User pressed Escape
    Escape,
    /// User selected a slash command
    SlashCommand(String),
    /// Navigate to next item in slash menu
    SlashMenuNext,
    /// Navigate to previous item in slash menu
    SlashMenuPrev,
}

/// Text editor widget
#[derive(Debug, Clone, Default)]
pub struct Editor {
    /// Base style for the editor
    style: Style,
    /// Style for selected text
    selection_style: Style,
    /// Style for cursor
    cursor_style: Style,
    /// Placeholder text when empty
    placeholder: Option<String>,
    /// Placeholder style
    placeholder_style: Style,
}

impl Editor {
    /// Create a new editor widget
    pub fn new() -> Self {
        Self {
            style: Style::default(),
            selection_style: Style::new().bg(Color::Blue).fg(Color::White),
            cursor_style: Style::new().bg(Color::White).fg(Color::Black),
            placeholder: None,
            placeholder_style: Style::new().fg(Color::DarkGrey),
        }
    }

    /// Set the base style
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the selection style
    pub fn selection_style(mut self, style: Style) -> Self {
        self.selection_style = style;
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
}

/// A wrapped character with its position info
struct WrappedChar {
    ch: char,
    byte_pos: usize,
    line: usize,
    col: usize,
}

impl StatefulWidget for Editor {
    type State = EditorState;

    fn render(&self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.is_empty() {
            return;
        }

        let width = area.width as usize;
        let height = area.height as usize;

        // Show placeholder if empty
        if state.text.is_empty() {
            if let Some(ref placeholder) = self.placeholder {
                let display: String = placeholder.chars().take(width).collect();
                buf.set_string(area.x, area.y, &display, self.placeholder_style);

                // Show cursor at start
                if state.focused {
                    if let Some(cell) = buf.get_mut(area.x, area.y) {
                        cell.bg = self.cursor_style.bg;
                        cell.fg = self.cursor_style.fg;
                    }
                }
            }
            return;
        }

        // Wrap text into lines
        let mut wrapped: Vec<WrappedChar> = Vec::new();
        let mut line = 0usize;
        let mut col = 0usize;
        let mut cursor_line = 0usize;
        let mut cursor_col = 0usize;

        for (byte_pos, c) in state.text.char_indices() {
            let char_width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);

            // Wrap if this char would exceed width
            if col + char_width > width && col > 0 {
                line += 1;
                col = 0;
            }

            // Track cursor position
            if byte_pos == state.selection.cursor {
                cursor_line = line;
                cursor_col = col;
            }

            wrapped.push(WrappedChar {
                ch: c,
                byte_pos,
                line,
                col,
            });

            col += char_width;
        }

        // Handle cursor at end of text
        if state.selection.cursor == state.text.len() {
            cursor_line = line;
            cursor_col = col;
            // Wrap cursor if at end of line
            if cursor_col >= width {
                cursor_line += 1;
                cursor_col = 0;
            }
        }

        let total_lines = line + 1;

        // Adjust vertical scroll to keep cursor visible
        if cursor_line < state.scroll {
            state.scroll = cursor_line;
        } else if cursor_line >= state.scroll + height {
            state.scroll = cursor_line - height + 1;
        }

        // Render visible lines
        for wc in &wrapped {
            if wc.line < state.scroll {
                continue;
            }
            let screen_line = wc.line - state.scroll;
            if screen_line >= height {
                break;
            }

            let y = area.y + screen_line as u16;
            let x = area.x + wc.col as u16;

            // Determine style based on selection
            let is_selected = state.has_selection()
                && wc.byte_pos >= state.selection.start()
                && wc.byte_pos < state.selection.end();
            let is_cursor = wc.byte_pos == state.selection.cursor;

            let style = if state.focused && is_cursor && !state.has_selection() {
                self.cursor_style
            } else if is_selected {
                self.selection_style
            } else {
                self.style
            };

            if let Some(cell) = buf.get_mut(x, y) {
                cell.symbol = wc.ch.to_string();
                cell.fg = style.fg;
                cell.bg = style.bg;
                cell.modifier = style.modifier;
            }
        }

        // Draw cursor at end if needed
        if state.focused && state.selection.cursor == state.text.len() && !state.has_selection() {
            if cursor_line >= state.scroll && cursor_line < state.scroll + height {
                let screen_line = cursor_line - state.scroll;
                let y = area.y + screen_line as u16;
                let x = area.x + cursor_col as u16;
                if x < area.right() {
                    if let Some(cell) = buf.get_mut(x, y) {
                        cell.symbol = " ".to_string();
                        cell.bg = self.cursor_style.bg;
                        cell.fg = self.cursor_style.fg;
                    }
                }
            }
        }

        // Draw scroll indicator if there's more content
        if total_lines > height {
            let indicator = format!("↕{}/{}", state.scroll + 1, total_lines);
            let indicator_x = area.right().saturating_sub(indicator.len() as u16);
            buf.set_string(indicator_x, area.y, &indicator, Style::new().fg(Color::DarkGrey));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_editor_state_new() {
        let state = EditorState::new();
        assert!(state.text.is_empty());
        assert_eq!(state.cursor(), 0);
        assert!(!state.has_selection());
    }

    #[test]
    fn test_editor_insert() {
        let mut state = EditorState::new();
        state.insert('H');
        state.insert('i');
        assert_eq!(state.value(), "Hi");
        assert_eq!(state.cursor(), 2);
    }

    #[test]
    fn test_editor_selection() {
        let mut state = EditorState::with_value("Hello World");
        state.select_all();
        assert!(state.has_selection());
        assert_eq!(state.selected_text(), "Hello World");
    }

    #[test]
    fn test_editor_undo_redo() {
        let mut state = EditorState::new();
        state.insert('A');
        state.insert('B');
        assert_eq!(state.value(), "AB");

        state.undo();
        assert_eq!(state.value(), "A");

        state.redo();
        assert_eq!(state.value(), "AB");
    }

    #[test]
    fn test_editor_copy_paste() {
        let mut state = EditorState::with_value("Hello");
        state.select_all();
        state.copy();
        state.move_end(false);
        state.paste();
        assert_eq!(state.value(), "HelloHello");
    }

    #[test]
    fn test_editor_word_navigation() {
        let mut state = EditorState::with_value("Hello World Test");
        state.move_home(false);

        state.move_word_right(false);
        assert!(state.cursor() > 0);

        state.move_word_left(false);
        assert_eq!(state.cursor(), 0);
    }

    #[test]
    fn test_slash_menu() {
        let mut state = EditorState::new();
        state.insert('/');
        assert!(state.show_slash_menu);
        assert_eq!(state.slash_query, "");

        state.insert('h');
        state.insert('e');
        state.insert('l');
        state.insert('p');
        assert!(state.show_slash_menu);
        assert_eq!(state.slash_query, "help");

        state.insert(' ');
        assert!(!state.show_slash_menu);
    }
}
