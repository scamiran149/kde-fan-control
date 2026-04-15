//! Control-loop helper functions.
//!
//! Pure functions for converting applied config entries to draft form,
//! building runtime snapshots from applied config, and deriving
//! auto-tune proposals from sample data. These have no dependency on
//! the running daemon state.

use kde_fan_control_core::config::{AppliedFanEntry, DraftFanEntry};
use kde_fan_control_core::control::AutoTuneProposal;
use kde_fan_control_core::lifecycle::ControlRuntimeSnapshot;

use crate::state::AutoTuneSample;

/// Build an initial [`ControlRuntimeSnapshot`] from an applied fan entry.
///
/// Used when starting a new control loop or recovering a degraded fan.
pub fn control_snapshot_from_applied(entry: &AppliedFanEntry) -> ControlRuntimeSnapshot {
    ControlRuntimeSnapshot {
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

/// Derive a draft entry from an applied entry, preserving all current settings.
///
/// Used when a client accepts auto-tune or edits a control profile:
/// the draft starts as a copy of the applied values so the user can
/// selectively override fields.
pub fn draft_entry_from_applied(entry: &AppliedFanEntry) -> DraftFanEntry {
    DraftFanEntry {
        managed: true,
        control_mode: Some(entry.control_mode),
        temp_sources: entry.temp_sources.clone(),
        target_temp_millidegrees: Some(entry.target_temp_millidegrees),
        aggregation: Some(entry.aggregation),
        pid_gains: Some(entry.pid_gains),
        cadence: Some(entry.cadence),
        deadband_millidegrees: Some(entry.deadband_millidegrees),
        actuator_policy: Some(entry.actuator_policy),
        pid_limits: Some(entry.pid_limits),
    }
}

/// Derive an auto-tune proposal from observed temperature samples.
///
/// Computes lag time and maximum cooling rate from the sample window,
/// then delegates to [`AutoTuneProposal::from_step_response`].
pub fn proposal_from_auto_tune_samples(
    observation_window_ms: u64,
    samples: &[AutoTuneSample],
) -> Result<AutoTuneProposal, String> {
    if samples.len() < 2 {
        return Err("auto-tune needs at least two temperature samples".to_string());
    }

    let baseline = samples[0].aggregated_temp_millidegrees;
    let lag_time_ms = samples
        .iter()
        .skip(1)
        .find(|sample| (sample.aggregated_temp_millidegrees - baseline).abs() >= 500)
        .map(|sample| sample.elapsed_ms.max(1))
        .unwrap_or_else(|| (observation_window_ms / 4).max(1));

    let mut max_rate_c_per_sec: f64 = 0.0;
    for window in samples.windows(2) {
        let earlier = &window[0];
        let later = &window[1];
        let dt_ms = later.elapsed_ms.saturating_sub(earlier.elapsed_ms);
        if dt_ms == 0 {
            continue;
        }

        let delta_c = (earlier.aggregated_temp_millidegrees - later.aggregated_temp_millidegrees)
            .abs() as f64
            / 1_000.0;
        let rate = delta_c / (dt_ms as f64 / 1_000.0);
        max_rate_c_per_sec = max_rate_c_per_sec.max(rate);
    }

    AutoTuneProposal::from_step_response(observation_window_ms, lag_time_ms, max_rate_c_per_sec)
        .ok_or_else(|| {
            "auto-tune could not derive a bounded proposal from sampled temperatures".to_string()
        })
}
