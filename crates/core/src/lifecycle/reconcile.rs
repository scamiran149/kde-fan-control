//! Boot reconciliation of persisted applied config against live inventory.
//!
//! At startup the daemon must verify that every fan in the applied
//! config still exists, is enrollable, supports the configured
//! control mode, and has valid temperature sources. Fans that fail
//! any check are skipped and marked degraded. The reconciled config
//! contains only the safe subset.
//!
//! This module also provides the `perform_boot_reconciliation` entry
//! point which loads the persisted config, runs reconciliation,
//! updates the owned-fan set, and records lifecycle events.

use serde::{Deserialize, Serialize};

use crate::config::{AppliedConfig, AppliedFanEntry};
use crate::inventory::{ControlMode, FanChannel, InventorySnapshot, SupportState};

use super::owned::OwnedFanSet;
use super::state::{DegradedReason, DegradedState, LifecycleEvent, LifecycleEventLog};
use super::time::format_iso8601_now;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReconcileOutcome {
    Restored {
        fan_id: String,
        control_mode: ControlMode,
        temp_sources: Vec<String>,
    },
    Missing {
        fan_id: String,
    },
    NotEnrollable {
        fan_id: String,
        support_state: SupportState,
        reason: String,
    },
    ControlModeUnavailable {
        fan_id: String,
        configured_mode: ControlMode,
        available_modes: Vec<ControlMode>,
    },
    TempSourceMissing {
        fan_id: String,
        missing_temp_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileResult {
    pub restored: Vec<ReconcileOutcome>,
    pub skipped: Vec<ReconcileOutcome>,
    pub reconciled_config: AppliedConfig,
    pub degraded_reasons: Vec<(String, DegradedReason)>,
}

pub fn reconcile_applied_config(
    applied: &AppliedConfig,
    snapshot: &InventorySnapshot,
) -> ReconcileResult {
    let mut restored = Vec::new();
    let mut skipped = Vec::new();
    let mut degraded_reasons = Vec::new();
    let mut reconciled_fans = std::collections::HashMap::new();

    for (fan_id, applied_entry) in &applied.fans {
        let Some(fan) = find_fan_in_snapshot(snapshot, fan_id) else {
            let outcome = ReconcileOutcome::Missing {
                fan_id: fan_id.clone(),
            };
            degraded_reasons.push((
                fan_id.clone(),
                DegradedReason::FanMissing {
                    fan_id: fan_id.clone(),
                },
            ));
            skipped.push(outcome);
            continue;
        };

        if fan.support_state != SupportState::Available {
            let reason = fan
                .support_reason
                .clone()
                .unwrap_or_else(|| "unsupported hardware".to_string());
            let outcome = ReconcileOutcome::NotEnrollable {
                fan_id: fan_id.clone(),
                support_state: fan.support_state,
                reason: reason.clone(),
            };
            degraded_reasons.push((
                fan_id.clone(),
                DegradedReason::FanNoLongerEnrollable {
                    fan_id: fan_id.clone(),
                    support_state: fan.support_state,
                    reason,
                },
            ));
            skipped.push(outcome);
            continue;
        }

        if !fan.control_modes.contains(&applied_entry.control_mode) {
            let outcome = ReconcileOutcome::ControlModeUnavailable {
                fan_id: fan_id.clone(),
                configured_mode: applied_entry.control_mode,
                available_modes: fan.control_modes.clone(),
            };
            degraded_reasons.push((
                fan_id.clone(),
                DegradedReason::ControlModeUnavailable {
                    fan_id: fan_id.clone(),
                    mode: applied_entry.control_mode,
                },
            ));
            skipped.push(outcome);
            continue;
        }

        let mut temp_missing = false;
        for temp_id in &applied_entry.temp_sources {
            if !temp_source_in_snapshot(snapshot, temp_id) {
                let outcome = ReconcileOutcome::TempSourceMissing {
                    fan_id: fan_id.clone(),
                    missing_temp_id: temp_id.clone(),
                };
                degraded_reasons.push((
                    fan_id.clone(),
                    DegradedReason::TempSourceMissing {
                        fan_id: fan_id.clone(),
                        temp_id: temp_id.clone(),
                    },
                ));
                skipped.push(outcome);
                temp_missing = true;
                break;
            }
        }
        if temp_missing {
            continue;
        }

        let outcome = ReconcileOutcome::Restored {
            fan_id: fan_id.clone(),
            control_mode: applied_entry.control_mode,
            temp_sources: applied_entry.temp_sources.clone(),
        };
        reconciled_fans.insert(
            fan_id.clone(),
            AppliedFanEntry {
                control_mode: applied_entry.control_mode,
                temp_sources: applied_entry.temp_sources.clone(),
                target_temp_millidegrees: applied_entry.target_temp_millidegrees,
                aggregation: applied_entry.aggregation,
                pid_gains: applied_entry.pid_gains,
                cadence: applied_entry.cadence,
                deadband_millidegrees: applied_entry.deadband_millidegrees,
                actuator_policy: applied_entry.actuator_policy,
                pid_limits: applied_entry.pid_limits,
            },
        );
        restored.push(outcome);
    }

    let reconciled_config = AppliedConfig {
        fans: reconciled_fans,
        applied_at: applied.applied_at.clone(),
    };

    ReconcileResult {
        restored,
        skipped,
        reconciled_config,
        degraded_reasons,
    }
}

pub(super) fn find_fan_in_snapshot<'a>(
    snapshot: &'a InventorySnapshot,
    fan_id: &str,
) -> Option<&'a FanChannel> {
    snapshot
        .devices
        .iter()
        .flat_map(|d| d.fans.iter())
        .find(|f| f.id == fan_id)
}

