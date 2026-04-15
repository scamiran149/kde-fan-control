//! Lifecycle event, degraded-state, and lifecycle-log types.
//!
//! Defines `DegradedReason`, `DegradedState`, `LifecycleEvent`,
//! and `LifecycleEventLog`. Events are append-only (capped at
//! `MAX_LIFECYCLE_EVENTS`) and persisted alongside the applied
//! config for post-mortem visibility after daemon restarts.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::inventory::{ControlMode, SupportState};

pub const MAX_LIFECYCLE_EVENTS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DegradedReason {
    BootRestored {
        fan_id: String,
    },
    BootReconciled {
        restored_count: u32,
    },
    FanMissing {
        fan_id: String,
    },
    FanNoLongerEnrollable {
        fan_id: String,
        support_state: SupportState,
        reason: String,
    },
    ControlModeUnavailable {
        fan_id: String,
        mode: ControlMode,
    },
    TempSourceMissing {
        fan_id: String,
        temp_id: String,
    },
    PartialBootRecovery {
        failed_count: u32,
        recovered_count: u32,
    },
    FallbackActive {
        affected_fans: Vec<String>,
    },
    StaleSensorData {
        fan_id: String,
    },
    FanRecovered {
        fan_id: String,
    },
}

impl std::fmt::Display for DegradedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BootRestored { fan_id } => {
                write!(f, "fan '{fan_id}' restored as managed on boot")
            }
            Self::BootReconciled { restored_count } => {
                write!(
                    f,
                    "boot reconciliation restored {restored_count} managed fan(s)"
                )
            }
            Self::FanMissing { fan_id } => {
                write!(f, "fan '{fan_id}' missing from hardware")
            }
            Self::FanNoLongerEnrollable {
                fan_id,
                support_state,
                reason,
            } => {
                write!(
                    f,
                    "fan '{fan_id}' no longer enrollable ({support_state:?}): {reason}"
                )
            }
            Self::ControlModeUnavailable { fan_id, mode } => {
                write!(f, "fan '{fan_id}' no longer supports control mode {mode:?}")
            }
            Self::TempSourceMissing { fan_id, temp_id } => {
                write!(
                    f,
                    "temperature source '{temp_id}' for fan '{fan_id}' missing"
                )
            }
            Self::PartialBootRecovery {
                failed_count,
                recovered_count,
            } => {
                write!(
                    f,
                    "partial boot recovery: {recovered_count} recovered, {failed_count} failed"
                )
            }
            Self::FallbackActive { affected_fans } => {
                write!(f, "fallback active for fans: {}", affected_fans.join(", "))
            }
            Self::StaleSensorData { fan_id } => {
                write!(
                    f,
                    "fan '{fan_id}' produced no valid sensor data for an extended period"
                )
            }
            Self::FanRecovered { fan_id } => {
                write!(f, "fan '{fan_id}' recovered from degraded state")
            }
        }
    }
}

impl DegradedReason {
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::TempSourceMissing { .. }
            | Self::StaleSensorData { .. }
            | Self::ControlModeUnavailable { .. } => true,
            Self::FanMissing { .. }
            | Self::FanNoLongerEnrollable { .. }
            | Self::BootRestored { .. }
            | Self::BootReconciled { .. }
            | Self::PartialBootRecovery { .. }
            | Self::FallbackActive { .. }
            | Self::FanRecovered { .. } => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub timestamp: String,
    pub reason: DegradedReason,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LifecycleEventLog {
    events: Vec<LifecycleEvent>,
}

impl LifecycleEventLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, event: LifecycleEvent) {
        if self.events.len() >= MAX_LIFECYCLE_EVENTS {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    pub fn events(&self) -> &[LifecycleEvent] {
        &self.events
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DegradedState {
    #[serde(default)]
    pub entries: HashMap<String, Vec<DegradedReason>>,
}

impl DegradedState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark_degraded(&mut self, fan_id: String, reasons: Vec<DegradedReason>) {
        self.entries.insert(fan_id, reasons);
    }

    pub fn clear_fan(&mut self, fan_id: &str) {
        self.entries.remove(fan_id);
    }

    pub fn clear_all(&mut self) {
        self.entries.clear();
    }

    pub fn has_degraded(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn degraded_fan_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    pub fn is_fan_recoverable(&self, fan_id: &str) -> bool {
        self.entries
            .get(fan_id)
            .map(|reasons| reasons.iter().any(|r| r.is_recoverable()))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ControlMode;

    #[test]
    fn lifecycle_event_log_bounds() {
        let mut log = LifecycleEventLog::new();
        for i in 0..MAX_LIFECYCLE_EVENTS + 10 {
            log.push(LifecycleEvent {
                timestamp: format!("2026-04-11T12:{i:02}:00Z"),
                reason: DegradedReason::FanMissing {
                    fan_id: format!("fan-{i}"),
                },
                detail: None,
            });
        }
        assert_eq!(log.len(), MAX_LIFECYCLE_EVENTS);
        let first = &log.events()[0];
        assert!(first.timestamp.contains("10"));
    }

    #[test]
    fn degraded_reason_display() {
        let reason = DegradedReason::FanMissing {
            fan_id: "fan-1".to_string(),
        };
        assert!(format!("{reason}").contains("fan-1"));

        let reason = DegradedReason::PartialBootRecovery {
            failed_count: 2,
            recovered_count: 3,
        };
        let text = format!("{reason}");
        assert!(text.contains("3 recovered"));
        assert!(text.contains("2 failed"));
    }

    #[test]
    fn is_recoverable_classifies_degraded_reasons() {
        assert!(
            DegradedReason::TempSourceMissing {
                fan_id: "f1".into(),
                temp_id: "t1".into(),
            }
            .is_recoverable()
        );
        assert!(
            DegradedReason::StaleSensorData {
                fan_id: "f1".into(),
            }
            .is_recoverable()
        );
        assert!(
            DegradedReason::ControlModeUnavailable {
                fan_id: "f1".into(),
                mode: ControlMode::Pwm,
            }
            .is_recoverable()
        );
        assert!(
            !DegradedReason::FanMissing {
                fan_id: "f1".into(),
            }
            .is_recoverable()
        );
        assert!(
            !DegradedReason::FanNoLongerEnrollable {
                fan_id: "f1".into(),
                support_state: SupportState::Unavailable,
                reason: "test".into(),
            }
            .is_recoverable()
        );
        assert!(
            !DegradedReason::FanRecovered {
                fan_id: "f1".into(),
            }
            .is_recoverable()
        );
    }

    #[test]
    fn degraded_state_is_fan_recoverable() {
        let mut state = DegradedState::new();
        state.mark_degraded(
            "f1".into(),
            vec![
                DegradedReason::TempSourceMissing {
                    fan_id: "f1".into(),
                    temp_id: "t1".into(),
                },
                DegradedReason::FanMissing {
                    fan_id: "f1".into(),
                },
            ],
        );
        assert!(state.is_fan_recoverable("f1"));

        let mut state_no_recovery = DegradedState::new();
        state_no_recovery.mark_degraded(
            "f1".into(),
            vec![DegradedReason::FanMissing {
                fan_id: "f1".into(),
            }],
        );
        assert!(!state_no_recovery.is_fan_recoverable("f1"));
    }
}
