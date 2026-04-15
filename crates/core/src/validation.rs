//! Draft configuration validation and apply-draft transaction.
//!
//! Validates a `DraftConfig` against live hardware inventory before
//! promotion to `AppliedConfig`. Each managed fan entry is checked for
//! fan existence, enrollability, control-mode support, temperature
//! source presence, cadence bounds, actuator policy, PID gains
//! finiteness, and target temperature range.
//!
//! `apply_draft` performs validation then builds an `AppliedConfig`
//! from the enrollable subset, preserving any previously applied
//! fans that are absent from the draft.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::config::{AppliedConfig, AppliedFanEntry, DraftConfig};
use crate::control::{ActuatorPolicy, ControlCadence, PidLimits};
use crate::inventory::{ControlMode, FanChannel, InventorySnapshot, SupportState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidationError {
    FanNotFound {
        fan_id: String,
    },

    FanNotEnrollable {
        fan_id: String,
        support_state: SupportState,
        reason: String,
    },

    UnsupportedControlMode {
        fan_id: String,
        requested: ControlMode,
        available: Vec<ControlMode>,
    },

    MissingControlMode {
        fan_id: String,
    },

    TempSourceNotFound {
        fan_id: String,
        temp_id: String,
    },

    MissingTargetTemp {
        fan_id: String,
    },

    NoSensorForManagedFan {
        fan_id: String,
    },

    InvalidCadence {
        fan_id: String,
        reason: String,
    },

    InvalidActuatorPolicy {
        fan_id: String,
        reason: String,
    },

    InvalidPidLimits {
        fan_id: String,
        reason: String,
    },

    InvalidPidGains {
        fan_id: String,
        detail: String,
    },

    InvalidTargetTemperature {
        fan_id: String,
        value: i64,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FanNotFound { fan_id } => {
                write!(f, "fan '{fan_id}' not found in current inventory")
            }
            Self::FanNotEnrollable {
                fan_id,
                support_state,
                reason,
            } => {
                write!(
                    f,
                    "fan '{fan_id}' is not enrollable (support: {support_state:?}): {reason}"
                )
            }
            Self::UnsupportedControlMode {
                fan_id,
                requested,
                available,
            } => {
                let available_str: Vec<String> =
                    available.iter().map(|m| format!("{m:?}")).collect();
                write!(
                    f,
                    "fan '{fan_id}' does not support control mode {requested:?} (available: {})",
                    available_str.join(", ")
                )
            }
            Self::MissingControlMode { fan_id } => {
                write!(f, "managed fan '{fan_id}' has no control mode selected")
            }
            Self::TempSourceNotFound { fan_id, temp_id } => {
                write!(
                    f,
                    "temperature source '{temp_id}' for fan '{fan_id}' not found in current inventory"
                )
            }
            Self::MissingTargetTemp { fan_id } => {
                write!(
                    f,
                    "managed fan '{fan_id}' has no target temperature configured"
                )
            }
            Self::NoSensorForManagedFan { fan_id } => {
                write!(
                    f,
                    "managed fan '{fan_id}' has no temperature source configured"
                )
            }
            Self::InvalidCadence { fan_id, reason } => {
                write!(f, "managed fan '{fan_id}' has invalid cadence: {reason}")
            }
            Self::InvalidActuatorPolicy { fan_id, reason } => {
                write!(
                    f,
                    "managed fan '{fan_id}' has invalid actuator policy: {reason}"
                )
            }
            Self::InvalidPidLimits { fan_id, reason } => {
                write!(f, "managed fan '{fan_id}' has invalid PID limits: {reason}")
            }
            Self::InvalidPidGains { fan_id, detail } => {
                write!(f, "managed fan '{fan_id}' has invalid PID gains: {detail}")
            }
            Self::InvalidTargetTemperature { fan_id, value } => {
                write!(
                    f,
                    "managed fan '{fan_id}' has invalid target temperature: {value} m°C (must be 1..=150000)"
                )
            }
        }
    }
}

impl std::error::Error for ValidationError {}

