//! 2D coordinate point

/// A 2D coordinate in the terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

impl Point {
    /// Create a new point
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    /// Origin point (0, 0)
    pub const fn origin() -> Self {
        Self { x: 0, y: 0 }
    }

    /// Add an offset to this point
    pub fn offset(self, dx: i16, dy: i16) -> Self {
        Self {
            x: (self.x as i32 + dx as i32).max(0) as u16,
            y: (self.y as i32 + dy as i32).max(0) as u16,
        }
    }
}

impl From<(u16, u16)> for Point {
    fn from((x, y): (u16, u16)) -> Self {
        Self { x, y }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_creation() {
        let p = Point::new(10, 20);
        assert_eq!(p.x, 10);
        assert_eq!(p.y, 20);
    }

    #[test]
    fn test_point_offset() {
        let p = Point::new(10, 10);
        assert_eq!(p.offset(5, -3), Point::new(15, 7));
        assert_eq!(p.offset(-20, 0), Point::new(0, 10)); // Clamped to 0
    }
}
