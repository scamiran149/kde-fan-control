//! Per-fan PID control loop and temperature sampling.
//!
//! This module contains the `ControlSupervisor` methods that implement
//! the main fan control loop (`run_fan_loop`) and the temperature sampling
//! helpers that feed aggregated sensor data into the PID controller.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};

use kde_fan_control_core::config::{AppliedFanEntry, DegradedReason};
use kde_fan_control_core::control::{
    AggregationFn, PidController, map_output_percent_to_pwm, startup_kick_required,
};
use kde_fan_control_core::lifecycle::ControlRuntimeSnapshot;

use crate::control::sampling::{resolve_fan_rpm_path, resolve_temp_sources, write_pwm_value};
use crate::control::supervisor::ControlSupervisor;

impl ControlSupervisor {
    pub async fn run_fan_loop(
        &self,
        fan_id: String,
        entry: AppliedFanEntry,
        local: Arc<StdMutex<ControlRuntimeSnapshot>>,
        rpm_local: Arc<StdMutex<Option<u64>>>,
    ) {
        let pwm_path = match self.inner.owned.read().await.sysfs_path(&fan_id) {
            Some(path) => path.to_string(),
            None => {
                self.clear_status(&fan_id).await;
                return;
            }
        };

        let rpm_path = {
            let snapshot = self.inner.snapshot.read().await;
            resolve_fan_rpm_path(&snapshot, &fan_id)
        };

        let resolved_temp_sources = {
            let snapshot = self.inner.snapshot.read().await;
            resolve_temp_sources(&snapshot, &entry.temp_sources)
        };

        let mut sample_interval = interval(Duration::from_millis(entry.cadence.sample_interval_ms));
        let mut control_interval =
            interval(Duration::from_millis(entry.cadence.control_interval_ms));
        let mut write_interval = interval(Duration::from_millis(entry.cadence.write_interval_ms));
        sample_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        control_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        write_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut controller = PidController::new(
            entry.target_temp_millidegrees,
            entry.pid_gains,
            entry.pid_limits,
            entry.deadband_millidegrees,
        );
        let mut latest_aggregated_temp = None;
        let mut latest_output_percent = None;
        let mut latest_pwm = None;
        let mut last_written_percent = None;

        loop {
            let cached_auto_tuning = self.auto_tune_output_override(&fan_id).await.is_some();

            tokio::select! {
                _ = sample_interval.tick() => {
                    match Self::sample_temperatures_cached(
                        &fan_id,
                        &resolved_temp_sources,
                        &entry.temp_sources,
                        &entry.aggregation,
                    ) {
                        Ok(aggregated_temp) => {
                            latest_aggregated_temp = Some(aggregated_temp);
                            Self::write_fan_local(&local, |status| {
                                status.aggregated_temp_millidegrees = Some(aggregated_temp);
                                status.alert_high_temp = aggregated_temp >= status.target_temp_millidegrees + 5_000;
                            });
                            self.record_auto_tune_sample(&fan_id, aggregated_temp).await;
                        }
                        Err(reason) => {
                            if cached_auto_tuning {
                                self.fail_auto_tune(&fan_id, reason.to_string()).await;
                                latest_output_percent = None;
                                continue;
                            }
                            self.degrade_and_stop(&fan_id, reason).await;
                            break;
                        }
                    }

                    if let Some(ref path) = rpm_path
                        && let Some(rpm) = fs::read_to_string(path)
                            .ok()
                            .and_then(|v| v.trim().parse::<u64>().ok())
                            && let Ok(mut guard) = rpm_local.lock() {
                                *guard = Some(rpm);
                            }
                }
                _ = control_interval.tick() => {
                    if cached_auto_tuning {
                        latest_output_percent = Some(100.0);
                        Self::write_fan_local(&local, |status| {
                            status.logical_output_percent = Some(100.0);
                            status.auto_tuning = true;
                        });
                        continue;
                    }
                    if let Some(aggregated_temp) = latest_aggregated_temp {
                        let output = controller.update(
                            aggregated_temp,
                            entry.cadence.control_interval_ms as f64 / 1_000.0,
                        );
                        latest_output_percent = Some(output.logical_output_percent);
                        Self::write_fan_local(&local, |status| {
                            status.aggregated_temp_millidegrees = Some(aggregated_temp);
                            status.logical_output_percent = Some(output.logical_output_percent);
                            status.last_error_millidegrees = Some(output.error_millidegrees.round() as i64);
                            status.alert_high_temp = aggregated_temp >= status.target_temp_millidegrees + 5_000;
                        });
                    }
                }
                _ = write_interval.tick() => {
                    if !self.inner.owned.read().await.owns(&fan_id) {
                        self.clear_status(&fan_id).await;
                        break;
                    }
                    if self.inner.degraded.read().await.entries.contains_key(&fan_id) {
                        break;
                    }

                    let Some(output_percent) = cached_auto_tuning.then_some(100.0).or(latest_output_percent) else {
                        continue;
                    };

                    if startup_kick_required(last_written_percent, output_percent) {
                        let kick_percent = entry
                            .actuator_policy
                            .startup_kick_percent
                            .max(output_percent);
                        let kick_pwm = map_output_percent_to_pwm(kick_percent, &entry.actuator_policy);
                        if let Err(error) = write_pwm_value(&pwm_path, kick_pwm) {
                            tracing::error!(fan_id = %fan_id, path = %pwm_path, error = %error, "failed startup-kick pwm write; degrading fan control");
                            self.handle_live_write_failure(&fan_id, &error.to_string()).await;
                            break;
                        }
                        tokio::time::sleep(Duration::from_millis(entry.actuator_policy.startup_kick_ms)).await;
                    }

                    let mapped_pwm = map_output_percent_to_pwm(output_percent, &entry.actuator_policy);
                    match write_pwm_value(&pwm_path, mapped_pwm) {
                        Ok(()) => {
                            latest_pwm = Some(mapped_pwm);
                            last_written_percent = Some(output_percent);
                            Self::write_fan_local(&local, |status| {
                                status.logical_output_percent = Some(output_percent);
                                status.mapped_pwm = Some(mapped_pwm);
                            });
                        }
                        Err(error) => {
                            tracing::error!(fan_id = %fan_id, path = %pwm_path, error = %error, "failed pwm control write; degrading fan control");
                            self.handle_live_write_failure(&fan_id, &error.to_string()).await;
                            break;
                        }
                    }
                }
            }
        }

        let _ = latest_pwm;
    }

