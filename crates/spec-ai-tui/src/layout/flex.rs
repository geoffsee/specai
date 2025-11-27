//! Flexbox-like layout engine

use super::{Constraint, Direction};
use crate::geometry::Rect;

/// Layout builder for arranging widgets
#[derive(Debug, Clone)]
pub struct Layout {
    direction: Direction,
    constraints: Vec<Constraint>,
    margin: u16,
    spacing: u16,
}

impl Layout {
    /// Create a new horizontal layout
    pub fn horizontal() -> Self {
        Self {
            direction: Direction::Horizontal,
            constraints: Vec::new(),
            margin: 0,
            spacing: 0,
        }
    }

    /// Create a new vertical layout
    pub fn vertical() -> Self {
        Self {
            direction: Direction::Vertical,
            constraints: Vec::new(),
            margin: 0,
            spacing: 0,
        }
    }

    /// Create a new layout with the given direction
    pub fn new(direction: Direction) -> Self {
        Self {
            direction,
            constraints: Vec::new(),
            margin: 0,
            spacing: 0,
        }
    }

    /// Set the direction
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    /// Set the constraints
    pub fn constraints<I: IntoIterator<Item = Constraint>>(mut self, constraints: I) -> Self {
        self.constraints = constraints.into_iter().collect();
        self
    }

    /// Set the margin (applied to all sides)
    pub fn margin(mut self, margin: u16) -> Self {
        self.margin = margin;
        self
    }

    /// Set the spacing between elements
    pub fn spacing(mut self, spacing: u16) -> Self {
        self.spacing = spacing;
        self
    }