fn validate_cadence(fan_id: &str, cadence: ControlCadence) -> Result<(), ValidationError> {
    if cadence.sample_interval_ms < 100
        || cadence.control_interval_ms < 100
        || cadence.write_interval_ms < 100
    {
        return Err(ValidationError::InvalidCadence {
            fan_id: fan_id.to_string(),
            reason: "sample, control, and write cadences must each be at least 100ms".to_string(),
        });
    }

    if cadence.sample_interval_ms > cadence.control_interval_ms
        || cadence.control_interval_ms > cadence.write_interval_ms
    {
        return Err(ValidationError::InvalidCadence {
            fan_id: fan_id.to_string(),
            reason: "cadence must satisfy sample <= control <= write".to_string(),
        });
    }

    Ok(())
}

fn validate_actuator_policy(fan_id: &str, policy: ActuatorPolicy) -> Result<(), ValidationError> {
    let percent_ok = |value: f64| (0.0..=100.0).contains(&value);

    if !percent_ok(policy.output_min_percent)
        || !percent_ok(policy.output_max_percent)
        || !percent_ok(policy.startup_kick_percent)
    {
        return Err(ValidationError::InvalidActuatorPolicy {
            fan_id: fan_id.to_string(),
            reason: "all actuator percentages must be within 0.0..=100.0".to_string(),
        });
    }

    if policy.output_min_percent > policy.output_max_percent {
        return Err(ValidationError::InvalidActuatorPolicy {
            fan_id: fan_id.to_string(),
            reason: "output_min_percent must be <= output_max_percent".to_string(),
        });
    }

    if policy.pwm_min > policy.pwm_max {
        return Err(ValidationError::InvalidActuatorPolicy {
            fan_id: fan_id.to_string(),
            reason: "pwm_min must be <= pwm_max".to_string(),
        });
    }

    Ok(())
}

fn validate_pid_limits(fan_id: &str, limits: PidLimits) -> Result<(), ValidationError> {
    if !limits.is_finite() {
        return Err(ValidationError::InvalidPidLimits {
            fan_id: fan_id.to_string(),
            reason: "PID limits contain non-finite values (NaN or Infinity)".to_string(),
        });
    }

    if limits.integral_min > limits.integral_max {
        return Err(ValidationError::InvalidPidLimits {
            fan_id: fan_id.to_string(),
            reason: "integral_min must be <= integral_max".to_string(),
        });
    }

    if limits.derivative_min > limits.derivative_max {
        return Err(ValidationError::InvalidPidLimits {
            fan_id: fan_id.to_string(),
            reason: "derivative_min must be <= derivative_max".to_string(),
        });
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub enrollable: Vec<String>,

    pub rejected: Vec<(String, ValidationError)>,
}

impl ValidationResult {
    pub fn all_passed(&self) -> bool {
        self.rejected.is_empty()
    }
}

pub fn find_fan_by_id<'a>(snapshot: &'a InventorySnapshot, fan_id: &str) -> Option<&'a FanChannel> {
    snapshot
        .devices
        .iter()
        .flat_map(|d| d.fans.iter())
        .find(|f| f.id == fan_id)
}

pub fn temp_source_exists(snapshot: &InventorySnapshot, temp_id: &str) -> bool {
    snapshot
        .devices
        .iter()
        .flat_map(|d| d.temperatures.iter())
        .any(|t| t.id == temp_id)
}

