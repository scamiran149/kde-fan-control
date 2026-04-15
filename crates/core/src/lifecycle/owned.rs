//! Runtime tracking of which fans the daemon owns.
//!
//! `OwnedFanSet` records every fan that the daemon has switched
//! from BIOS control to manual PWM, along with its control mode
//! and sysfs path. This set drives the safety fallback: on
//! shutdown or crash, every owned fan must be driven to PWM 255.
//!
//! The set is persisted to disk so that a subsequent boot can
//! restore the owned-fan list before the daemon fully starts.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::inventory::ControlMode;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OwnedFanSet {
    owned: HashSet<String>,
    control_modes: HashMap<String, ControlMode>,
    fan_sysfs_paths: HashMap<String, String>,
}

impl OwnedFanSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn claim_fan(&mut self, fan_id: &str, control_mode: ControlMode, sysfs_path: &str) {
        self.owned.insert(fan_id.to_string());
        self.control_modes.insert(fan_id.to_string(), control_mode);
        self.fan_sysfs_paths
            .insert(fan_id.to_string(), sysfs_path.to_string());
    }

    pub fn release_fan(&mut self, fan_id: &str) {
        self.owned.remove(fan_id);
        self.control_modes.remove(fan_id);
        self.fan_sysfs_paths.remove(fan_id);
    }

    pub fn release_all(&mut self) {
        self.owned.clear();
        self.control_modes.clear();
        self.fan_sysfs_paths.clear();
    }

    pub fn owns(&self, fan_id: &str) -> bool {
        self.owned.contains(fan_id)
    }

    pub fn owned_fan_ids(&self) -> impl Iterator<Item = &str> {
        self.owned.iter().map(|s| s.as_str())
    }

    pub fn control_mode(&self, fan_id: &str) -> Option<ControlMode> {
        self.control_modes.get(fan_id).copied()
    }

    pub fn sysfs_path(&self, fan_id: &str) -> Option<&str> {
        self.fan_sysfs_paths.get(fan_id).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.owned.len()
    }

    pub fn is_empty(&self) -> bool {
        self.owned.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_fan_set_claim_and_release() {
        let mut owned = OwnedFanSet::new();

        owned.claim_fan("fan-1", ControlMode::Pwm, "/sys/class/hwmon/hwmon0/pwm1");
        assert!(owned.owns("fan-1"));
        assert_eq!(owned.control_mode("fan-1"), Some(ControlMode::Pwm));
        assert_eq!(
            owned.sysfs_path("fan-1"),
            Some("/sys/class/hwmon/hwmon0/pwm1")
        );
        assert_eq!(owned.len(), 1);

        owned.release_fan("fan-1");
        assert!(!owned.owns("fan-1"));
        assert!(owned.is_empty());
    }

    #[test]
    fn owned_fan_set_never_contains_unmanaged() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan("fan-1", ControlMode::Pwm, "/sys/class/hwmon/hwmon0/pwm1");

        assert!(!owned.owns("fan-unmanaged"));
        assert_eq!(owned.len(), 1);
    }

    #[test]
    fn owned_fan_set_release_all() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan("fan-1", ControlMode::Pwm, "/sys/class/hwmon/hwmon0/pwm1");
        owned.claim_fan(
            "fan-2",
            ControlMode::Voltage,
            "/sys/class/hwmon/hwmon0/pwm2",
        );

        owned.release_all();
        assert!(owned.is_empty());
        assert!(!owned.owns("fan-1"));
        assert!(!owned.owns("fan-2"));
    }
}
