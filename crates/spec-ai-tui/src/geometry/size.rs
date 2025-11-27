//! Size with width and height

/// Size representing width and height in terminal cells
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Size {
    pub width: u16,
    pub height: u16,
}

impl Size {
    /// Create a new size
    pub const fn new(width: u16, height: u16) -> Self {
        Self { width, height }
    }

    /// Zero size
    pub const fn zero() -> Self {
        Self { width: 0, height: 0 }
    }

    /// Check if the size is empty (zero area)
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Calculate the area (number of cells)
    pub const fn area(&self) -> u32 {
        self.width as u32 * self.height as u32
    }
}

impl From<(u16, u16)> for Size {
    fn from((width, height): (u16, u16)) -> Self {
        Self { width, height }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_creation() {
        let s = Size::new(80, 24);
        assert_eq!(s.width, 80);
        assert_eq!(s.height, 24);
    }

    #[test]
    fn test_size_area() {
        let s = Size::new(80, 24);
        assert_eq!(s.area(), 1920);
    }

    #[test]
    fn test_size_empty() {
        assert!(Size::new(0, 10).is_empty());
        assert!(Size::new(10, 0).is_empty());
        assert!(Size::zero().is_empty());
        assert!(!Size::new(1, 1).is_empty());
    }
}