    pub async fn sample_temperatures(
        &self,
        fan_id: &str,
        entry: &AppliedFanEntry,
    ) -> Result<i64, DegradedReason> {
        let resolved_sources = {
            let snapshot = self.inner.snapshot.read().await;
            resolve_temp_sources(&snapshot, &entry.temp_sources)
        };
        Self::read_temperature_sources(
            fan_id,
            &resolved_sources,
            &entry.temp_sources,
            &entry.aggregation,
        )
    }

    fn sample_temperatures_cached(
        fan_id: &str,
        resolved_sources: &[(String, PathBuf)],
        temp_sources: &[String],
        aggregation: &AggregationFn,
    ) -> Result<i64, DegradedReason> {
        Self::read_temperature_sources(fan_id, resolved_sources, temp_sources, aggregation)
    }

    fn read_temperature_sources(
        fan_id: &str,
        resolved_sources: &[(String, PathBuf)],
        temp_sources: &[String],
        aggregation: &AggregationFn,
    ) -> Result<i64, DegradedReason> {
        let mut readings = Vec::new();
        let mut first_missing = temp_sources
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown-temp".to_string());

        for (temp_id, path) in resolved_sources {
            first_missing = temp_id.clone();
            match fs::read_to_string(path)
                .ok()
                .and_then(|value| value.trim().parse::<i64>().ok())
            {
                Some(reading) => readings.push(reading),
                None => {
                    tracing::warn!(fan_id = %fan_id, temp_id = %temp_id, "failed to read live temperature input");
                }
            }
        }

        aggregation.compute_millidegrees(&readings).ok_or_else(|| {
            DegradedReason::TempSourceMissing {
                fan_id: fan_id.to_string(),
                temp_id: first_missing,
            }
        })
    }
}