pub(super) fn temp_source_in_snapshot(snapshot: &InventorySnapshot, temp_id: &str) -> bool {
    snapshot
        .devices
        .iter()
        .flat_map(|d| d.temperatures.iter())
        .any(|t| t.id == temp_id)
}

pub fn perform_boot_reconciliation(
    applied_config: Option<&AppliedConfig>,
    snapshot: &InventorySnapshot,
    owned: &mut OwnedFanSet,
    degraded: &mut DegradedState,
    events: &mut LifecycleEventLog,
) -> ReconcileResult {
    owned.release_all();
    degraded.clear_all();

    let Some(applied) = applied_config else {
        return ReconcileResult {
            restored: vec![],
            skipped: vec![],
            reconciled_config: AppliedConfig {
                fans: std::collections::HashMap::new(),
                applied_at: None,
            },
            degraded_reasons: vec![],
        };
    };

    if applied.fans.is_empty() {
        return ReconcileResult {
            restored: vec![],
            skipped: vec![],
            reconciled_config: AppliedConfig {
                fans: std::collections::HashMap::new(),
                applied_at: applied.applied_at.clone(),
            },
            degraded_reasons: vec![],
        };
    }

    let result = reconcile_applied_config(applied, snapshot);

    for outcome in &result.restored {
        if let ReconcileOutcome::Restored {
            fan_id,
            control_mode,
            ..
        } = outcome
        {
            let sysfs_path = snapshot
                .devices
                .iter()
                .flat_map(|d| d.fans.iter())
                .find(|f| &f.id == fan_id)
                .map(|f| {
                    let device_path = snapshot
                        .devices
                        .iter()
                        .find(|d| d.fans.iter().any(|f| &f.id == fan_id))
                        .map(|d| d.sysfs_path.as_str())
                        .unwrap_or("");
                    format!("{}/pwm{}", device_path, f.channel)
                })
                .unwrap_or_default();

            owned.claim_fan(fan_id, *control_mode, &sysfs_path);
        }
    }

    for (fan_id, reason) in &result.degraded_reasons {
        degraded.mark_degraded(fan_id.clone(), vec![reason.clone()]);
    }

    let timestamp = format_iso8601_now();

    for outcome in &result.restored {
        if let ReconcileOutcome::Restored { fan_id, .. } = outcome {
            events.push(LifecycleEvent {
                timestamp: timestamp.clone(),
                reason: DegradedReason::BootRestored {
                    fan_id: fan_id.clone(),
                },
                detail: Some(format!("fan {fan_id} restored as managed on boot")),
            });
        }
    }

    for outcome in &result.skipped {
        let (fan_id, reason) = result
            .degraded_reasons
            .iter()
            .find(|(fid, _)| match outcome {
                ReconcileOutcome::Missing { fan_id: id } => fid == id,
                ReconcileOutcome::NotEnrollable { fan_id: id, .. } => fid == id,
                ReconcileOutcome::ControlModeUnavailable { fan_id: id, .. } => fid == id,
                ReconcileOutcome::TempSourceMissing { fan_id: id, .. } => fid == id,
                _ => false,
            })
            .cloned()
            .unwrap_or_else(|| {
                (
                    "unknown".to_string(),
                    DegradedReason::FanMissing {
                        fan_id: "unknown".to_string(),
                    },
                )
            });

        events.push(LifecycleEvent {
            timestamp: timestamp.clone(),
            reason,
            detail: Some(format!("fan {fan_id} skipped during boot reconciliation")),
        });
    }

    if !result.skipped.is_empty() {
        events.push(LifecycleEvent {
            timestamp: timestamp.clone(),
            reason: DegradedReason::PartialBootRecovery {
                failed_count: result.skipped.len() as u32,
                recovered_count: result.restored.len() as u32,
            },
            detail: Some("boot reconciliation completed with partial recovery".into()),
        });
    }

    if !result.restored.is_empty() && result.skipped.is_empty() {
        events.push(LifecycleEvent {
            timestamp,
            reason: DegradedReason::BootReconciled {
                restored_count: result.restored.len() as u32,
            },
            detail: Some(format!(
                "all {} managed fans restored successfully on boot",
                result.restored.len()
            )),
        });
    }

    result
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::AppliedFanEntry;
    use crate::control::{ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits};
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
                    support_state: SupportState::Available,
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
                support_state: SupportState::Available,
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
    fn reconcile_exact_match_restore() {
        let snapshot = test_snapshot();
        let applied = applied_config_single_fan();

        let result = reconcile_applied_config(&applied, &snapshot);

        assert_eq!(result.restored.len(), 1);
        assert!(result.skipped.is_empty());
        assert!(result.degraded_reasons.is_empty());

        match &result.restored[0] {
            ReconcileOutcome::Restored {
                fan_id,
                control_mode,
                temp_sources,
            } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
                assert_eq!(*control_mode, ControlMode::Pwm);
                assert_eq!(
                    temp_sources,
                    &["hwmon-test-0000000000000001-temp1".to_string()]
                );
            }
            _ => panic!("expected Restored outcome"),
        }

        assert!(
            result
                .reconciled_config
                .fans
                .contains_key("hwmon-test-0000000000000001-fan1")
        );
    }

    #[test]
    fn reconcile_missing_fan_id() {
        let snapshot = InventorySnapshot {
            devices: vec![HwmonDevice {
                id: "hwmon-test-0000000000000001".to_string(),
                name: "testchip".to_string(),
                sysfs_path: "/sys/class/hwmon/hwmon0".to_string(),
                stable_identity: "/sys/devices/platform/testchip".to_string(),
                temperatures: vec![],
                fans: vec![],
            }],
        };

        let applied = applied_config_single_fan();

        let result = reconcile_applied_config(&applied, &snapshot);

        assert!(result.restored.is_empty());
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.degraded_reasons.len(), 1);

        match &result.skipped[0] {
            ReconcileOutcome::Missing { fan_id } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
            }
            _ => panic!("expected Missing outcome"),
        }

        match &result.degraded_reasons[0] {
            (fid, DegradedReason::FanMissing { fan_id }) => {
                assert_eq!(fid, "hwmon-test-0000000000000001-fan1");
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
            }
            _ => panic!("expected FanMissing degraded reason"),
        }

        assert!(result.reconciled_config.fans.is_empty());
    }

    #[test]
    fn reconcile_changed_support_state() {
        let mut snapshot = test_snapshot();
        snapshot.devices[0].fans[0].support_state = SupportState::Partial;
        snapshot.devices[0].fans[0].support_reason = Some("pwm node not writable".to_string());
        snapshot.devices[0].fans[0].control_modes.clear();

        let applied = applied_config_single_fan();

        let result = reconcile_applied_config(&applied, &snapshot);

        assert!(result.restored.is_empty());
        assert_eq!(result.skipped.len(), 1);

        match &result.skipped[0] {
            ReconcileOutcome::NotEnrollable {
                fan_id,
                support_state,
                reason,
            } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
                assert_eq!(*support_state, SupportState::Partial);
                assert!(reason.contains("pwm"));
            }
            _ => panic!("expected NotEnrollable outcome"),
        }
    }

    #[test]
    fn reconcile_changed_control_mode() {
        let mut snapshot = test_snapshot();
        snapshot.devices[0].fans[0].support_state = SupportState::Available;
        snapshot.devices[0].fans[0].control_modes = vec![ControlMode::Voltage];

        let applied = applied_config_single_fan();

        let result = reconcile_applied_config(&applied, &snapshot);

        assert!(result.restored.is_empty());
        assert_eq!(result.skipped.len(), 1);

        match &result.skipped[0] {
            ReconcileOutcome::ControlModeUnavailable {
                fan_id,
                configured_mode,
                available_modes,
            } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
                assert_eq!(*configured_mode, ControlMode::Pwm);
                assert_eq!(*available_modes, vec![ControlMode::Voltage]);
            }
            _ => panic!("expected ControlModeUnavailable outcome"),
        }
    }

    #[test]
    fn reconcile_missing_temp_source() {
        let mut snapshot = test_snapshot();
        snapshot.devices[0].temperatures.clear();

        let applied = applied_config_single_fan();

        let result = reconcile_applied_config(&applied, &snapshot);

        assert!(result.restored.is_empty());
        assert_eq!(result.skipped.len(), 1);

        match &result.skipped[0] {
            ReconcileOutcome::TempSourceMissing {
                fan_id,
                missing_temp_id,
            } => {
                assert_eq!(fan_id, "hwmon-test-0000000000000001-fan1");
                assert_eq!(missing_temp_id, "hwmon-test-0000000000000001-temp1");
            }
            _ => panic!("expected TempSourceMissing outcome"),
        }
    }

    #[test]
    fn reconcile_partial_restore() {
        let snapshot = test_snapshot_with_second_device();

        let applied = AppliedConfig {
            fans: {
                let mut m = HashMap::new();
                m.insert(
                    "hwmon-test-0000000000000001-fan1".to_string(),
                    applied_fan_entry(vec!["hwmon-test-0000000000000001-temp1".to_string()]),
                );
                m.insert(
                    "hwmon-ghost-0000000000000003-fan1".to_string(),
                    applied_fan_entry(vec![]),
                );
                m
            },
            applied_at: Some("2026-04-11T12:00:00Z".to_string()),
        };

        let result = reconcile_applied_config(&applied, &snapshot);

        assert_eq!(result.restored.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.degraded_reasons.len(), 1);

        assert!(
            result
                .reconciled_config
                .fans
                .contains_key("hwmon-test-0000000000000001-fan1")
        );
        assert!(
            !result
                .reconciled_config
                .fans
                .contains_key("hwmon-ghost-0000000000000003-fan1")
        );
    }

    #[test]
    fn boot_reconciliation_restores_matching_fans() {
        let snapshot = test_snapshot();
        let applied = applied_config_single_fan();
        let mut owned = OwnedFanSet::new();
        let mut degraded = DegradedState::new();
        let mut events = LifecycleEventLog::new();

        let result = perform_boot_reconciliation(
            Some(&applied),
            &snapshot,
            &mut owned,
            &mut degraded,
            &mut events,
        );

        assert_eq!(result.restored.len(), 1);
        assert!(result.skipped.is_empty());
        assert!(owned.owns("hwmon-test-0000000000000001-fan1"));
        assert!(!degraded.has_degraded());
        assert!(!events.is_empty());
        assert!(matches!(
            events.events().last().map(|event| &event.reason),
            Some(DegradedReason::BootReconciled { restored_count: 1 })
        ));
    }

    #[test]
    fn boot_reconciliation_skips_missing_fans() {
        let snapshot = InventorySnapshot {
            devices: vec![HwmonDevice {
                id: "hwmon-test-0000000000000001".to_string(),
                name: "testchip".to_string(),
                sysfs_path: "/sys/class/hwmon/hwmon0".to_string(),
                stable_identity: "/sys/devices/platform/testchip".to_string(),
                temperatures: vec![],
                fans: vec![],
            }],
        };
        let applied = applied_config_single_fan();
        let mut owned = OwnedFanSet::new();
        let mut degraded = DegradedState::new();
        let mut events = LifecycleEventLog::new();

        let result = perform_boot_reconciliation(
            Some(&applied),
            &snapshot,
            &mut owned,
            &mut degraded,
            &mut events,
        );

        assert!(result.restored.is_empty());
        assert_eq!(result.skipped.len(), 1);
        assert!(!owned.owns("hwmon-test-0000000000000001-fan1"));
        assert!(degraded.has_degraded());
        assert!(!events.is_empty());
    }

    #[test]
    fn boot_reconciliation_no_applied_config() {
        let snapshot = test_snapshot();
        let mut owned = OwnedFanSet::new();
        let mut degraded = DegradedState::new();
        let mut events = LifecycleEventLog::new();

        let result =
            perform_boot_reconciliation(None, &snapshot, &mut owned, &mut degraded, &mut events);

        assert!(result.restored.is_empty());
        assert!(result.skipped.is_empty());
        assert!(owned.is_empty());
    }

    #[test]
    fn boot_reconciliation_empty_applied_config() {
        let snapshot = test_snapshot();
        let applied = AppliedConfig {
            fans: HashMap::new(),
            applied_at: Some("2026-04-11T12:00:00Z".to_string()),
        };
        let mut owned = OwnedFanSet::new();
        let mut degraded = DegradedState::new();
        let mut events = LifecycleEventLog::new();

        let result = perform_boot_reconciliation(
            Some(&applied),
            &snapshot,
            &mut owned,
            &mut degraded,
            &mut events,
        );

        assert!(result.restored.is_empty());
        assert!(result.skipped.is_empty());
        assert!(owned.is_empty());
    }
}
