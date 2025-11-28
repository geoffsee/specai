use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    versions: HashMap<String, i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockOrder {
    Before,
    After,
    Concurrent,
    Equal,
}

impl VectorClock {
    pub fn new() -> Self {
        Self {
            versions: HashMap::new(),
        }
    }

    pub fn from_json(json: &str) -> Result<Self> {
        if json.is_empty() || json == "{}" {
            return Ok(Self::new());
        }
        serde_json::from_str(json).context("parsing vector clock JSON")
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(&self).context("serializing vector clock")
    }

    pub fn get(&self, instance_id: &str) -> i64 {
        self.versions.get(instance_id).copied().unwrap_or(0)
    }

    pub fn set(&mut self, instance_id: String, version: i64) {
        self.versions.insert(instance_id, version);
    }

    pub fn increment(&mut self, instance_id: &str) -> i64 {
        let version = self.get(instance_id) + 1;
        self.versions.insert(instance_id.to_string(), version);
        version
    }

    pub fn merge(&mut self, other: &VectorClock) {
        for (instance_id, &other_version) in &other.versions {
            let current_version = self.get(instance_id);
            if other_version > current_version {
                self.versions.insert(instance_id.clone(), other_version);
            }
        }
    }

    pub fn compare(&self, other: &VectorClock) -> ClockOrder {
        let mut self_less_or_equal = true;
        let mut other_less_or_equal = true;

        let all_instances: HashSet<_> = self.versions.keys().chain(other.versions.keys()).collect();

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

    pub fn happens_before(&self, other: &VectorClock) -> bool {
        matches!(self.compare(other), ClockOrder::Before)
    }

    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        matches!(self.compare(other), ClockOrder::Concurrent)
    }

    pub fn is_equal(&self, other: &VectorClock) -> bool {
        matches!(self.compare(other), ClockOrder::Equal)
    }

    pub fn instances(&self) -> Vec<String> {
        self.versions.keys().cloned().collect()
    }

    pub fn instance_count(&self) -> usize {
        self.versions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

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
        assert_eq!(clock1.get("b"), 3);
        assert_eq!(clock1.get("c"), 1);
    }
}
