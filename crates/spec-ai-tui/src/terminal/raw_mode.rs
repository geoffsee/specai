//! RAII guard for raw terminal mode

use crossterm::{
    cursor::Show,
    event::DisableBracketedPaste,
    execute,
    terminal::{disable_raw_mode, LeaveAlternateScreen},
};
use std::io::{self, stdout};

/// RAII guard for raw terminal mode
///
/// When this guard is dropped, it will:
/// 1. Disable raw mode
/// 2. Leave alternate screen
/// 3. Show the cursor
///
/// This ensures the terminal is properly restored even if the program panics.
pub struct RawModeGuard {
    // Private field to prevent construction outside this module
    _private: (),
}

impl RawModeGuard {
    /// Create a new raw mode guard
    ///
    /// This should only be called by Terminal::enter_raw_mode()
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // Best effort cleanup - ignore errors during drop
        let _ = cleanup_terminal();
    }
}

/// Cleanup the terminal state
fn cleanup_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), DisableBracketedPaste, LeaveAlternateScreen, Show)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    // Note: Terminal tests require special handling due to raw mode
    // These would typically be run manually or in integration tests
}
