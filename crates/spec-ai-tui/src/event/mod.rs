//! Event handling system

mod input;
mod event_loop;

pub use input::{Event, KeyEvent, MouseEvent, KeyCode, KeyModifiers};
pub use event_loop::EventLoop;

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
