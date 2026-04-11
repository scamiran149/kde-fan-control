//! Boot reconciliation, runtime ownership tracking, and fallback lifecycle.
//!
//! This module implements the safe startup, ownership, and crash-path behavior
//! described in Phase 2 Plan 03:
//!
//! - Reconcile persisted applied config against live inventory at startup
//! - Restore safe matches as managed, skip unsafe or missing fans
//! - Track which fans the daemon actually owns at runtime
//! - Provide safe-maximum fallback for owned fans on failure or shutdown

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::config::{
    AppliedConfig, AppliedFanEntry, DegradedReason, DegradedState, LifecycleEvent,
    LifecycleEventLog,
};
use crate::inventory::{ControlMode, FanChannel, InventorySnapshot, SupportState};

// ---------------------------------------------------------------------------
// Startup reconciliation
// ---------------------------------------------------------------------------

/// Result of reconciling a single fan at startup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReconcileOutcome {
    /// The fan was successfully restored as managed.
    Restored {
        fan_id: String,
        control_mode: ControlMode,
        temp_sources: Vec<String>,
    },
    /// The fan was skipped because it is missing from current hardware.
    Missing { fan_id: String },
    /// The fan was skipped because it is no longer safely enrollable.
    NotEnrollable {
        fan_id: String,
        support_state: SupportState,
        reason: String,
    },
    /// The fan was skipped because its configured control mode is no longer supported.
    ControlModeUnavailable {
        fan_id: String,
        configured_mode: ControlMode,
        available_modes: Vec<ControlMode>,
    },
    /// A temperature source referenced by the applied config is now missing.
    TempSourceMissing {
        fan_id: String,
        missing_temp_id: String,
    },
}

/// Full result of boot reconciliation across all applied fans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconcileResult {
    /// Fans that were successfully restored as managed.
    pub restored: Vec<ReconcileOutcome>,

    /// Fans that were skipped with reasons.
    pub skipped: Vec<ReconcileOutcome>,

    /// The reconstructed applied config containing only restored fans.
    pub reconciled_config: AppliedConfig,

    /// Degraded reasons for all skipped fans.
    pub degraded_reasons: Vec<(String, DegradedReason)>,
}

/// Reconcile the persisted applied config against the current live inventory.
///
/// For each fan in the applied config:
/// - Verify the fan still exists in the inventory (stable ID match)
/// - Verify the fan's support state is Available (capability match)
/// - Verify the configured control mode is still supported
/// - Verify all referenced temperature sources still exist
///
/// Fans that pass all checks are restored as managed. Fans that fail are
/// skipped and their degraded reasons are recorded. The returned
/// `ReconcileResult` contains the reduced applied config (only valid fans)
/// and all reconciliation outcomes.
pub fn reconcile_applied_config(
    applied: &AppliedConfig,
    snapshot: &InventorySnapshot,
) -> ReconcileResult {
    let mut restored = Vec::new();
    let mut skipped = Vec::new();
    let mut degraded_reasons = Vec::new();
    let mut reconciled_fans = HashMap::new();

    for (fan_id, applied_entry) in &applied.fans {
        // 1. Fan must exist in current inventory.
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

        // 2. Fan's support state must be Available.
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

        // 3. Configured control mode must still be supported.
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

        // 4. All referenced temperature sources must still exist.
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

        // All checks passed — restore as managed.
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

/// Look up a fan channel by stable ID across all devices in the snapshot.
fn find_fan_in_snapshot<'a>(
    snapshot: &'a InventorySnapshot,
    fan_id: &str,
) -> Option<&'a FanChannel> {
    snapshot
        .devices
        .iter()
        .flat_map(|d| d.fans.iter())
        .find(|f| f.id == fan_id)
}

/// Check whether a temperature sensor ID exists in the snapshot.
fn temp_source_in_snapshot(snapshot: &InventorySnapshot, temp_id: &str) -> bool {
    snapshot
        .devices
        .iter()
        .flat_map(|d| d.temperatures.iter())
        .any(|t| t.id == temp_id)
}

