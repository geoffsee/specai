//! Terminal backend wrapping crossterm operations

use crate::buffer::{Buffer, Cell};
use crate::geometry::{Rect, Size};
use crate::style::Color;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{EnableBracketedPaste},
    execute, queue,
    style::{
        Attribute, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
    },
    terminal::{self, Clear, ClearType, EnterAlternateScreen, enable_raw_mode},
};
use std::io::{self, Stdout, Write};

use super::RawModeGuard;

/// Terminal backend wrapping crossterm operations
pub struct Terminal {
    stdout: Stdout,
    /// Current terminal size
    size: Size,
    /// Previous buffer for diff rendering
    prev_buffer: Option<Buffer>,
}

impl Terminal {
    /// Create a new terminal instance
    pub fn new() -> io::Result<Self> {
        let stdout = io::stdout();
        let (width, height) = terminal::size()?;
        Ok(Self {
            stdout,
            size: Size::new(width, height),
            prev_buffer: None,
        })
    }

    /// Enter raw mode with RAII guard
    ///
    /// This will:
    /// 1. Enable raw mode
    /// 2. Enter alternate screen
    /// 3. Hide the cursor
    /// 4. Enable bracketed paste mode
    ///
    /// The returned guard will cleanup when dropped.
    pub fn enter_raw_mode(&mut self) -> io::Result<RawModeGuard> {
        enable_raw_mode()?;
        execute!(self.stdout, EnterAlternateScreen, Hide, EnableBracketedPaste)?;
        Ok(RawModeGuard::new())
    }

    /// Get current terminal size
    pub fn size(&self) -> Size {
        self.size
    }

    /// Refresh size from terminal (call after resize event)
    pub fn refresh_size(&mut self) -> io::Result<()> {
        let (width, height) = terminal::size()?;
        self.size = Size::new(width, height);
        Ok(())
    }

    /// Get a full-screen rect
    pub fn full_rect(&self) -> Rect {
        Rect::sized(self.size.width, self.size.height)
    }

    /// Clear the screen
    pub fn clear(&mut self) -> io::Result<()> {
        execute!(self.stdout, Clear(ClearType::All))
    }

    /// Move cursor to position
    pub fn move_cursor(&mut self, x: u16, y: u16) -> io::Result<()> {
        execute!(self.stdout, MoveTo(x, y))
    }

    /// Show the cursor
    pub fn show_cursor(&mut self) -> io::Result<()> {
        execute!(self.stdout, Show)
    }

    /// Hide the cursor
    pub fn hide_cursor(&mut self) -> io::Result<()> {
        execute!(self.stdout, Hide)
    }

    /// Set cursor visibility
    pub fn set_cursor_visible(&mut self, visible: bool) -> io::Result<()> {
        if visible {
            self.show_cursor()
        } else {
            self.hide_cursor()
        }
    }

    /// Draw a single cell at position
    pub fn draw_cell(&mut self, x: u16, y: u16, cell: &Cell) -> io::Result<()> {
        queue!(self.stdout, MoveTo(x, y))?;

        // Set colors
        queue!(self.stdout, SetForegroundColor(cell.fg.into()))?;
        queue!(self.stdout, SetBackgroundColor(cell.bg.into()))?;

        // Set attributes
        if !cell.modifier.is_empty() {
            for attr in cell.modifier.attributes() {
                queue!(self.stdout, SetAttribute(attr))?;
            }
        }

        // Draw the symbol
        queue!(self.stdout, Print(&cell.symbol))?;

        // Reset attributes if we set any
        if !cell.modifier.is_empty() {
            queue!(self.stdout, SetAttribute(Attribute::Reset))?;
        }

        Ok(())
    }

    /// Flush all pending writes to the terminal
    pub fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }

    /// Draw an entire buffer with diff optimization
    ///
    /// Only cells that have changed since the last draw are written.
    pub fn draw(&mut self, buffer: &Buffer) -> io::Result<()> {
        let needs_full_draw = self.prev_buffer.is_none();

        if needs_full_draw {
            // Full draw on first render
            self.draw_full(buffer)?;
        } else {
            // Diff rendering - collect changed cells first to avoid borrow issues
            let prev = self.prev_buffer.as_ref().unwrap();
            let changes: Vec<_> = buffer.diff(prev)
                .map(|(x, y, cell)| (x, y, cell.clone()))
                .collect();

            for (x, y, cell) in changes {
                self.draw_cell(x, y, &cell)?;
            }
        }

        // Store buffer for next diff
        self.prev_buffer = Some(buffer.clone());
        self.flush()
    }

    /// Force a full redraw of the buffer (no diff)
    pub fn draw_full(&mut self, buffer: &Buffer) -> io::Result<()> {
        // Reset terminal state
        queue!(self.stdout, ResetColor)?;

        let mut last_style = (Color::Reset, Color::Reset);

        for (x, y, cell) in buffer.iter() {
            queue!(self.stdout, MoveTo(x, y))?;

            // Only change colors if needed
            let current_style = (cell.fg, cell.bg);
            if current_style != last_style {
                queue!(self.stdout, SetForegroundColor(cell.fg.into()))?;
                queue!(self.stdout, SetBackgroundColor(cell.bg.into()))?;
                last_style = current_style;
            }

            // Handle modifiers
            if !cell.modifier.is_empty() {
                for attr in cell.modifier.attributes() {
                    queue!(self.stdout, SetAttribute(attr))?;
                }
            }

            queue!(self.stdout, Print(&cell.symbol))?;

            // Reset if we had modifiers
            if !cell.modifier.is_empty() {
                queue!(self.stdout, SetAttribute(Attribute::Reset))?;
                last_style = (Color::Reset, Color::Reset); // Force color reset next time
            }
        }

        // Store for future diffs
        self.prev_buffer = Some(buffer.clone());
        self.flush()
    }

    /// Force clear the previous buffer, causing next draw to be a full redraw
    pub fn invalidate(&mut self) {
        self.prev_buffer = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_creation() {
        // This test requires a real terminal, so we just verify it compiles
        // Actual terminal tests would need to be run manually
    }
}
