use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Vector clock for tracking causality in distributed systems
/// Each instance maintains its own logical clock version
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    /// Map of instance_id to version number
    versions: HashMap<String, i64>,
}

/// Result of comparing two vector clocks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockOrder {
    /// First clock happens-before second (causally ordered)
    Before,
    /// Second clock happens-before first (causally ordered)
    After,
    /// Clocks are concurrent (need conflict resolution)
    Concurrent,
    /// Clocks are identical
    Equal,
}

impl VectorClock {
    /// Create a new empty vector clock
    pub fn new() -> Self {
        Self {
            versions: HashMap::new(),
        }
    }

    /// Create a vector clock from a JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        if json.is_empty() || json == "{}" {
            return Ok(Self::new());
        }
        serde_json::from_str(json).context("parsing vector clock JSON")
    }

    /// Serialize to JSON string for storage
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(&self).context("serializing vector clock")
    }

    /// Get the version for a specific instance (0 if not present)
    pub fn get(&self, instance_id: &str) -> i64 {
        self.versions.get(instance_id).copied().unwrap_or(0)
    }

    /// Set the version for a specific instance
    pub fn set(&mut self, instance_id: String, version: i64) {
        self.versions.insert(instance_id, version);
    }

    /// Increment the version for the given instance
    pub fn increment(&mut self, instance_id: &str) -> i64 {
        let version = self.get(instance_id) + 1;
        self.versions.insert(instance_id.to_string(), version);
        version
    }

    /// Merge another vector clock, taking the maximum version for each instance
    /// This is used when receiving updates from other instances
    pub fn merge(&mut self, other: &VectorClock) {
        for (instance_id, &other_version) in &other.versions {
            let current_version = self.get(instance_id);
            if other_version > current_version {
                self.versions.insert(instance_id.clone(), other_version);
            }
        }
    }

    /// Compare this clock with another to determine causality relationship
    /// Returns:
    /// - Before: self happened-before other (self is older)
    /// - After: other happened-before self (self is newer)
    /// - Concurrent: neither happened-before the other (conflict)
    /// - Equal: clocks are identical
    pub fn compare(&self, other: &VectorClock) -> ClockOrder {
        let mut self_less_or_equal = true;
        let mut other_less_or_equal = true;

        // Get all instance IDs from both clocks
        let all_instances: std::collections::HashSet<_> = self
            .versions
            .keys()
            .chain(other.versions.keys())
            .collect();

        for instance_id in all_instances {
            let self_version = self.get(instance_id);
            let other_version = other.get(instance_id);

            if self_version > other_version {
                self_less_or_equal = false;
            }
            if other_version > self_version {
                other_less_or_equal = false;
            }
        }

        match (self_less_or_equal, other_less_or_equal) {
            (true, true) => ClockOrder::Equal,
            (true, false) => ClockOrder::Before,
            (false, true) => ClockOrder::After,
            (false, false) => ClockOrder::Concurrent,
        }
    }

    /// Check if this clock happened-before another (causally precedes)
    pub fn happens_before(&self, other: &VectorClock) -> bool {
        matches!(self.compare(other), ClockOrder::Before)
    }

    /// Check if this clock is concurrent with another (conflict)
    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        matches!(self.compare(other), ClockOrder::Concurrent)
    }

    /// Check if this clock is equal to another
    pub fn is_equal(&self, other: &VectorClock) -> bool {
        matches!(self.compare(other), ClockOrder::Equal)
    }

    /// Get all instance IDs tracked by this clock
    pub fn instances(&self) -> Vec<String> {
        self.versions.keys().cloned().collect()
    }

    /// Get the number of instances tracked
    pub fn instance_count(&self) -> usize {
        self.versions.len()
    }

    /// Check if this clock is empty (no versions tracked)
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    /// Clear all versions
    pub fn clear(&mut self) {
        self.versions.clear();
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for VectorClock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{")?;
        let mut first = true;
        for (instance_id, version) in &self.versions {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{}: {}", instance_id, version)?;
            first = false;
        }
        write!(f, "}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_clock_is_empty() {
        let clock = VectorClock::new();
        assert!(clock.is_empty());
        assert_eq!(clock.get("instance1"), 0);
    }

    #[test]
    fn test_increment() {
        let mut clock = VectorClock::new();
        assert_eq!(clock.increment("instance1"), 1);
        assert_eq!(clock.increment("instance1"), 2);
        assert_eq!(clock.get("instance1"), 2);
    }

    #[test]
    fn test_merge() {
        let mut clock1 = VectorClock::new();
        clock1.set("a".to_string(), 1);
        clock1.set("b".to_string(), 2);

        let mut clock2 = VectorClock::new();
        clock2.set("b".to_string(), 3);
        clock2.set("c".to_string(), 1);

        clock1.merge(&clock2);

        assert_eq!(clock1.get("a"), 1);
        assert_eq!(clock1.get("b"), 3); // max(2, 3)
        assert_eq!(clock1.get("c"), 1);
    }

    #[test]
    fn test_compare_equal() {
        let mut clock1 = VectorClock::new();
        clock1.set("a".to_string(), 1);

        let mut clock2 = VectorClock::new();
        clock2.set("a".to_string(), 1);

        assert_eq!(clock1.compare(&clock2), ClockOrder::Equal);
        assert!(clock1.is_equal(&clock2));
    }

    #[test]
    fn test_compare_before() {
        let mut clock1 = VectorClock::new();
        clock1.set("a".to_string(), 1);

        let mut clock2 = VectorClock::new();
        clock2.set("a".to_string(), 2);

        assert_eq!(clock1.compare(&clock2), ClockOrder::Before);
        assert!(clock1.happens_before(&clock2));
    }

    #[test]
    fn test_compare_after() {
        let mut clock1 = VectorClock::new();
        clock1.set("a".to_string(), 2);

        let mut clock2 = VectorClock::new();
        clock2.set("a".to_string(), 1);

        assert_eq!(clock1.compare(&clock2), ClockOrder::After);
        assert!(!clock1.happens_before(&clock2));
    }

    #[test]
    fn test_compare_concurrent() {
        let mut clock1 = VectorClock::new();
        clock1.set("a".to_string(), 2);
        clock1.set("b".to_string(), 1);

        let mut clock2 = VectorClock::new();
        clock2.set("a".to_string(), 1);
        clock2.set("b".to_string(), 2);

        assert_eq!(clock1.compare(&clock2), ClockOrder::Concurrent);
        assert!(clock1.is_concurrent(&clock2));
    }

    #[test]
    fn test_json_serialization() {
        let mut clock = VectorClock::new();
        clock.set("instance1".to_string(), 5);
        clock.set("instance2".to_string(), 3);

        let json = clock.to_json().unwrap();
        let parsed = VectorClock::from_json(&json).unwrap();

        assert_eq!(clock, parsed);
        assert_eq!(parsed.get("instance1"), 5);
        assert_eq!(parsed.get("instance2"), 3);
    }

    #[test]
    fn test_empty_json() {
        let clock = VectorClock::from_json("{}").unwrap();
        assert!(clock.is_empty());

        let clock2 = VectorClock::from_json("").unwrap();
        assert!(clock2.is_empty());
    }

    #[test]
    fn test_display() {
        let mut clock = VectorClock::new();
        clock.set("a".to_string(), 1);
        clock.set("b".to_string(), 2);

        let display = format!("{}", clock);
        assert!(display.contains("a: 1"));
        assert!(display.contains("b: 2"));
    }
}