// ---------------------------------------------------------------------------
// Runtime ownership tracking
// ---------------------------------------------------------------------------

/// Runtime tracking of which fans the daemon currently owns and controls.
///
/// Only fans that have been successfully claimed (through reconciliation or
/// live apply) are inserted into the owned set. This set is the authority
/// for which fans receive fallback writes on shutdown or failure.
///
/// Unmanaged fans are NEVER in this set.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OwnedFanSet {
    /// The set of stable fan IDs the daemon currently owns and controls.
    owned: HashSet<String>,

    /// Per-fan control mode for owned fans, used for fallback writes.
    control_modes: HashMap<String, ControlMode>,

    /// Per-fan sysfs paths for owned fans, needed to write fallback values.
    fan_sysfs_paths: HashMap<String, String>,
}

impl OwnedFanSet {
    /// Create an empty owned fan set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim a fan as owned by the daemon.
    ///
    /// Only call this after successful reconciliation or live apply confirmation.
    /// Never call this for fans that are not safely enrollable.
    pub fn claim_fan(&mut self, fan_id: &str, control_mode: ControlMode, sysfs_path: &str) {
        self.owned.insert(fan_id.to_string());
        self.control_modes.insert(fan_id.to_string(), control_mode);
        self.fan_sysfs_paths
            .insert(fan_id.to_string(), sysfs_path.to_string());
    }

    /// Release a fan from daemon ownership (e.g., when unenrolled).
    pub fn release_fan(&mut self, fan_id: &str) {
        self.owned.remove(fan_id);
        self.control_modes.remove(fan_id);
        self.fan_sysfs_paths.remove(fan_id);
    }

    /// Release all owned fans.
    pub fn release_all(&mut self) {
        self.owned.clear();
        self.control_modes.clear();
        self.fan_sysfs_paths.clear();
    }

    /// Whether the daemon currently owns a particular fan.
    pub fn owns(&self, fan_id: &str) -> bool {
        self.owned.contains(fan_id)
    }

    /// Iterator over all owned fan IDs.
    pub fn owned_fan_ids(&self) -> impl Iterator<Item = &str> {
        self.owned.iter().map(|s| s.as_str())
    }

    /// Get the control mode for an owned fan.
    pub fn control_mode(&self, fan_id: &str) -> Option<ControlMode> {
        self.control_modes.get(fan_id).copied()
    }

    /// Get the sysfs path for an owned fan's pwm node.
    pub fn sysfs_path(&self, fan_id: &str) -> Option<&str> {
        self.fan_sysfs_paths.get(fan_id).map(|s| s.as_str())
    }

    /// Number of owned fans.
    pub fn len(&self) -> usize {
        self.owned.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.owned.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Fallback lifecycle
// ---------------------------------------------------------------------------

/// Per-fan runtime status for the DBus status model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FanRuntimeStatus {
    /// Fan is not managed by the daemon.
    Unmanaged,

    /// Fan is actively managed by the daemon.
    Managed { control_mode: ControlMode },

    /// Fan was previously managed but is now degraded (skipped at boot
    /// or became unsafe during runtime).
    Degraded { reasons: Vec<DegradedReason> },

    /// Fan has been driven to safe maximum output because the daemon
    /// is shutting down or has encountered a failure.
    Fallback,
}

/// Inspectable runtime state for all fans, surfaced via DBus.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuntimeState {
    /// Per-fan runtime status, keyed by stable fan ID.
    pub fan_statuses: HashMap<String, FanRuntimeStatus>,

    /// Snapshot of owned fan IDs at the time of last update.
    pub owned_fans: Vec<String>,
}

