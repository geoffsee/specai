//! 2D buffer of cells for rendering

use super::Cell;
use crate::geometry::Rect;
use crate::style::{Line, Span, Style};

/// 2D buffer of cells for rendering
#[derive(Debug, Clone)]
pub struct Buffer {
    /// The area this buffer represents
    area: Rect,
    /// Flat array of cells (row-major order)
    cells: Vec<Cell>,
}

impl Buffer {
    /// Create a new buffer for the given area
    pub fn new(area: Rect) -> Self {
        let size = area.area() as usize;
        Self {
            area,
            cells: vec![Cell::empty(); size],
        }
    }

    /// Create a buffer filled with a specific cell
    pub fn filled(area: Rect, cell: Cell) -> Self {
        let size = area.area() as usize;
        Self {
            area,
            cells: vec![cell; size],
        }
    }

    /// Get the buffer area
    pub fn area(&self) -> Rect {
        self.area
    }

    /// Convert absolute (x, y) to index in the cells array
    fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.area.x
            && x < self.area.right()
            && y >= self.area.y
            && y < self.area.bottom()
        {
            let local_x = (x - self.area.x) as usize;
            let local_y = (y - self.area.y) as usize;
            Some(local_y * self.area.width as usize + local_x)
        } else {
            None
        }
    }

    /// Get a cell at position (returns None if out of bounds)
    pub fn get(&self, x: u16, y: u16) -> Option<&Cell> {
        self.index(x, y).map(|i| &self.cells[i])
    }

    /// Get a mutable cell at position (returns None if out of bounds)
    pub fn get_mut(&mut self, x: u16, y: u16) -> Option<&mut Cell> {
        self.index(x, y).map(|i| &mut self.cells[i])
    }

    /// Set a cell at position (does nothing if out of bounds)
    pub fn set(&mut self, x: u16, y: u16, cell: Cell) {
        if let Some(idx) = self.index(x, y) {
            self.cells[idx] = cell;
        }
    }

    /// Set just the symbol at a position
    pub fn set_symbol(&mut self, x: u16, y: u16, symbol: &str) {
        if let Some(cell) = self.get_mut(x, y) {
            cell.symbol = symbol.to_string();
        }
    }

    /// Set a string starting at position with the given style
    pub fn set_string(&mut self, x: u16, y: u16, s: &str, style: Style) {
        let mut current_x = x;
        for c in s.chars() {
            if current_x >= self.area.right() {
                break;
            }
            if let Some(cell) = self.get_mut(current_x, y) {
                cell.symbol = c.to_string();
                cell.fg = style.fg;
                cell.bg = style.bg;
                cell.modifier = style.modifier;
            }
            // Handle wide characters
            let width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
            current_x = current_x.saturating_add(width as u16);
        }
    }

    /// Set a string with default style
    pub fn set_string_raw(&mut self, x: u16, y: u16, s: &str) {
        self.set_string(x, y, s, Style::default());
    }

    /// Set a span (styled string)
    pub fn set_span(&mut self, x: u16, y: u16, span: &Span) {
        self.set_string(x, y, &span.content, span.style);
    }

    /// Set a line (multiple spans)
    pub fn set_line(&mut self, x: u16, y: u16, line: &Line) {
        let mut current_x = x;
        for span in &line.spans {
            self.set_span(current_x, y, span);
            current_x = current_x.saturating_add(span.width() as u16);
            if current_x >= self.area.right() {
                break;
            }
        }
    }

    /// Fill an area with a cell
    pub fn fill(&mut self, area: Rect, cell: Cell) {
        let clipped = self.area.intersect(&area);
        for y in clipped.y..clipped.bottom() {
            for x in clipped.x..clipped.right() {
                self.set(x, y, cell.clone());
            }
        }
    }

    /// Fill an area with a specific style (preserves symbols)
    pub fn fill_style(&mut self, area: Rect, style: Style) {
        let clipped = self.area.intersect(&area);
        for y in clipped.y..clipped.bottom() {
            for x in clipped.x..clipped.right() {
                if let Some(cell) = self.get_mut(x, y) {
                    cell.fg = style.fg;
                    cell.bg = style.bg;
                    cell.modifier = style.modifier;
                }
            }
        }
    }

    /// Clear the entire buffer (reset all cells to empty)
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.reset();
        }
    }

    /// Clear an area within the buffer
    pub fn clear_area(&mut self, area: Rect) {
        self.fill(area, Cell::empty());
    }

    /// Iterate over all cells with their positions
    pub fn iter(&self) -> impl Iterator<Item = (u16, u16, &Cell)> {
        self.cells.iter().enumerate().map(move |(i, cell)| {
            let x = self.area.x + (i % self.area.width as usize) as u16;
            let y = self.area.y + (i / self.area.width as usize) as u16;
            (x, y, cell)
        })
    }

    /// Iterate over cells that differ from another buffer
    pub fn diff<'a>(&'a self, other: &'a Buffer) -> impl Iterator<Item = (u16, u16, &'a Cell)> {
        self.iter()
            .filter(move |(x, y, cell)| {
                other.get(*x, *y).map(|c| c != *cell).unwrap_or(true)
            })
    }

    /// Merge another buffer into this one at its position
    pub fn merge(&mut self, other: &Buffer) {
        for (x, y, cell) in other.iter() {
            self.set(x, y, cell.clone());
        }
    }

    /// Resize the buffer to a new area
    pub fn resize(&mut self, area: Rect) {
        let new_size = area.area() as usize;
        let mut new_cells = vec![Cell::empty(); new_size];

        // Copy existing cells that fit in the new area
        let copy_area = self.area.intersect(&area);
        for y in copy_area.y..copy_area.bottom() {
            for x in copy_area.x..copy_area.right() {
                if let Some(old_cell) = self.get(x, y) {
                    let local_x = (x - area.x) as usize;
                    let local_y = (y - area.y) as usize;
                    let idx = local_y * area.width as usize + local_x;
                    if idx < new_cells.len() {
                        new_cells[idx] = old_cell.clone();
                    }
                }
            }
        }

        self.area = area;
        self.cells = new_cells;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Color;

    #[test]
    fn test_buffer_creation() {
        let area = Rect::new(0, 0, 10, 5);
        let buf = Buffer::new(area);
        assert_eq!(buf.area(), area);
        assert_eq!(buf.cells.len(), 50);
    }

    #[test]
    fn test_buffer_get_set() {
        let area = Rect::new(0, 0, 10, 5);
        let mut buf = Buffer::new(area);

        buf.set(5, 2, Cell::new("X").fg(Color::Red));

        let cell = buf.get(5, 2).unwrap();
        assert_eq!(cell.symbol, "X");
        assert_eq!(cell.fg, Color::Red);
    }

    #[test]
    fn test_buffer_bounds() {
        let area = Rect::new(5, 5, 10, 10);
        let buf = Buffer::new(area);

        // Within bounds
        assert!(buf.get(5, 5).is_some());
        assert!(buf.get(14, 14).is_some());

        // Out of bounds
        assert!(buf.get(4, 5).is_none());
        assert!(buf.get(15, 5).is_none());
    }

    #[test]
    fn test_buffer_set_string() {
        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::new(area);

        buf.set_string(0, 0, "Hello", Style::new().fg(Color::Green));

        assert_eq!(buf.get(0, 0).unwrap().symbol, "H");
        assert_eq!(buf.get(4, 0).unwrap().symbol, "o");
        assert_eq!(buf.get(5, 0).unwrap().symbol, " "); // After the string
    }

    #[test]
    fn test_buffer_fill() {
        let area = Rect::new(0, 0, 5, 5);
        let mut buf = Buffer::new(area);

        let fill_area = Rect::new(1, 1, 3, 3);
        buf.fill(fill_area, Cell::new("#").fg(Color::Blue));

        assert_eq!(buf.get(0, 0).unwrap().symbol, " "); // Outside fill area
        assert_eq!(buf.get(1, 1).unwrap().symbol, "#"); // Inside fill area
        assert_eq!(buf.get(3, 3).unwrap().symbol, "#"); // Inside fill area
        assert_eq!(buf.get(4, 4).unwrap().symbol, " "); // Outside fill area
    }

    #[test]
    fn test_buffer_diff() {
        let area = Rect::new(0, 0, 5, 5);
        let mut buf1 = Buffer::new(area);
        let mut buf2 = Buffer::new(area);

        buf1.set(1, 1, Cell::new("A"));
        buf2.set(1, 1, Cell::new("B"));
        buf2.set(2, 2, Cell::new("C"));

        let diffs: Vec<_> = buf2.diff(&buf1).collect();
        assert_eq!(diffs.len(), 2);
    }

    #[test]
    fn test_buffer_clear() {
        let area = Rect::new(0, 0, 5, 5);
        let mut buf = Buffer::new(area);

        buf.set(2, 2, Cell::new("X"));
        buf.clear();

        assert_eq!(buf.get(2, 2).unwrap().symbol, " ");
    }
}