    /// Split an area according to constraints
    pub fn split(&self, area: Rect) -> Vec<Rect> {
        // Apply margin
        let inner = area.inner(self.margin);
        if inner.is_empty() || self.constraints.is_empty() {
            return vec![];
        }

        // Determine the dimension we're splitting
        let (total_space, cross_size) = match self.direction {
            Direction::Horizontal => (inner.width, inner.height),
            Direction::Vertical => (inner.height, inner.width),
        };

        // Account for spacing between elements
        let num_gaps = self.constraints.len().saturating_sub(1) as u16;
        let spacing_total = self.spacing * num_gaps;
        let available = total_space.saturating_sub(spacing_total);

        // First pass: resolve fixed constraints and count fill weights
        let mut sizes: Vec<u16> = vec![0; self.constraints.len()];
        let mut remaining = available;
        let mut total_fill_weight = 0u32;

        for (i, constraint) in self.constraints.iter().enumerate() {
            // Percentages and ratios should be calculated from total available, not remaining
            let resolve_base = match constraint {
                Constraint::Percentage(_) | Constraint::Ratio(_, _) => available,
                _ => remaining,
            };
            let (size, is_fill) = constraint.resolve(resolve_base);
            if is_fill {
                total_fill_weight += constraint.fill_weight() as u32;
            } else {
                sizes[i] = size;
                remaining = remaining.saturating_sub(size);
            }
        }

        // Second pass: distribute remaining space to Fill constraints
        if total_fill_weight > 0 && remaining > 0 {
            let fill_space = remaining;
            let mut distributed = 0u16;

            for (i, constraint) in self.constraints.iter().enumerate() {
                if let Constraint::Fill(weight) = constraint {
                    // Calculate this fill's share
                    let share = (fill_space as u32 * *weight as u32 / total_fill_weight) as u16;
                    sizes[i] = share;
                    distributed += share;
                }
            }

            // Distribute any rounding remainder to the last Fill
            let leftover = fill_space.saturating_sub(distributed);
            if leftover > 0 {
                for (i, constraint) in self.constraints.iter().enumerate().rev() {
                    if matches!(constraint, Constraint::Fill(_)) {
                        sizes[i] = sizes[i].saturating_add(leftover);
                        break;
                    }
                }
            }
        }

        // Build result rectangles
        let mut result = Vec::with_capacity(self.constraints.len());
        let mut offset = match self.direction {
            Direction::Horizontal => inner.x,
            Direction::Vertical => inner.y,
        };

        for (i, size) in sizes.into_iter().enumerate() {
            let rect = match self.direction {
                Direction::Horizontal => Rect::new(offset, inner.y, size, cross_size),
                Direction::Vertical => Rect::new(inner.x, offset, cross_size, size),
            };
            result.push(rect);

            offset = offset.saturating_add(size);
            if i < self.constraints.len() - 1 {
                offset = offset.saturating_add(self.spacing);
            }
        }

        result
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::vertical()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertical_split_fixed() {
        let area = Rect::new(0, 0, 100, 50);
        let chunks = Layout::vertical()
            .constraints([
                Constraint::Fixed(10),
                Constraint::Fixed(20),
                Constraint::Fixed(10),
            ])
            .split(area);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], Rect::new(0, 0, 100, 10));
        assert_eq!(chunks[1], Rect::new(0, 10, 100, 20));
        assert_eq!(chunks[2], Rect::new(0, 30, 100, 10));
    }

    #[test]
    fn test_vertical_split_with_fill() {
        let area = Rect::new(0, 0, 100, 50);
        let chunks = Layout::vertical()
            .constraints([
                Constraint::Fixed(10),
                Constraint::Fill(1),
                Constraint::Fixed(5),
            ])
            .split(area);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].height, 10);
        assert_eq!(chunks[1].height, 35); // 50 - 10 - 5
        assert_eq!(chunks[2].height, 5);
    }

    #[test]
    fn test_horizontal_split() {
        let area = Rect::new(0, 0, 100, 50);
        let chunks = Layout::horizontal()
            .constraints([Constraint::Percentage(30), Constraint::Fill(1)])
            .split(area);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].width, 30);
        assert_eq!(chunks[1].width, 70);
        assert_eq!(chunks[0].height, 50);
        assert_eq!(chunks[1].height, 50);
    }

    #[test]
    fn test_multiple_fills() {
        let area = Rect::new(0, 0, 100, 100);
        let chunks = Layout::vertical()
            .constraints([
                Constraint::Fill(1),
                Constraint::Fill(2),
                Constraint::Fill(1),
            ])
            .split(area);

        assert_eq!(chunks.len(), 3);
        // Total weight = 4, so 1/4, 2/4, 1/4
        assert_eq!(chunks[0].height, 25);
        assert_eq!(chunks[1].height, 50);
        assert_eq!(chunks[2].height, 25);
    }

    #[test]
    fn test_with_margin() {
        let area = Rect::new(0, 0, 100, 50);
        let chunks = Layout::vertical()
            .margin(5)
            .constraints([Constraint::Fill(1)])
            .split(area);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], Rect::new(5, 5, 90, 40)); // Margin applied
    }

    #[test]
    fn test_with_spacing() {
        let area = Rect::new(0, 0, 100, 50);
        let chunks = Layout::vertical()
            .spacing(2)
            .constraints([
                Constraint::Fixed(10),
                Constraint::Fixed(10),
                Constraint::Fixed(10),
            ])
            .split(area);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].y, 0);
        assert_eq!(chunks[1].y, 12); // 10 + 2 spacing
        assert_eq!(chunks[2].y, 24); // 12 + 10 + 2 spacing
    }

    #[test]
    fn test_percentage() {
        let area = Rect::new(0, 0, 100, 100);
        let chunks = Layout::vertical()
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(50),
                Constraint::Percentage(25),
            ])
            .split(area);

        assert_eq!(chunks[0].height, 25);
        assert_eq!(chunks[1].height, 50);
        assert_eq!(chunks[2].height, 25);
    }

    #[test]
    fn test_empty_constraints() {
        let area = Rect::new(0, 0, 100, 50);
        let chunks = Layout::vertical().constraints([]).split(area);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_empty_area() {
        let area = Rect::new(0, 0, 0, 0);
        let chunks = Layout::vertical()
            .constraints([Constraint::Fill(1)])
            .split(area);
        assert!(chunks.is_empty());
    }
}