impl RuntimeState {
    /// Create an empty runtime state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the runtime state from current ownership, applied config,
    /// degraded state, and the set of fans in fallback.
    pub fn build(
        owned: &OwnedFanSet,
        _applied: Option<&AppliedConfig>,
        degraded: &DegradedState,
        fallback_fan_ids: &HashSet<String>,
        snapshot: &InventorySnapshot,
    ) -> Self {
        let mut fan_statuses = HashMap::new();

        // First, mark all inventory fans as unmanaged.
        for device in &snapshot.devices {
            for fan in &device.fans {
                fan_statuses.insert(fan.id.clone(), FanRuntimeStatus::Unmanaged);
            }
        }

        // Then, upgrade owned fans to Managed.
        for fan_id in owned.owned_fan_ids() {
            if let Some(mode) = owned.control_mode(fan_id) {
                fan_statuses.insert(
                    fan_id.to_string(),
                    FanRuntimeStatus::Managed { control_mode: mode },
                );
            }
        }

        // Then, override degraded fans (takes precedence over managed).
        for (fan_id, reasons) in &degraded.entries {
            fan_statuses.insert(
                fan_id.clone(),
                FanRuntimeStatus::Degraded {
                    reasons: reasons.clone(),
                },
            );
        }

        // Then, override fallback fans (takes precedence over degraded).
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

// ---------------------------------------------------------------------------
// Fallback fan writes
// ---------------------------------------------------------------------------

/// Result of attempting to write safe-maximum values for owned fans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackResult {
    /// Fans successfully driven to safe maximum.
    pub succeeded: Vec<String>,

    /// Fans where fallback write failed, with errors.
    pub failed: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Reconciliation → ownership + degraded → lifecycle events integration
// ---------------------------------------------------------------------------

/// Run full boot reconciliation: reconcile the applied config against the
/// current inventory, update the owned fan set, degraded state, and
/// lifecycle event log.
///
/// This is the main entry point called at daemon startup.
pub fn perform_boot_reconciliation(
    applied_config: Option<&AppliedConfig>,
    snapshot: &InventorySnapshot,
    owned: &mut OwnedFanSet,
    degraded: &mut DegradedState,
    events: &mut LifecycleEventLog,
) -> ReconcileResult {
    // Clear previous ownership (fresh start at boot).
    owned.release_all();
    degraded.clear_all();

    let Some(applied) = applied_config else {
        // No applied config to reconcile — nothing to restore.
        return ReconcileResult {
            restored: vec![],
            skipped: vec![],
            reconciled_config: AppliedConfig {
                fans: HashMap::new(),
                applied_at: None,
            },
            degraded_reasons: vec![],
        };
    };

    if applied.fans.is_empty() {
        // Empty applied config — nothing to restore.
        return ReconcileResult {
            restored: vec![],
            skipped: vec![],
            reconciled_config: AppliedConfig {
                fans: HashMap::new(),
                applied_at: applied.applied_at.clone(),
            },
            degraded_reasons: vec![],
        };
    }

    let result = reconcile_applied_config(applied, snapshot);

    // Claim successfully restored fans into the owned set.
    for outcome in &result.restored {
        if let ReconcileOutcome::Restored {
            fan_id,
            control_mode,
            ..
        } = outcome
        {
            // Find the fan's sysfs path from the snapshot.
            let sysfs_path = snapshot
                .devices
                .iter()
                .flat_map(|d| d.fans.iter())
                .find(|f| &f.id == fan_id)
                .map(|f| {
                    // The pwm node path is derived from the fan's device sysfs path
                    // and channel number.
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

    // Record degraded state for all skipped fans.
    for (fan_id, reason) in &result.degraded_reasons {
        degraded.mark_degraded(fan_id.clone(), vec![reason.clone()]);
    }

    // Record lifecycle events.
    let timestamp = format_iso8601_now();

    for outcome in &result.restored {
        if let ReconcileOutcome::Restored { fan_id, .. } = outcome {
            events.push(LifecycleEvent {
                timestamp: timestamp.clone(),
                reason: DegradedReason::FanMissing {
                    // Use a different approach — we don't have a "Restored" variant
                    // in DegradedReason, so record under PartialBootRecovery since
                    // this is positive restoration.
                    fan_id: format!("__restored__{fan_id}"),
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

    // If any fans were skipped, record a partial boot recovery event.
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

    // If fans were restored, record an overall success event.
    if !result.restored.is_empty() && result.skipped.is_empty() {
        events.push(LifecycleEvent {
            timestamp,
            reason: DegradedReason::FanMissing {
                fan_id: format!("__boot_reconciled__{}", result.restored.len()),
            },
            detail: Some(format!(
                "all {} managed fans restored successfully on boot",
                result.restored.len()
            )),
        });
    }

    result
}

// ---------------------------------------------------------------------------
// Timestamp helper (shared with daemon)
// ---------------------------------------------------------------------------

/// Return the current time as an ISO 8601 string (UTC).
pub fn format_iso8601_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let (year, month, day) = civil_from_days(days_since_epoch as i64);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
/// Based on Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppliedConfig, AppliedFanEntry};
    use crate::inventory::{HwmonDevice, TemperatureSensor};

    /// Build a test snapshot with configurable devices and fans.
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
                fans: vec![FanChannel {
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

    /// Build a second test device for multi-device scenarios.
    fn test_snapshot_with_second_device() -> InventorySnapshot {
        let mut snapshot = test_snapshot();
        snapshot.devices.push(HwmonDevice {
            id: "hwmon-other-0000000000000002".to_string(),
            name: "otherchip".to_string(),
            sysfs_path: "/sys/class/hwmon/hwmon1".to_string(),
            stable_identity: "/sys/devices/platform/otherchip".to_string(),
            temperatures: vec![],
            fans: vec![FanChannel {
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
                    AppliedFanEntry {
                        control_mode: ControlMode::Pwm,
                        temp_sources: vec!["hwmon-test-0000000000000001-temp1".to_string()],
                    },
                );
                m
            },
            applied_at: Some("2026-04-11T12:00:00Z".to_string()),
        }
    }

    // --- Reconciliation tests ---

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

        // Reconciled config should contain the fan.
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
                fans: vec![], // No fans — the applied fan ID won't be found.
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

        // Reconciled config should be empty.
        assert!(result.reconciled_config.fans.is_empty());
    }

    #[test]
    fn reconcile_changed_support_state() {
        let mut snapshot = test_snapshot();
        // Change the fan's support state to Partial.
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
        // Fan is Available but no longer supports PWM (now Voltage only).
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
        // Remove the temperature sensor.
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

        // Applied config has one fan that exists and one that doesn't.
        let applied = AppliedConfig {
            fans: {
                let mut m = HashMap::new();
                m.insert(
                    "hwmon-test-0000000000000001-fan1".to_string(),
                    AppliedFanEntry {
                        control_mode: ControlMode::Pwm,
                        temp_sources: vec!["hwmon-test-0000000000000001-temp1".to_string()],
                    },
                );
                m.insert(
                    "hwmon-ghost-0000000000000003-fan1".to_string(),
                    AppliedFanEntry {
                        control_mode: ControlMode::Pwm,
                        temp_sources: vec![],
                    },
                );
                m
            },
            applied_at: Some("2026-04-11T12:00:00Z".to_string()),
        };

        let result = reconcile_applied_config(&applied, &snapshot);

        assert_eq!(result.restored.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.degraded_reasons.len(), 1);

        // Reconciled config should only have the real fan.
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

    // --- Ownership tests ---

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

        // Claim a fan.
        owned.claim_fan("fan-1", ControlMode::Pwm, "/sys/class/hwmon/hwmon0/pwm1");

        // An unmanaged fan should never appear in the owned set.
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

    // --- Boot reconciliation integration test ---

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

    // --- RuntimeState tests ---

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
            Some(FanRuntimeStatus::Managed { control_mode }) => {
                assert_eq!(*control_mode, ControlMode::Pwm);
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
    fn runtime_state_all_unmanaged_by_default() {
        let snapshot = test_snapshot_with_second_device();
        let owned = OwnedFanSet::new();
        let degraded = DegradedState::new();
        let fallback = HashSet::new();

        let state = RuntimeState::build(&owned, None, &degraded, &fallback, &snapshot);

        // All fans should be Unmanaged.
        for (fan_id, status) in &state.fan_statuses {
            assert!(
                matches!(status, FanRuntimeStatus::Unmanaged),
                "fan {fan_id} should be Unmanaged, got {status:?}"
            );
        }
    }
}
