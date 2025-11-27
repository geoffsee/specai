//! Rectangular region in the terminal

use super::{Point, Size};

/// A rectangular region in the terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    /// Create a new rectangle
    pub const fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self { x, y, width, height }
    }

    /// Create a rectangle at origin with given size
    pub const fn sized(width: u16, height: u16) -> Self {
        Self::new(0, 0, width, height)
    }

    /// Create from position and size
    pub const fn from_pos_size(pos: Point, size: Size) -> Self {
        Self::new(pos.x, pos.y, size.width, size.height)
    }

    /// Empty rectangle
    pub const fn empty() -> Self {
        Self::new(0, 0, 0, 0)
    }

    /// Get the area (number of cells)
    pub const fn area(&self) -> u32 {
        self.width as u32 * self.height as u32
    }

    /// Check if the rectangle is empty
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Get the top-left corner position
    pub const fn position(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Get the size
    pub const fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Get the left edge x coordinate
    pub const fn left(&self) -> u16 {
        self.x
    }

    /// Get the right edge x coordinate (exclusive)
    pub const fn right(&self) -> u16 {
        self.x.saturating_add(self.width)
    }

    /// Get the top edge y coordinate
    pub const fn top(&self) -> u16 {
        self.y
    }

    /// Get the bottom edge y coordinate (exclusive)
    pub const fn bottom(&self) -> u16 {
        self.y.saturating_add(self.height)
    }

    /// Check if a point is inside this rectangle
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// Check if a point is inside this rectangle
    pub fn contains_point(&self, point: Point) -> bool {
        self.contains(point.x, point.y)
    }

    /// Get the inner rectangle with the given margin on all sides
    pub fn inner(&self, margin: u16) -> Self {
        Self {
            x: self.x.saturating_add(margin),
            y: self.y.saturating_add(margin),
            width: self.width.saturating_sub(margin * 2),
            height: self.height.saturating_sub(margin * 2),
        }
    }

    /// Get the inner rectangle with asymmetric margins
    pub fn inner_margin(&self, top: u16, right: u16, bottom: u16, left: u16) -> Self {
        Self {
            x: self.x.saturating_add(left),
            y: self.y.saturating_add(top),
            width: self.width.saturating_sub(left + right),
            height: self.height.saturating_sub(top + bottom),
        }
    }

    /// Split horizontally at a position (returns left and right)
    pub fn split_horizontal(&self, at: u16) -> (Self, Self) {
        let at = at.min(self.width);
        (
            Self::new(self.x, self.y, at, self.height),
            Self::new(self.x.saturating_add(at), self.y, self.width.saturating_sub(at), self.height),
        )
    }

    /// Split vertically at a position (returns top and bottom)
    pub fn split_vertical(&self, at: u16) -> (Self, Self) {
        let at = at.min(self.height);
        (
            Self::new(self.x, self.y, self.width, at),
            Self::new(self.x, self.y.saturating_add(at), self.width, self.height.saturating_sub(at)),
        )
    }

    /// Get the intersection of two rectangles
    pub fn intersect(&self, other: &Rect) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        if x >= right || y >= bottom {
            Self::empty()
        } else {
            Self::new(x, y, right - x, bottom - y)
        }
    }

    /// Get the bounding rectangle containing both rectangles
    pub fn union(&self, other: &Rect) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }

        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());

        Self::new(x, y, right - x, bottom - y)
    }

    /// Iterate over all positions in this rectangle
    pub fn positions(&self) -> impl Iterator<Item = (u16, u16)> {
        let x_start = self.x;
        let x_end = self.right();
        let y_start = self.y;
        let y_end = self.bottom();

        (y_start..y_end).flat_map(move |y| (x_start..x_end).map(move |x| (x, y)))
    }
}

impl From<(u16, u16, u16, u16)> for Rect {
    fn from((x, y, width, height): (u16, u16, u16, u16)) -> Self {
        Self::new(x, y, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_creation() {
        let r = Rect::new(5, 10, 80, 24);
        assert_eq!(r.x, 5);
        assert_eq!(r.y, 10);
        assert_eq!(r.width, 80);
        assert_eq!(r.height, 24);
    }

    #[test]
    fn test_rect_edges() {
        let r = Rect::new(5, 10, 80, 24);
        assert_eq!(r.left(), 5);
        assert_eq!(r.right(), 85);
        assert_eq!(r.top(), 10);
        assert_eq!(r.bottom(), 34);
    }

    #[test]
    fn test_rect_contains() {
        let r = Rect::new(5, 5, 10, 10);
        assert!(r.contains(5, 5));
        assert!(r.contains(14, 14));
        assert!(!r.contains(4, 5)); // Left of rect
        assert!(!r.contains(15, 5)); // Right of rect (exclusive)
        assert!(!r.contains(5, 4)); // Above rect
        assert!(!r.contains(5, 15)); // Below rect (exclusive)
    }

    #[test]
    fn test_rect_inner() {
        let r = Rect::new(0, 0, 20, 10);
        let inner = r.inner(2);
        assert_eq!(inner, Rect::new(2, 2, 16, 6));
    }

    #[test]
    fn test_rect_split_horizontal() {
        let r = Rect::new(0, 0, 100, 50);
        let (left, right) = r.split_horizontal(30);
        assert_eq!(left, Rect::new(0, 0, 30, 50));
        assert_eq!(right, Rect::new(30, 0, 70, 50));
    }

    #[test]
    fn test_rect_split_vertical() {
        let r = Rect::new(0, 0, 100, 50);
        let (top, bottom) = r.split_vertical(20);
        assert_eq!(top, Rect::new(0, 0, 100, 20));
        assert_eq!(bottom, Rect::new(0, 20, 100, 30));
    }

    #[test]
    fn test_rect_intersect() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(5, 5, 10, 10);
        let intersection = a.intersect(&b);
        assert_eq!(intersection, Rect::new(5, 5, 5, 5));

        // No intersection
        let c = Rect::new(20, 20, 10, 10);
        assert!(a.intersect(&c).is_empty());
    }

    #[test]
    fn test_rect_positions() {
        let r = Rect::new(0, 0, 3, 2);
        let positions: Vec<_> = r.positions().collect();
        assert_eq!(positions, vec![
            (0, 0), (1, 0), (2, 0),
            (0, 1), (1, 1), (2, 1),
        ]);
    }
}
