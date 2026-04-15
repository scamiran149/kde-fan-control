//! Conversion helpers shared across DBus interface implementations.
//!
//! This module contains pure functions that translate between core-crate types
//! and DBus-facing representations. They are used by the `inventory`,
//! `lifecycle`, and `control` interface modules.

use kde_fan_control_core::config::{DegradedReason, ValidationError};
use kde_fan_control_core::inventory::{ControlMode, SupportState};
use zbus::fdo;

/// Parse a control mode string into a `ControlMode` enum value.
///
/// Returns `Ok(None)` for empty or `"none"`, `Ok(Some(mode))` for recognised
/// modes, and an `fdo::Error::Failed` for unknown strings.
pub fn parse_control_mode(mode: &str) -> fdo::Result<Option<ControlMode>> {
    match mode {
        "" | "none" => Ok(None),
        "pwm" => Ok(Some(ControlMode::Pwm)),
        "voltage" => Ok(Some(ControlMode::Voltage)),
        _ => Err(fdo::Error::Failed(format!(
            "unknown control mode '{mode}'; expected 'pwm', 'voltage', or empty"
        ))),
    }
}

/// Map a `ValidationError` to a `DegradedReason` for degraded-state tracking.
pub fn validation_error_to_degraded_reason(error: &ValidationError) -> DegradedReason {
    match error {
        ValidationError::FanNotFound { fan_id } => DegradedReason::FanMissing {
            fan_id: fan_id.clone(),
        },
        ValidationError::FanNotEnrollable {
            fan_id,
            support_state,
            reason,
        } => DegradedReason::FanNoLongerEnrollable {
            fan_id: fan_id.clone(),
            support_state: *support_state,
            reason: reason.clone(),
        },
        ValidationError::UnsupportedControlMode {
            fan_id, requested, ..
        } => DegradedReason::ControlModeUnavailable {
            fan_id: fan_id.clone(),
            mode: *requested,
        },
        ValidationError::MissingControlMode { fan_id } => DegradedReason::FanNoLongerEnrollable {
            fan_id: fan_id.clone(),
            support_state: SupportState::Unavailable,
            reason: "no control mode selected".into(),
        },
        ValidationError::TempSourceNotFound { fan_id, temp_id } => {
            DegradedReason::TempSourceMissing {
                fan_id: fan_id.clone(),
                temp_id: temp_id.clone(),
            }
        }
        ValidationError::MissingTargetTemp { fan_id }
        | ValidationError::NoSensorForManagedFan { fan_id }
        | ValidationError::InvalidCadence { fan_id, .. }
        | ValidationError::InvalidActuatorPolicy { fan_id, .. }
        | ValidationError::InvalidPidLimits { fan_id, .. }
        | ValidationError::InvalidPidGains { fan_id, .. }
        | ValidationError::InvalidTargetTemperature { fan_id, .. } => {
            DegradedReason::FanNoLongerEnrollable {
                fan_id: fan_id.clone(),
                support_state: SupportState::Unavailable,
                reason: error.to_string(),
            }
        }
    }
}
