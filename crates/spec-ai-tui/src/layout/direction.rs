//! Layout direction

/// Direction for layout operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Direction {
    /// Horizontal layout (left to right)
    #[default]
    Horizontal,
    /// Vertical layout (top to bottom)
    Vertical,
}

impl Direction {
    /// Check if horizontal
    pub fn is_horizontal(&self) -> bool {
        matches!(self, Direction::Horizontal)
    }

    /// Check if vertical
    pub fn is_vertical(&self) -> bool {
        matches!(self, Direction::Vertical)
    }
}
