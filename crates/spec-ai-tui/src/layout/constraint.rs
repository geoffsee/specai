//! Size constraints for layout

/// Size constraint for layout calculations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Constraint {
    /// Fixed size in cells
    Fixed(u16),
    /// Percentage of available space (0-100)
    Percentage(u16),
    /// Minimum size (at least this many cells)
    Min(u16),
    /// Maximum size (at most this many cells)
    Max(u16),
    /// Fill remaining space with the given weight
    /// Multiple Fill constraints share space proportionally by weight
    Fill(u16),
    /// Ratio of available space (numerator/denominator)
    Ratio(u16, u16),
}

impl Constraint {
    /// Create a fixed constraint
    pub const fn fixed(n: u16) -> Self {
        Self::Fixed(n)
    }

    /// Create a percentage constraint
    pub const fn percentage(p: u16) -> Self {
        Self::Percentage(p)
    }

    /// Create a minimum constraint
    pub const fn min(n: u16) -> Self {
        Self::Min(n)
    }

    /// Create a maximum constraint
    pub const fn max(n: u16) -> Self {
        Self::Max(n)
    }

    /// Create a fill constraint with weight
    pub const fn fill(weight: u16) -> Self {
        Self::Fill(weight)
    }

    /// Create a ratio constraint
    pub const fn ratio(numerator: u16, denominator: u16) -> Self {
        Self::Ratio(numerator, denominator)
    }

    /// Check if this is a flexible constraint (Fill)
    pub fn is_flexible(&self) -> bool {
        matches!(self, Self::Fill(_))
    }

    /// Get the weight if this is a Fill constraint, otherwise 0
    pub fn fill_weight(&self) -> u16 {
        match self {
            Self::Fill(w) => *w,
            _ => 0,
        }
    }

    /// Calculate the size for a fixed constraint given available space
    /// Returns (size, remaining) where remaining is for Fill constraints
    pub fn resolve(&self, available: u16) -> (u16, bool) {
        match self {
            Self::Fixed(n) => ((*n).min(available), false),
            Self::Percentage(p) => {
                let size = (available as u32 * (*p).min(100) as u32 / 100) as u16;
                (size, false)
            }
            Self::Min(n) => ((*n).min(available), false),
            Self::Max(n) => ((*n).min(available), false),
            Self::Fill(_) => (0, true), // Handled specially
            Self::Ratio(num, denom) => {
                if *denom == 0 {
                    (0, false)
                } else {
                    let size = (available as u32 * *num as u32 / *denom as u32) as u16;
                    (size.min(available), false)
                }
            }
        }
    }
}

impl Default for Constraint {
    fn default() -> Self {
        Self::Fill(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_constraint() {
        let c = Constraint::Fixed(10);
        assert_eq!(c.resolve(100), (10, false));
        assert_eq!(c.resolve(5), (5, false)); // Clamped to available
    }

    #[test]
    fn test_percentage_constraint() {
        let c = Constraint::Percentage(50);
        assert_eq!(c.resolve(100), (50, false));
        assert_eq!(c.resolve(80), (40, false));
    }

    #[test]
    fn test_ratio_constraint() {
        let c = Constraint::Ratio(1, 3);
        assert_eq!(c.resolve(90), (30, false));
        assert_eq!(c.resolve(100), (33, false)); // Integer division
    }

    #[test]
    fn test_fill_constraint() {
        let c = Constraint::Fill(1);
        assert!(c.is_flexible());
        assert_eq!(c.fill_weight(), 1);
        assert_eq!(c.resolve(100), (0, true)); // Returns 0, marked as flexible
    }

    #[test]
    fn test_ratio_zero_denominator() {
        let c = Constraint::Ratio(1, 0);
        assert_eq!(c.resolve(100), (0, false));
    }
}