pub fn validate_draft(draft: &DraftConfig, snapshot: &InventorySnapshot) -> ValidationResult {
    let mut enrollable = Vec::new();
    let mut rejected = Vec::new();

    for (fan_id, entry) in &draft.fans {
        if !entry.managed {
            continue;
        }

        let Some(fan) = find_fan_by_id(snapshot, fan_id) else {
            rejected.push((
                fan_id.clone(),
                ValidationError::FanNotFound {
                    fan_id: fan_id.clone(),
                },
            ));
            continue;
        };

        if fan.support_state != SupportState::Available {
            rejected.push((
                fan_id.clone(),
                ValidationError::FanNotEnrollable {
                    fan_id: fan_id.clone(),
                    support_state: fan.support_state,
                    reason: fan
                        .support_reason
                        .clone()
                        .unwrap_or_else(|| "unsupported hardware".to_string()),
                },
            ));
            continue;
        }

        let Some(ref requested_mode) = entry.control_mode else {
            rejected.push((
                fan_id.clone(),
                ValidationError::MissingControlMode {
                    fan_id: fan_id.clone(),
                },
            ));
            continue;
        };

        if !fan.control_modes.contains(requested_mode) {
            rejected.push((
                fan_id.clone(),
                ValidationError::UnsupportedControlMode {
                    fan_id: fan_id.clone(),
                    requested: *requested_mode,
                    available: fan.control_modes.clone(),
                },
            ));
            continue;
        }

        if entry.resolved_target_temp_millidegrees().is_none() {
            rejected.push((
                fan_id.clone(),
                ValidationError::MissingTargetTemp {
                    fan_id: fan_id.clone(),
                },
            ));
            continue;
        }

        if entry.temp_sources.is_empty() {
            rejected.push((
                fan_id.clone(),
                ValidationError::NoSensorForManagedFan {
                    fan_id: fan_id.clone(),
                },
            ));
            continue;
        }

        if let Err(error) = validate_cadence(fan_id, entry.resolved_cadence()) {
            rejected.push((fan_id.clone(), error));
            continue;
        }

        if let Err(error) = validate_actuator_policy(fan_id, entry.resolved_actuator_policy()) {
            rejected.push((fan_id.clone(), error));
            continue;
        }

        if let Err(error) = validate_pid_limits(fan_id, entry.resolved_pid_limits()) {
            rejected.push((fan_id.clone(), error));
            continue;
        }

        let gains = entry.resolved_pid_gains();
        if !gains.is_finite() {
            let mut details = Vec::new();
            if !gains.kp.is_finite() {
                details.push(format!("kp={}", gains.kp));
            }
            if !gains.ki.is_finite() {
                details.push(format!("ki={}", gains.ki));
            }
            if !gains.kd.is_finite() {
                details.push(format!("kd={}", gains.kd));
            }
            rejected.push((
                fan_id.clone(),
                ValidationError::InvalidPidGains {
                    fan_id: fan_id.clone(),
                    detail: format!("non-finite PID gains: {}", details.join(", ")),
                },
            ));
            continue;
        }

        if let Some(target) = entry.resolved_target_temp_millidegrees()
            && (target <= 0 || target > 150_000)
        {
            rejected.push((
                fan_id.clone(),
                ValidationError::InvalidTargetTemperature {
                    fan_id: fan_id.clone(),
                    value: target,
                },
            ));
            continue;
        }

        let mut temp_ok = true;
        for temp_id in &entry.temp_sources {
            if !temp_source_exists(snapshot, temp_id) {
                rejected.push((
                    fan_id.clone(),
                    ValidationError::TempSourceNotFound {
                        fan_id: fan_id.clone(),
                        temp_id: temp_id.clone(),
                    },
                ));
                temp_ok = false;
                break;
            }
        }
        if !temp_ok {
            continue;
        }

        enrollable.push(fan_id.clone());
    }

    ValidationResult {
        enrollable,
        rejected,
    }
}

pub fn apply_draft(
    draft: &DraftConfig,
    snapshot: &InventorySnapshot,
    applied_at: String,
    previous_applied: Option<&AppliedConfig>,
) -> (AppliedConfig, ValidationResult) {
    let result = validate_draft(draft, snapshot);

    let mut fans = result
        .enrollable
        .iter()
        .filter_map(|fan_id| {
            let entry = draft.fans.get(fan_id)?;
            let control_mode = entry.control_mode?;
            Some((
                fan_id.clone(),
                AppliedFanEntry {
                    control_mode,
                    temp_sources: entry.temp_sources.clone(),
                    target_temp_millidegrees: entry.resolved_target_temp_millidegrees()?,
                    aggregation: entry.resolved_aggregation(),
                    pid_gains: entry.resolved_pid_gains(),
                    cadence: entry.resolved_cadence(),
                    deadband_millidegrees: entry.resolved_deadband_millidegrees(),
                    actuator_policy: entry.resolved_actuator_policy(),
                    pid_limits: entry.resolved_pid_limits(),
                },
            ))
        })
        .collect::<HashMap<_, _>>();

    if let Some(prev) = previous_applied {
        for (fan_id, entry) in &prev.fans {
            if !fans.contains_key(fan_id) && !draft.fans.contains_key(fan_id) {
                fans.insert(fan_id.clone(), entry.clone());
            }
        }
    }

    let applied = AppliedConfig {
        fans,
        applied_at: Some(applied_at),
    };

    (applied, result)
}

