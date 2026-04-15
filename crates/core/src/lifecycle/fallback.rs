//! Safe-maximum fallback write for owned fans.
//!
//! When fans must be driven to PWM 255 (e.g., on shutdown, crash,
//! or service failure), this module provides `write_fallback_for_owned`
//! and `write_fallback_single` to write `pwm_enable=1` and `pwm=255`
//! to sysfs. It also converts fallback results into `LifecycleEvent`
//! and `FallbackIncident` records.
//!
//! **Safety invariant**: every owned fan must have fallback attempted
//! before the daemon process terminates.

use serde::{Deserialize, Serialize};

use crate::config::{FallbackFailure, FallbackIncident};

use super::owned::OwnedFanSet;
use super::state::{DegradedReason, LifecycleEvent};

pub const PWM_SAFE_MAX: u32 = 255;
pub const PWM_ENABLE_MANUAL: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<(String, String)>,
}

impl FallbackResult {
    pub fn all_succeeded(&self) -> bool {
        self.failed.is_empty()
    }
}

impl FallbackIncident {
    pub fn from_owned_and_result(
        timestamp: String,
        owned: &OwnedFanSet,
        result: &FallbackResult,
        detail: Option<String>,
    ) -> Self {
        let mut affected_fans: Vec<String> = owned.owned_fan_ids().map(str::to_string).collect();
        affected_fans.sort();

        let failed = result
            .failed
            .iter()
            .map(|(fan_id, error)| FallbackFailure {
                fan_id: fan_id.clone(),
                error: error.clone(),
            })
            .collect();

        Self {
            timestamp,
            affected_fans,
            failed,
            detail,
        }
    }
}

pub fn lifecycle_event_from_fallback_incident(incident: &FallbackIncident) -> LifecycleEvent {
    LifecycleEvent {
        timestamp: incident.timestamp.clone(),
        reason: DegradedReason::FallbackActive {
            affected_fans: incident.affected_fans.clone(),
        },
        detail: incident.detail.clone(),
    }
}

pub fn write_fallback_for_owned(owned: &OwnedFanSet) -> FallbackResult {
    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for fan_id in owned.owned_fan_ids() {
        let pwm_path = match owned.sysfs_path(fan_id) {
            Some(path) => path.to_string(),
            None => {
                failed.push((
                    fan_id.to_string(),
                    "no sysfs path recorded for owned fan".into(),
                ));
                continue;
            }
        };

        let pwm_enable_path = {
            let path = std::path::Path::new(&pwm_path);
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            let enable_name = format!("{file_name}_enable");
            path.with_file_name(enable_name)
                .to_string_lossy()
                .to_string()
        };

        if let Err(e) = std::fs::write(&pwm_enable_path, PWM_ENABLE_MANUAL.to_string()) {
            tracing::warn!(
                fan_id = fan_id,
                path = %pwm_enable_path,
                error = %e,
                "could not set pwm_enable to manual mode during fallback"
            );
        }

        match std::fs::write(&pwm_path, PWM_SAFE_MAX.to_string()) {
            Ok(()) => {
                tracing::info!(
                    fan_id = fan_id,
                    pwm_path = %pwm_path,
                    "fallback: set fan to safe maximum (pwm=255)"
                );
                succeeded.push(fan_id.to_string());
            }
            Err(e) => {
                tracing::error!(
                    fan_id = fan_id,
                    pwm_path = %pwm_path,
                    error = %e,
                    "fallback: FAILED to set fan to safe maximum"
                );
                failed.push((fan_id.to_string(), format!("pwm write failed: {e}")));
            }
        }
    }

    FallbackResult { succeeded, failed }
}

pub fn write_fallback_single(fan_id: &str, owned: &OwnedFanSet) -> Result<(), String> {
    if !owned.owns(fan_id) {
        return Ok(());
    }

    let pwm_path = match owned.sysfs_path(fan_id) {
        Some(path) => path.to_string(),
        None => return Err("no sysfs path recorded for owned fan".into()),
    };

    let pwm_enable_path = {
        let path = std::path::Path::new(&pwm_path);
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        let enable_name = format!("{file_name}_enable");
        path.with_file_name(enable_name)
            .to_string_lossy()
            .to_string()
    };

    if let Err(e) = std::fs::write(&pwm_enable_path, PWM_ENABLE_MANUAL.to_string()) {
        tracing::warn!(
            fan_id = fan_id,
            path = %pwm_enable_path,
            error = %e,
            "could not set pwm_enable to manual mode during single-fan fallback"
        );
    }

    std::fs::write(&pwm_path, PWM_SAFE_MAX.to_string())
        .map_err(|e| format!("pwm write failed for {fan_id}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ControlMode;

    #[test]
    fn fallback_incident_records_only_owned_fans() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            "/sys/class/hwmon/hwmon0/pwm1",
        );

        let incident = FallbackIncident::from_owned_and_result(
            "2026-04-11T16:30:00Z".to_string(),
            &owned,
            &FallbackResult {
                succeeded: vec!["hwmon-test-0000000000000001-fan1".to_string()],
                failed: vec![],
            },
            Some("ctrl-c fallback".to_string()),
        );

        assert_eq!(
            incident.affected_fans,
            vec!["hwmon-test-0000000000000001-fan1"]
        );
        assert!(
            !incident
                .affected_fans
                .iter()
                .any(|fan| fan == "fan-unmanaged")
        );
    }
}
