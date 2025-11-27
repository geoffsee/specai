//! Input event types

// Re-export crossterm types with cleaner names
pub use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
    MouseEvent,
};

/// Unified input event type
#[derive(Debug, Clone)]
pub enum Event {
    /// Keyboard event
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Terminal resize event
    Resize {
        /// New width in columns
        width: u16,
        /// New height in rows
        height: u16,
    },
    /// Periodic tick event for animations/updates
    Tick,
    /// Focus gained
    FocusGained,
    /// Focus lost
    FocusLost,
    /// Paste event (bracketed paste mode)
    Paste(String),
}

impl Event {
    /// Check if this is a quit event (Ctrl+C or Ctrl+Q)
    pub fn is_quit(&self) -> bool {
        matches!(
            self,
            Event::Key(KeyEvent {
                code: KeyCode::Char('c') | KeyCode::Char('q'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL)
        )
    }

    /// Check if this is an Enter key press
    pub fn is_enter(&self) -> bool {
        matches!(
            self,
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                ..
            })
        )
    }

    /// Check if this is an Escape key press
    pub fn is_escape(&self) -> bool {
        matches!(
            self,
            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                ..
            })
        )
    }

    /// Check if this is a Tab key press (forward focus)
    pub fn is_tab(&self) -> bool {
        matches!(
            self,
            Event::Key(KeyEvent {
                code: KeyCode::Tab,
                modifiers,
                ..
            }) if !modifiers.contains(KeyModifiers::SHIFT)
        )
    }

    /// Check if this is a Shift+Tab (backward focus)
    pub fn is_backtab(&self) -> bool {
        match self {
            Event::Key(KeyEvent { code: KeyCode::BackTab, .. }) => true,
            Event::Key(KeyEvent { code: KeyCode::Tab, modifiers, .. })
                if modifiers.contains(KeyModifiers::SHIFT) => true,
            _ => false,
        }
    }

    /// Check if this is a resize event
    pub fn is_resize(&self) -> bool {
        matches!(self, Event::Resize { .. })
    }

    /// Get the key event if this is a key event
    pub fn as_key(&self) -> Option<&KeyEvent> {
        match self {
            Event::Key(key) => Some(key),
            _ => None,
        }
    }

    /// Get the character if this is a character key press
    pub fn as_char(&self) -> Option<char> {
        match self {
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers,
                ..
            }) if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) => Some(*c),
            _ => None,
        }
    }
}

impl From<crossterm::event::Event> for Event {
    fn from(event: crossterm::event::Event) -> Self {
        use crossterm::event::Event as CEvent;
        match event {
            CEvent::Key(key) => Event::Key(key),
            CEvent::Mouse(mouse) => Event::Mouse(mouse),
            CEvent::Resize(w, h) => Event::Resize { width: w, height: h },
            CEvent::FocusGained => Event::FocusGained,
            CEvent::FocusLost => Event::FocusLost,
            CEvent::Paste(s) => Event::Paste(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_quit() {
        let ctrl_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(ctrl_c.is_quit());

        let ctrl_q = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert!(ctrl_q.is_quit());

        let just_c = Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert!(!just_c.is_quit());
    }

    #[test]
    fn test_is_enter() {
        let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(enter.is_enter());

        let space = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(!space.is_enter());
    }

    #[test]
    fn test_as_char() {
        let a = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(a.as_char(), Some('a'));

        let shift_a = Event::Key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
        assert_eq!(shift_a.as_char(), Some('A'));

        // Ctrl+A should not be a char
        let ctrl_a = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL));
        assert_eq!(ctrl_a.as_char(), None);

        let enter = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(enter.as_char(), None);
    }
}
