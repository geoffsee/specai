//! Event handling system

mod event_loop;
mod input;

pub use event_loop::EventLoop;
pub use input::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent};

/// Result of handling an event
#[derive(Debug, Clone)]
pub enum EventResult {
    /// Event was consumed and processed
    Consumed,
    /// Event was ignored, should propagate
    Ignored,
    /// Request to quit the application
    Quit,
}

impl EventResult {
    /// Check if the event was consumed
    pub fn is_consumed(&self) -> bool {
        matches!(self, EventResult::Consumed)
    }

    /// Check if quit was requested
    pub fn is_quit(&self) -> bool {
        matches!(self, EventResult::Quit)
    }
}
