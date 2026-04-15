//! Runtime state types for live fan status reporting.
//!
//! Defines `ControlRuntimeSnapshot`, `FanRuntimeStatus`, and
//! `RuntimeState`. These are the DBus-facing projections of the
//! daemon's live control-loop state: managed/degraded/fallback/
//! unmanaged per fan, with sensor readings and output percentages.
//!
//! This module must not perform I/O; it constructs snapshots from
//! already-resolved data.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::config::AppliedConfig;
use crate::control::AggregationFn;
use crate::inventory::{ControlMode, InventorySnapshot};

use super::owned::OwnedFanSet;
use super::state::{DegradedReason, DegradedState};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ControlRuntimeSnapshot {
    pub sensor_ids: Vec<String>,
    pub aggregation: AggregationFn,
    pub target_temp_millidegrees: i64,
    pub aggregated_temp_millidegrees: Option<i64>,
    pub logical_output_percent: Option<f64>,
    pub mapped_pwm: Option<u16>,
    pub auto_tuning: bool,
    pub alert_high_temp: bool,
    pub last_error_millidegrees: Option<i64>,
}

impl ControlRuntimeSnapshot {
    pub(crate) fn from_applied_entry(entry: &crate::config::AppliedFanEntry) -> Self {
        Self {
            sensor_ids: entry.temp_sources.clone(),
            aggregation: entry.aggregation,
            target_temp_millidegrees: entry.target_temp_millidegrees,
            aggregated_temp_millidegrees: None,
            logical_output_percent: None,
            mapped_pwm: None,
            auto_tuning: false,
            alert_high_temp: false,
            last_error_millidegrees: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FanRuntimeStatus {
    Unmanaged,
    Managed {
        control_mode: ControlMode,
        control: ControlRuntimeSnapshot,
    },
    Degraded {
        reasons: Vec<DegradedReason>,
    },
    Fallback,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeState {
    pub fan_statuses: HashMap<String, FanRuntimeStatus>,
    pub owned_fans: Vec<String>,
}

impl RuntimeState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn build(
        owned: &OwnedFanSet,
        applied: Option<&AppliedConfig>,
        degraded: &DegradedState,
        fallback_fan_ids: &HashSet<String>,
        snapshot: &InventorySnapshot,
    ) -> Self {
        let mut fan_statuses = HashMap::new();

        for device in &snapshot.devices {
            for fan in &device.fans {
                fan_statuses.insert(fan.id.clone(), FanRuntimeStatus::Unmanaged);
            }
        }

        for fan_id in owned.owned_fan_ids() {
            if let Some(mode) = owned.control_mode(fan_id) {
                let control = applied
                    .and_then(|config| config.fans.get(fan_id))
                    .map(ControlRuntimeSnapshot::from_applied_entry)
                    .unwrap_or_default();
                fan_statuses.insert(
                    fan_id.to_string(),
                    FanRuntimeStatus::Managed {
                        control_mode: mode,
                        control,
                    },
                );
            }
        }

        for (fan_id, reasons) in &degraded.entries {
            fan_statuses.insert(
                fan_id.clone(),
                FanRuntimeStatus::Degraded {
                    reasons: reasons.clone(),
                },
            );
        }

        for fan_id in fallback_fan_ids {
            fan_statuses.insert(fan_id.clone(), FanRuntimeStatus::Fallback);
        }

        let owned_fans: Vec<String> = owned.owned_fan_ids().map(|s| s.to_string()).collect();

        RuntimeState {
            fan_statuses,
            owned_fans,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::{AppliedConfig, AppliedFanEntry, FallbackIncident};
    use crate::control::{ActuatorPolicy, ControlCadence, PidGains, PidLimits};
    use crate::inventory::{HwmonDevice, TemperatureSensor};

    fn test_snapshot() -> InventorySnapshot {
        InventorySnapshot {
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

    fn test_snapshot_with_second_device() -> InventorySnapshot {
        let mut snapshot = test_snapshot();
        snapshot.devices.push(HwmonDevice {
            id: "hwmon-other-0000000000000002".to_string(),
            name: "otherchip".to_string(),
            sysfs_path: "/sys/class/hwmon/hwmon1".to_string(),
            stable_identity: "/sys/devices/platform/otherchip".to_string(),
            temperatures: vec![],
            fans: vec![crate::inventory::FanChannel {
                id: "hwmon-other-0000000000000002-fan1".to_string(),
                channel: 1,
                label: Some("Case Fan".to_string()),
                friendly_name: None,
                rpm_feedback: false,
                current_rpm: None,
                control_modes: vec![ControlMode::Pwm],
                support_state: crate::inventory::SupportState::Available,
                support_reason: None,
            }],
        });
        snapshot
    }

    fn applied_config_single_fan() -> AppliedConfig {
        AppliedConfig {
            fans: {
                let mut m = HashMap::new();
                m.insert(
                    "hwmon-test-0000000000000001-fan1".to_string(),
                    applied_fan_entry(vec!["hwmon-test-0000000000000001-temp1".to_string()]),
                );
                m
            },
            applied_at: Some("2026-04-11T12:00:00Z".to_string()),
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
    fn runtime_state_build_managed_and_unmanaged() {
        let snapshot = test_snapshot();
        let applied = applied_config_single_fan();
        let mut owned = OwnedFanSet::new();
        owned.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            "/sys/class/hwmon/hwmon0/pwm1",
        );
        let degraded = DegradedState::new();
        let fallback = HashSet::new();

        let state = RuntimeState::build(&owned, Some(&applied), &degraded, &fallback, &snapshot);

        match state.fan_statuses.get("hwmon-test-0000000000000001-fan1") {
            Some(FanRuntimeStatus::Managed {
                control_mode,
                control,
            }) => {
                assert_eq!(*control_mode, ControlMode::Pwm);
                assert_eq!(
                    control.sensor_ids,
                    vec!["hwmon-test-0000000000000001-temp1".to_string()]
                );
                assert_eq!(control.target_temp_millidegrees, 65_000);
            }
            _ => panic!("expected Managed status for fan1"),
        }
        assert!(
            state
                .owned_fans
                .contains(&"hwmon-test-0000000000000001-fan1".to_string())
        );
    }

    #[test]
    fn runtime_state_build_degraded_fan() {
        let snapshot = test_snapshot();
        let applied = applied_config_single_fan();
        let owned = OwnedFanSet::new();
        let mut degraded = DegradedState::new();
        degraded.mark_degraded(
            "hwmon-test-0000000000000001-fan1".to_string(),
            vec![DegradedReason::FanMissing {
                fan_id: "hwmon-test-0000000000000001-fan1".to_string(),
            }],
        );
        let fallback = HashSet::new();

        let state = RuntimeState::build(&owned, Some(&applied), &degraded, &fallback, &snapshot);

        match state.fan_statuses.get("hwmon-test-0000000000000001-fan1") {
            Some(FanRuntimeStatus::Degraded { reasons }) => {
                assert!(!reasons.is_empty());
            }
            _ => panic!("expected Degraded status for fan1"),
        }
    }

    #[test]
    fn runtime_state_build_fallback_fan() {
        let snapshot = test_snapshot();
        let applied = applied_config_single_fan();
        let owned = OwnedFanSet::new();
        let degraded = DegradedState::new();
        let mut fallback = HashSet::new();
        fallback.insert("hwmon-test-0000000000000001-fan1".to_string());

        let state = RuntimeState::build(&owned, Some(&applied), &degraded, &fallback, &snapshot);

        match state.fan_statuses.get("hwmon-test-0000000000000001-fan1") {
            Some(FanRuntimeStatus::Fallback) => {}
            _ => panic!("expected Fallback status for fan1"),
        }
    }

    #[test]
    fn runtime_state_rebuild_marks_persisted_fallback_after_restart() {
        let snapshot = test_snapshot();
        let applied = applied_config_single_fan();
        let mut owned = OwnedFanSet::new();
        owned.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            "/sys/class/hwmon/hwmon0/pwm1",
        );
        let degraded = DegradedState::new();
        let fallback = FallbackIncident {
            timestamp: "2026-04-11T16:30:00Z".to_string(),
            affected_fans: vec!["hwmon-test-0000000000000001-fan1".to_string()],
            failed: vec![],
            detail: Some("persisted after panic".to_string()),
        };

        let state = RuntimeState::build(
            &owned,
            Some(&applied),
            &degraded,
            &fallback.fallback_fan_ids(),
            &snapshot,
        );

        match state.fan_statuses.get("hwmon-test-0000000000000001-fan1") {
            Some(FanRuntimeStatus::Fallback) => {}
            other => panic!("expected Fallback after restart, got {other:?}"),
        }
    }

    #[test]
    fn lifecycle_runtime_snapshot_serializes_control_payload() {
        let status = FanRuntimeStatus::Managed {
            control_mode: ControlMode::Pwm,
            control: ControlRuntimeSnapshot {
                sensor_ids: vec!["temp-a".to_string(), "temp-b".to_string()],
                aggregation: AggregationFn::Max,
                target_temp_millidegrees: 70_000,
                aggregated_temp_millidegrees: Some(72_500),
                logical_output_percent: Some(62.5),
                mapped_pwm: Some(159),
                auto_tuning: true,
                alert_high_temp: true,
                last_error_millidegrees: Some(2_500),
            },
        };

        let serialized = toml::to_string(&status).expect("status should serialize");
        assert!(serialized.contains("logical_output_percent"));
        assert!(serialized.contains("mapped_pwm"));
        assert!(serialized.contains("aggregation"));
    }

    #[test]
    fn runtime_state_all_unmanaged_by_default() {
        let snapshot = test_snapshot_with_second_device();
        let owned = OwnedFanSet::new();
        let degraded = DegradedState::new();
        let fallback = HashSet::new();

        let state = RuntimeState::build(&owned, None, &degraded, &fallback, &snapshot);

        for (fan_id, status) in &state.fan_statuses {
            assert!(
                matches!(status, FanRuntimeStatus::Unmanaged),
                "fan {fan_id} should be Unmanaged, got {status:?}"
            );
        }
    }
}