#[cfg(test)]
mod tests {
    use crate::config::{
        AppliedConfig, AppliedFanEntry, DraftConfig, DraftFanEntry,
    };
    use crate::control::{
        ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits,
    };
    use crate::inventory::{
        ControlMode, FanChannel, HwmonDevice, InventorySnapshot, SupportState, TemperatureSensor,
    };
    use super::apply_draft;

    fn test_applied_entry() -> AppliedFanEntry {
        AppliedFanEntry {
            control_mode: ControlMode::Pwm,
            temp_sources: vec!["hwmon-test-0000000000000001-temp1".to_string()],
            target_temp_millidegrees: 50_000,
            aggregation: AggregationFn::Average,
            pid_gains: PidGains {
                kp: 1.0,
                ki: 0.0,
                kd: 0.0,
            },
            cadence: ControlCadence {
                sample_interval_ms: 500,
                control_interval_ms: 1000,
                write_interval_ms: 2000,
            },
            deadband_millidegrees: 1000,
            actuator_policy: ActuatorPolicy {
                output_min_percent: 0.0,
                output_max_percent: 100.0,
                pwm_min: 0,
                pwm_max: 255,
                startup_kick_percent: 35.0,
                startup_kick_ms: 1000,
            },
            pid_limits: PidLimits::default(),
        }
    }

