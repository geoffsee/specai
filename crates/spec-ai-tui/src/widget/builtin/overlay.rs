//! Overlay widget for modal dialogs

use crate::buffer::Buffer;
use crate::geometry::Rect;
use crate::style::{Color, Style};
use crate::widget::Widget;

/// A centered overlay/modal dialog with rounded border
#[derive(Debug, Clone)]
pub struct Overlay {
    /// Title displayed centered on top border
    title: String,
    /// Border color
    border_color: Color,
    /// Background color
    bg_color: Color,
    /// Help text displayed at bottom
    help_text: Option<String>,
    /// Width as percentage of parent (0.0-1.0)
    width_pct: f32,
    /// Height as percentage of parent (0.0-1.0)
    height_pct: f32,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            title: String::new(),
            border_color: Color::Cyan,
            bg_color: Color::Rgb(20, 20, 30),
            help_text: None,
            width_pct: 0.6,
            height_pct: 0.6,
        }
    }
}

impl Overlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    pub fn bg_color(mut self, color: Color) -> Self {
        self.bg_color = color;
        self
    }

    pub fn help_text(mut self, text: impl Into<String>) -> Self {
        self.help_text = Some(text.into());
        self
    }

    pub fn dimensions(mut self, width_pct: f32, height_pct: f32) -> Self {
        self.width_pct = width_pct.clamp(0.1, 1.0);
        self.height_pct = height_pct.clamp(0.1, 1.0);
        self
    }

    /// Calculate the overlay area within parent
    pub fn area(&self, parent: Rect) -> Rect {
        let width = (parent.width as f32 * self.width_pct) as u16;
        let height = (parent.height as f32 * self.height_pct) as u16;
        let x = parent.x + (parent.width - width) / 2;
        let y = parent.y + (parent.height - height) / 2;
        Rect::new(x, y, width, height)
    }

    /// Get the inner content area (inside border, excluding help line)
    pub fn inner(&self, overlay_area: Rect) -> Rect {
        let help_lines = if self.help_text.is_some() { 2 } else { 0 };
        Rect::new(
            overlay_area.x + 2,
            overlay_area.y + 2,
            overlay_area.width.saturating_sub(4),
            overlay_area.height.saturating_sub(4 + help_lines),
        )
    }

    /// Render overlay and return the inner content area
    pub fn render_frame(&self, parent: Rect, buf: &mut Buffer) -> Rect {
        let area = self.area(parent);

        // Fill background
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                if let Some(cell) = buf.get_mut(x, y) {
                    cell.bg = self.bg_color;
                    cell.fg = Color::White;
                    cell.symbol = " ".to_string();
                }
            }
        }

        // Draw rounded border
        let border_style = Style::new().fg(self.border_color);
        buf.set_string(area.x, area.y, "╭", border_style);
        buf.set_string(area.right() - 1, area.y, "╮", border_style);
        buf.set_string(area.x, area.bottom() - 1, "╰", border_style);
        buf.set_string(area.right() - 1, area.bottom() - 1, "╯", border_style);

        for x in (area.x + 1)..(area.right() - 1) {
            buf.set_string(x, area.y, "─", border_style);
            buf.set_string(x, area.bottom() - 1, "─", border_style);
        }
        for y in (area.y + 1)..(area.bottom() - 1) {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.right() - 1, y, "│", border_style);
        }

        // Draw title
        if !self.title.is_empty() {
            let title = format!(" {} ", self.title);
            let title_x = area.x + (area.width.saturating_sub(title.len() as u16)) / 2;
            buf.set_string(
                title_x,
                area.y,
                &title,
                Style::new().fg(self.border_color).bold(),
            );
        }

        // Draw help text
        if let Some(ref help) = self.help_text {
            buf.set_string(
                area.x + 2,
                area.bottom() - 2,
                help,
                Style::new().fg(Color::DarkGrey),
            );
        }

        self.inner(area)
    }
}

impl Widget for Overlay {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.render_frame(area, buf);
    }
}
