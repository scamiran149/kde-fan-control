//! Shared daemon state types.
//!
//! Data structures used across multiple daemon subsystems:
//! auto-tune execution tracking, control result views,
//! draft profile patch payloads, and tuning settings.

use serde::{Deserialize, Serialize};
use std::time::Instant;

use kde_fan_control_core::control::{
    ActuatorPolicy, AggregationFn, AutoTuneProposal, ControlCadence, PidGains, PidLimits,
};

/// A single temperature observation recorded during an auto-tune run.
#[derive(Debug, Clone)]
pub struct AutoTuneSample {
    pub elapsed_ms: u64,
    pub aggregated_temp_millidegrees: i64,
}

/// Tracks the live execution state of an auto-tune cycle for a single fan.
#[derive(Debug, Clone)]
pub enum AutoTuneExecutionState {
    /// Auto-tune is actively collecting samples.
    Running {
        started_at: Instant,
        observation_window_ms: u64,
        samples: Vec<AutoTuneSample>,
    },
    /// Auto-tune finished and produced a proposal.
    Completed {
        observation_window_ms: u64,
        proposal: AutoTuneProposal,
    },
    /// Auto-tune failed with an error.
    Failed {
        observation_window_ms: u64,
        error: String,
    },
}

/// A serializable view of the auto-tune state, sent over DBus to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AutoTuneResultView {
    /// No auto-tune in progress for this fan.
    Idle { observation_window_ms: u64 },
    /// Auto-tune is actively running.
    Running { observation_window_ms: u64 },
    /// Auto-tune completed successfully.
    Completed {
        observation_window_ms: u64,
        proposal: AutoTuneProposal,
    },
    /// Auto-tune failed.
    Failed {
        observation_window_ms: u64,
        error: String,
    },
}

/// A partial patch payload for updating a fan's control profile in the draft config.
///
/// All fields are optional to allow partial updates. Inner `Option<Option<...>>` fields
/// distinguish between "not provided" and "explicitly set to null".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DraftFanControlProfilePayload {
    #[serde(default)]
    pub temp_sources: Option<Vec<String>>,
    #[serde(default)]
    pub target_temp_millidegrees: Option<Option<i64>>,
    #[serde(default)]
    pub aggregation: Option<Option<AggregationFn>>,
    #[serde(default)]
    pub pid_gains: Option<Option<PidGains>>,
    #[serde(default)]
    pub cadence: Option<Option<ControlCadence>>,
    #[serde(default)]
    pub deadband_millidegrees: Option<Option<i64>>,
    #[serde(default)]
    pub actuator_policy: Option<Option<ActuatorPolicy>>,
    #[serde(default)]
    pub pid_limits: Option<Option<PidLimits>>,
}

/// Tuning knobs for the control daemon that can be adjusted at runtime.
#[derive(Debug, Clone, Copy)]
pub struct DaemonTuningSettings {
    pub auto_tune_observation_window_ms: u64,
}

impl Default for DaemonTuningSettings {
    fn default() -> Self {
        Self {
            auto_tune_observation_window_ms: 30_000,
        }
    }
}
