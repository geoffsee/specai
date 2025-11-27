//! Focus management for interactive widgets

use crate::geometry::Rect;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// Unique identifier for a focusable widget
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FocusId(u32);

impl FocusId {
    /// Generate a new unique focus ID
    pub fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    /// Get the numeric value
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl Default for FocusId {
    fn default() -> Self {
        Self::new()
    }
}

/// Focus navigation direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusDirection {
    /// Move to next widget in tab order
    Next,
    /// Move to previous widget in tab order
    Previous,
    /// Move up spatially
    Up,
    /// Move down spatially
    Down,
    /// Move left spatially
    Left,
    /// Move right spatially
    Right,
}

/// Manages focus state across widgets
#[derive(Debug, Default)]
pub struct FocusManager {
    /// Currently focused widget
    current: Option<FocusId>,
    /// Ordered list of focusable widgets (tab order)
    focus_order: Vec<FocusId>,
    /// Spatial positions for directional navigation
    positions: HashMap<FocusId, Rect>,
}

impl FocusManager {
    /// Create a new focus manager
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a focusable widget
    pub fn register(&mut self, id: FocusId, position: Rect) {
        if !self.focus_order.contains(&id) {
            self.focus_order.push(id);
        }
        self.positions.insert(id, position);

        // Auto-focus first widget
        if self.current.is_none() {
            self.current = Some(id);
        }
    }

    /// Unregister a widget
    pub fn unregister(&mut self, id: FocusId) {
        self.focus_order.retain(|&i| i != id);
        self.positions.remove(&id);

        if self.current == Some(id) {
            self.current = self.focus_order.first().copied();
        }
    }

    /// Clear all registered widgets
    pub fn clear(&mut self) {
        self.focus_order.clear();
        self.positions.clear();
        self.current = None;
    }

    /// Get the currently focused widget
    pub fn current(&self) -> Option<FocusId> {
        self.current
    }

    /// Check if a widget is focused
    pub fn is_focused(&self, id: FocusId) -> bool {
        self.current == Some(id)
    }

    /// Set focus to a specific widget
    pub fn focus(&mut self, id: FocusId) {
        if self.focus_order.contains(&id) {
            self.current = Some(id);
        }
    }

    /// Clear focus (no widget focused)
    pub fn blur(&mut self) {
        self.current = None;
    }

    /// Navigate focus in the given direction
    pub fn navigate(&mut self, direction: FocusDirection) -> Option<FocusId> {
        match direction {
            FocusDirection::Next => self.focus_next(),
            FocusDirection::Previous => self.focus_previous(),
            FocusDirection::Up
            | FocusDirection::Down
            | FocusDirection::Left
            | FocusDirection::Right => self.focus_spatial(direction),
        }
    }

    /// Move focus to the next widget in tab order
    pub fn focus_next(&mut self) -> Option<FocusId> {
        if self.focus_order.is_empty() {
            return None;
        }

        let next = match self.current {
            Some(id) => {
                let idx = self.focus_order.iter().position(|&i| i == id).unwrap_or(0);
                self.focus_order[(idx + 1) % self.focus_order.len()]
            }
            None => self.focus_order[0],
        };

        self.current = Some(next);
        self.current
    }

    /// Move focus to the previous widget in tab order
    pub fn focus_previous(&mut self) -> Option<FocusId> {
        if self.focus_order.is_empty() {
            return None;
        }

        let prev = match self.current {
            Some(id) => {
                let idx = self.focus_order.iter().position(|&i| i == id).unwrap_or(0);
                let prev_idx = if idx == 0 {
                    self.focus_order.len() - 1
                } else {
                    idx - 1
                };
                self.focus_order[prev_idx]
            }
            None => *self.focus_order.last().unwrap(),
        };

        self.current = Some(prev);
        self.current
    }

    /// Move focus spatially in a direction
    fn focus_spatial(&mut self, direction: FocusDirection) -> Option<FocusId> {
        let current_pos = self.current.and_then(|id| self.positions.get(&id))?;

        // Find candidates in the given direction
        let candidates: Vec<_> = self
            .positions
            .iter()
            .filter(|(&id, _)| Some(id) != self.current)
            .filter(|(_, rect)| match direction {
                FocusDirection::Up => rect.bottom() <= current_pos.top(),
                FocusDirection::Down => rect.top() >= current_pos.bottom(),
                FocusDirection::Left => rect.right() <= current_pos.left(),
                FocusDirection::Right => rect.left() >= current_pos.right(),
                _ => false,
            })
            .collect();

        // Find the nearest candidate
        let nearest = candidates
            .iter()
            .min_by_key(|(_, rect)| {
                let dx = (rect.x as i32 - current_pos.x as i32).abs();
                let dy = (rect.y as i32 - current_pos.y as i32).abs();
                dx + dy // Manhattan distance
            })
            .map(|(&id, _)| id);

        if let Some(id) = nearest {
            self.current = Some(id);
        }

        self.current
    }

    /// Get the number of focusable widgets
    pub fn count(&self) -> usize {
        self.focus_order.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_id_unique() {
        let id1 = FocusId::new();
        let id2 = FocusId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_focus_manager_register() {
        let mut fm = FocusManager::new();
        let id = FocusId::new();

        fm.register(id, Rect::new(0, 0, 10, 10));

        assert_eq!(fm.current(), Some(id)); // Auto-focused
        assert_eq!(fm.count(), 1);
    }

    #[test]
    fn test_focus_next() {
        let mut fm = FocusManager::new();
        let id1 = FocusId::new();
        let id2 = FocusId::new();
        let id3 = FocusId::new();

        fm.register(id1, Rect::new(0, 0, 10, 10));
        fm.register(id2, Rect::new(0, 10, 10, 10));
        fm.register(id3, Rect::new(0, 20, 10, 10));

        assert_eq!(fm.current(), Some(id1));
        fm.focus_next();
        assert_eq!(fm.current(), Some(id2));
        fm.focus_next();
        assert_eq!(fm.current(), Some(id3));
        fm.focus_next();
        assert_eq!(fm.current(), Some(id1)); // Wraps around
    }

    #[test]
    fn test_focus_previous() {
        let mut fm = FocusManager::new();
        let id1 = FocusId::new();
        let id2 = FocusId::new();

        fm.register(id1, Rect::new(0, 0, 10, 10));
        fm.register(id2, Rect::new(0, 10, 10, 10));

        assert_eq!(fm.current(), Some(id1));
        fm.focus_previous();
        assert_eq!(fm.current(), Some(id2)); // Wraps around
    }

    #[test]
    fn test_focus_unregister() {
        let mut fm = FocusManager::new();
        let id1 = FocusId::new();
        let id2 = FocusId::new();

        fm.register(id1, Rect::new(0, 0, 10, 10));
        fm.register(id2, Rect::new(0, 10, 10, 10));

        fm.focus(id2);
        fm.unregister(id2);

        assert_eq!(fm.current(), Some(id1)); // Falls back to first
        assert_eq!(fm.count(), 1);
    }
}