    fn multi_fan_snapshot() -> InventorySnapshot {
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
                    input_millidegrees_celsius: Some(45_000),
                }],
                fans: vec![
                    FanChannel {
                        id: "hwmon-test-0000000000000001-fan1".to_string(),
                        channel: 1,
                        label: Some("CPU Fan".to_string()),
                        friendly_name: None,
                        rpm_feedback: true,
                        current_rpm: Some(1200),
                        control_modes: vec![ControlMode::Pwm],
                        support_state: SupportState::Available,
                        support_reason: None,
                    },
                    FanChannel {
                        id: "hwmon-test-0000000000000001-fan2".to_string(),
                        channel: 2,
                        label: Some("Case Fan".to_string()),
                        friendly_name: None,
                        rpm_feedback: true,
                        current_rpm: Some(800),
                        control_modes: vec![ControlMode::Pwm],
                        support_state: SupportState::Available,
                        support_reason: None,
                    },
                    FanChannel {
                        id: "hwmon-test-0000000000000001-fan3".to_string(),
                        channel: 3,
                        label: Some("GPU Fan".to_string()),
                        friendly_name: None,
                        rpm_feedback: true,
                        current_rpm: Some(900),
                        control_modes: vec![ControlMode::Pwm],
                        support_state: SupportState::Available,
                        support_reason: None,
                    },
                ],
            }],
        }
    }

    fn managed_draft(_fan_id: &str) -> DraftFanEntry {
        DraftFanEntry {
            managed: true,
            control_mode: Some(ControlMode::Pwm),
            temp_sources: vec!["hwmon-test-0000000000000001-temp1".to_string()],
            target_temp_millidegrees: Some(60_000),
            aggregation: None,
            pid_gains: None,
            cadence: None,
            deadband_millidegrees: None,
            actuator_policy: None,
            pid_limits: None,
        }
    }

    #[test]
    fn apply_draft_preserves_previous_applied_fans_absent_from_draft() {
        let snapshot = multi_fan_snapshot();

        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            managed_draft("hwmon-test-0000000000000001-fan1"),
        );

        let previous = AppliedConfig {
            fans: [
                ("hwmon-test-0000000000000001-fan1".to_string(), test_applied_entry()),
                ("hwmon-test-0000000000000001-fan2".to_string(), test_applied_entry()),
                ("hwmon-test-0000000000000001-fan3".to_string(), test_applied_entry()),
            ]
            .into(),
            applied_at: Some("2026-04-10T12:00:00Z".to_string()),
        };

        let (applied, result) = apply_draft(
            &draft,
            &snapshot,
            "2026-04-15T12:00:00Z".to_string(),
            Some(&previous),
        );

        assert!(result.all_passed(), "draft validation should pass");
        assert_eq!(
            applied.fans.len(),
            3,
            "applied config should contain all 3 fans (1 from draft + 2 preserved)"
        );
        assert!(
            applied.fans.contains_key("hwmon-test-0000000000000001-fan1"),
            "fan1 from draft should be in applied"
        );
        assert!(
            applied.fans.contains_key("hwmon-test-0000000000000001-fan2"),
            "fan2 preserved from previous should be in applied"
        );
        assert!(
            applied.fans.contains_key("hwmon-test-0000000000000001-fan3"),
            "fan3 preserved from previous should be in applied"
        );

        let draft_entry = applied
            .fans
            .get("hwmon-test-0000000000000001-fan1")
            .unwrap();
        assert_eq!(
            draft_entry.target_temp_millidegrees, 60_000,
            "fan1 should use draft target temp"
        );

        let preserved_entry = applied
            .fans
            .get("hwmon-test-0000000000000001-fan2")
            .unwrap();
        assert_eq!(
            preserved_entry.target_temp_millidegrees, 50_000,
            "fan2 should use previous applied target temp"
        );
    }

    #[test]
    fn apply_draft_enrollable_only_lists_draft_fans_not_preserved() {
        let snapshot = multi_fan_snapshot();

        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            managed_draft("hwmon-test-0000000000000001-fan1"),
        );

        let previous = AppliedConfig {
            fans: [
                ("hwmon-test-0000000000000001-fan2".to_string(), test_applied_entry()),
                ("hwmon-test-0000000000000001-fan3".to_string(), test_applied_entry()),
            ]
            .into(),
            applied_at: Some("2026-04-10T12:00:00Z".to_string()),
        };

        let (_, result) = apply_draft(
            &draft,
            &snapshot,
            "2026-04-15T12:00:00Z".to_string(),
            Some(&previous),
        );

        assert!(result.all_passed());
        assert_eq!(
            result.enrollable.len(),
            1,
            "enrollable should only contain draft-validated fans, not preserved ones"
        );
        assert_eq!(
            result.enrollable[0], "hwmon-test-0000000000000001-fan1",
            "enrollable should contain the draft fan"
        );
    }

    #[test]
    fn apply_draft_does_not_preserve_fans_present_in_draft_as_unmanaged() {
        let snapshot = multi_fan_snapshot();

        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            managed_draft("hwmon-test-0000000000000001-fan1"),
        );
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan2".to_string(),
            DraftFanEntry {
                managed: false,
                ..Default::default()
            },
        );

        let previous = AppliedConfig {
            fans: [
                ("hwmon-test-0000000000000001-fan1".to_string(), test_applied_entry()),
                ("hwmon-test-0000000000000001-fan2".to_string(), test_applied_entry()),
            ]
            .into(),
            applied_at: Some("2026-04-10T12:00:00Z".to_string()),
        };

        let (applied, _) = apply_draft(
            &draft,
            &snapshot,
            "2026-04-15T12:00:00Z".to_string(),
            Some(&previous),
        );

        assert!(
            applied.fans.contains_key("hwmon-test-0000000000000001-fan1"),
            "managed draft fan should be in applied"
        );
        assert!(
            !applied.fans.contains_key("hwmon-test-0000000000000001-fan2"),
            "fan explicitly unmanaged in draft should NOT be preserved"
        );
    }
}
