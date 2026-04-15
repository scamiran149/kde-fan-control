//! Re-assessment of previously degraded fans.
//!
//! Periodically, the daemon re-checks degraded fans against the
//! current hardware inventory. If a fan's hardware has returned
//! (e.g., hot-plug), `reassess_single_fan` returns `Recoverable`
//! and the daemon may restore PID control. Otherwise it returns
//! `StillDegraded` with the reason.

use serde::{Deserialize, Serialize};

use crate::config::AppliedFanEntry;
use crate::inventory::{ControlMode, InventorySnapshot, SupportState};

use super::reconcile::{find_fan_in_snapshot, temp_source_in_snapshot};
use super::state::DegradedReason;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReassessOutcome {
    Recoverable {
        fan_id: String,
        control_mode: ControlMode,
        temp_sources: Vec<String>,
    },
    StillDegraded {
        fan_id: String,
        reason: DegradedReason,
    },
}

pub fn reassess_single_fan(
    fan_id: &str,
    applied_entry: &AppliedFanEntry,
    snapshot: &InventorySnapshot,
) -> ReassessOutcome {
    let Some(fan) = find_fan_in_snapshot(snapshot, fan_id) else {
        return ReassessOutcome::StillDegraded {
            fan_id: fan_id.to_string(),
            reason: DegradedReason::FanMissing {
                fan_id: fan_id.to_string(),
            },
        };
    };

    if fan.support_state != SupportState::Available {
        let reason = fan
            .support_reason
            .clone()
            .unwrap_or_else(|| "unsupported hardware".to_string());
        return ReassessOutcome::StillDegraded {
            fan_id: fan_id.to_string(),
            reason: DegradedReason::FanNoLongerEnrollable {
                fan_id: fan_id.to_string(),
                support_state: fan.support_state,
                reason,
            },
        };
    }

    if !fan.control_modes.contains(&applied_entry.control_mode) {
        return ReassessOutcome::StillDegraded {
            fan_id: fan_id.to_string(),
            reason: DegradedReason::ControlModeUnavailable {
                fan_id: fan_id.to_string(),
                mode: applied_entry.control_mode,
            },
        };
    }

    for temp_id in &applied_entry.temp_sources {
        if !temp_source_in_snapshot(snapshot, temp_id) {
            return ReassessOutcome::StillDegraded {
                fan_id: fan_id.to_string(),
                reason: DegradedReason::TempSourceMissing {
                    fan_id: fan_id.to_string(),
                    temp_id: temp_id.clone(),
                },
            };
        }
    }

    ReassessOutcome::Recoverable {
        fan_id: fan_id.to_string(),
        control_mode: applied_entry.control_mode,
        temp_sources: applied_entry.temp_sources.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits};
    use crate::inventory::{HwmonDevice, TemperatureSensor};

    fn test_snapshot() -> crate::inventory::InventorySnapshot {
        crate::inventory::InventorySnapshot {
            devices: vec![HwmonDevice {
                id: "hwmon-test-0000000000000001".to_string(),
                name: "testchip".to_string(),
                sysfs_path: "/sys/class/hwmon/hwmon0".to_string(),
                stable_identity: "/sys/devices/platform/testchip".to_string(),
                temperatures: vec![TemperatureSensor {
                    id: "hwmon-test-0000000000000001-temp1".to_string(),
                    channel: 1,
                    label: Some("CPU".to_string()),
                    friendly_name: None,
                    input_millidegrees_celsius: Some(45000),
                }],
                fans: vec![crate::inventory::FanChannel {
                    id: "hwmon-test-0000000000000001-fan1".to_string(),
                    channel: 1,
                    label: Some("CPU Fan".to_string()),
                    friendly_name: None,
                    rpm_feedback: true,
                    current_rpm: Some(1200),
                    control_modes: vec![ControlMode::Pwm],
                    support_state: crate::inventory::SupportState::Available,
                    support_reason: None,
                }],
            }],
        }
    }

    fn applied_fan_entry(temp_sources: Vec<String>) -> AppliedFanEntry {
        AppliedFanEntry {
            control_mode: ControlMode::Pwm,
            temp_sources,
            target_temp_millidegrees: 65_000,
            aggregation: AggregationFn::Average,
            pid_gains: PidGains::default(),
            cadence: ControlCadence::default(),
            deadband_millidegrees: 1_000,
            actuator_policy: ActuatorPolicy::default(),
            pid_limits: PidLimits::default(),
        }
    }

    #[test]
    fn reassess_single_fan_recovers_when_temp_source_returns() {
        let entry = applied_fan_entry(vec!["hwmon-test-0000000000000001-temp1".to_string()]);
        let mut snapshot_no_temp = test_snapshot();
        snapshot_no_temp.devices[0].temperatures.clear();

        let result = reassess_single_fan(
            "hwmon-test-0000000000000001-fan1",
            &entry,
            &snapshot_no_temp,
        );
        match result {
            ReassessOutcome::StillDegraded {
                fan_id,
                reason: DegradedReason::TempSourceMissing { .. },
            } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
            }
            other => panic!("expected StillDegraded(TempSourceMissing), got {other:?}"),
        }

        let snapshot_with_temp = test_snapshot();
        let result = reassess_single_fan(
            "hwmon-test-0000000000000001-fan1",
            &entry,
            &snapshot_with_temp,
        );
        match result {
            ReassessOutcome::Recoverable {
                fan_id,
                control_mode,
                temp_sources,
            } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
                assert_eq!(control_mode, ControlMode::Pwm);
                assert_eq!(
                    temp_sources,
                    vec!["hwmon-test-0000000000000001-temp1".to_string()]
                );
            }
            other => panic!("expected Recoverable, got {other:?}"),
        }
    }

    #[test]
    fn reassess_single_fan_still_degraded_when_fan_missing() {
        let entry = applied_fan_entry(vec!["hwmon-test-0000000000000001-temp1".to_string()]);
        let snapshot_no_fan = crate::inventory::InventorySnapshot {
            devices: vec![HwmonDevice {
                id: "hwmon-test-0000000000000001".to_string(),
                name: "testchip".to_string(),
                sysfs_path: "/sys/class/hwmon/hwmon0".to_string(),
                stable_identity: "/sys/devices/platform/testchip".to_string(),
                temperatures: vec![TemperatureSensor {
                    id: "hwmon-test-0000000000000001-temp1".to_string(),
                    channel: 1,
                    label: Some("CPU".to_string()),
                    friendly_name: None,
                    input_millidegrees_celsius: Some(45000),
                }],
                fans: vec![],
            }],
        };

        let result =
            reassess_single_fan("hwmon-test-0000000000000001-fan1", &entry, &snapshot_no_fan);
        match result {
            ReassessOutcome::StillDegraded {
                fan_id,
                reason: DegradedReason::FanMissing { .. },
            } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
            }
            other => panic!("expected StillDegraded(FanMissing), got {other:?}"),
        }
    }

    #[test]
    fn reassess_single_fan_recovers_when_control_mode_available() {
        let entry = applied_fan_entry(vec!["hwmon-test-0000000000000001-temp1".to_string()]);
        let mut snapshot_no_pwm = test_snapshot();
        snapshot_no_pwm.devices[0].fans[0].control_modes.clear();

        let result =
            reassess_single_fan("hwmon-test-0000000000000001-fan1", &entry, &snapshot_no_pwm);
        match result {
            ReassessOutcome::StillDegraded {
                fan_id,
                reason: DegradedReason::ControlModeUnavailable { mode, .. },
            } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
                assert_eq!(mode, ControlMode::Pwm);
            }
            other => panic!("expected StillDegraded(ControlModeUnavailable), got {other:?}"),
        }

        let snapshot_with_pwm = test_snapshot();
        let result = reassess_single_fan(
            "hwmon-test-0000000000000001-fan1",
            &entry,
            &snapshot_with_pwm,
        );
        match result {
            ReassessOutcome::Recoverable { fan_id, .. } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
            }
            other => panic!("expected Recoverable, got {other:?}"),
        }
    }
}
