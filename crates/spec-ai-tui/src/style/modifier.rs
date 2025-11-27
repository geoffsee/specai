//! Text style modifiers (bold, italic, underline, etc.)

use crossterm::style::Attribute;
use std::ops::{BitOr, BitOrAssign};

/// Style modifiers as a bitfield
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Modifier(u16);

impl Modifier {
    /// No modifiers
    pub const NONE: Self = Self(0);
    /// Bold text
    pub const BOLD: Self = Self(1 << 0);
    /// Dim/faint text
    pub const DIM: Self = Self(1 << 1);
    /// Italic text
    pub const ITALIC: Self = Self(1 << 2);
    /// Underlined text
    pub const UNDERLINED: Self = Self(1 << 3);
    /// Slow blink
    pub const SLOW_BLINK: Self = Self(1 << 4);
    /// Rapid blink
    pub const RAPID_BLINK: Self = Self(1 << 5);
    /// Reversed (swap fg/bg)
    pub const REVERSED: Self = Self(1 << 6);
    /// Hidden text
    pub const HIDDEN: Self = Self(1 << 7);
    /// Strikethrough
    pub const CROSSED_OUT: Self = Self(1 << 8);

    /// Create an empty modifier set
    pub const fn empty() -> Self {
        Self::NONE
    }

    /// Check if no modifiers are set
    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// Check if a modifier is set
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Create union of modifiers
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Create intersection of modifiers
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Remove modifiers
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// Insert a modifier
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Remove a modifier
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Get crossterm attributes for this modifier
    pub fn attributes(&self) -> Vec<Attribute> {
        let mut attrs = Vec::new();
        if self.contains(Self::BOLD) {
            attrs.push(Attribute::Bold);
        }
        if self.contains(Self::DIM) {
            attrs.push(Attribute::Dim);
        }
        if self.contains(Self::ITALIC) {
            attrs.push(Attribute::Italic);
        }
        if self.contains(Self::UNDERLINED) {
            attrs.push(Attribute::Underlined);
        }
        if self.contains(Self::SLOW_BLINK) {
            attrs.push(Attribute::SlowBlink);
        }
        if self.contains(Self::RAPID_BLINK) {
            attrs.push(Attribute::RapidBlink);
        }
        if self.contains(Self::REVERSED) {
            attrs.push(Attribute::Reverse);
        }
        if self.contains(Self::HIDDEN) {
            attrs.push(Attribute::Hidden);
        }
        if self.contains(Self::CROSSED_OUT) {
            attrs.push(Attribute::CrossedOut);
        }
        attrs
    }
}

impl BitOr for Modifier {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl BitOrAssign for Modifier {
    fn bitor_assign(&mut self, rhs: Self) {
        self.insert(rhs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modifier_empty() {
        let m = Modifier::empty();
        assert!(m.is_empty());
        assert!(!m.contains(Modifier::BOLD));
    }

    #[test]
    fn test_modifier_union() {
        let m = Modifier::BOLD | Modifier::ITALIC;
        assert!(m.contains(Modifier::BOLD));
        assert!(m.contains(Modifier::ITALIC));
        assert!(!m.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn test_modifier_insert_remove() {
        let mut m = Modifier::BOLD;
        m.insert(Modifier::ITALIC);
        assert!(m.contains(Modifier::BOLD));
        assert!(m.contains(Modifier::ITALIC));

        m.remove(Modifier::BOLD);
        assert!(!m.contains(Modifier::BOLD));
        assert!(m.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_modifier_attributes() {
        let m = Modifier::BOLD | Modifier::UNDERLINED;
        let attrs = m.attributes();
        assert_eq!(attrs.len(), 2);
        assert!(attrs.contains(&Attribute::Bold));
        assert!(attrs.contains(&Attribute::Underlined));
    }
}
