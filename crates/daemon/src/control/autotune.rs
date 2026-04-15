//! Auto-tune state machine for per-fan PID parameter discovery.
//!
//! This module contains the `ControlSupervisor` methods that manage the
//! auto-tune lifecycle: starting a run, recording temperature samples,
//! transitioning between states (Running → Completed / Failed), and
//! extracting the resulting proposal for client acceptance.

use zbus::fdo;

use kde_fan_control_core::control::AutoTuneProposal;

use crate::control::helpers::proposal_from_auto_tune_samples;
use crate::control::supervisor::ControlSupervisor;
use crate::dbus::signals::emit_auto_tune_completed;
use crate::state::{AutoTuneExecutionState, AutoTuneResultView, AutoTuneSample};

impl ControlSupervisor {
    #[allow(dead_code)]
    pub async fn set_auto_tune_observation_window_ms(&self, observation_window_ms: u64) {
        self.inner
            .tuning
            .write()
            .await
            .auto_tune_observation_window_ms = observation_window_ms;
    }

    pub async fn start_auto_tune(&self, fan_id: &str) -> fdo::Result<()> {
        let is_managed = {
            let config = self.inner.config.read().await;
            config
                .applied
                .as_ref()
                .and_then(|applied| applied.fans.get(fan_id))
                .is_some()
        };
        if !is_managed {
            return Err(fdo::Error::Failed(format!(
                "fan '{fan_id}' is not managed by the applied config"
            )));
        }

        if !self.inner.owned.read().await.owns(fan_id) {
            return Err(fdo::Error::Failed(format!(
                "fan '{fan_id}' is not currently owned by the daemon"
            )));
        }

        if !self.inner.status.read().await.contains_key(fan_id) {
            return Err(fdo::Error::Failed(format!(
                "fan '{fan_id}' is missing live control state"
            )));
        }

        {
            let auto_tune = self.inner.auto_tune.read().await;
            if matches!(
                auto_tune.get(fan_id),
                Some(AutoTuneExecutionState::Running { .. })
            ) {
                return Err(fdo::Error::Failed(format!(
                    "fan '{fan_id}' is already auto-tuning"
                )));
            }
        }

        let observation_window_ms = self
            .inner
            .tuning
            .read()
            .await
            .auto_tune_observation_window_ms;
        self.inner.auto_tune.write().await.insert(
            fan_id.to_string(),
            AutoTuneExecutionState::Running {
                started_at: std::time::Instant::now(),
                observation_window_ms,
                samples: Vec::new(),
            },
        );

        let locals = self.inner.fan_locals.read().await;
        if let Some(local) = locals.get(fan_id) {
            Self::write_fan_local(local, |status| {
                status.auto_tuning = true;
                status.logical_output_percent = Some(100.0);
            });
        }

        let snapshot_to_publish = locals
            .get(fan_id)
            .and_then(|local| local.lock().ok().map(|guard| guard.clone()));
        drop(locals);

        if let Some(snapshot) = snapshot_to_publish
            && let Some(entry) = self.inner.status.write().await.get_mut(fan_id)
        {
            *entry = snapshot;
        }

        Ok(())
    }

    pub async fn auto_tune_result_json(&self, fan_id: &str) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.auto_tune_result_view(fan_id).await)
    }

    pub async fn auto_tune_result_view(&self, fan_id: &str) -> AutoTuneResultView {
        match self.inner.auto_tune.read().await.get(fan_id) {
            Some(AutoTuneExecutionState::Running {
                observation_window_ms,
                ..
            }) => AutoTuneResultView::Running {
                observation_window_ms: *observation_window_ms,
            },
            Some(AutoTuneExecutionState::Completed {
                observation_window_ms,
                proposal,
            }) => AutoTuneResultView::Completed {
                observation_window_ms: *observation_window_ms,
                proposal: proposal.clone(),
            },
            Some(AutoTuneExecutionState::Failed {
                observation_window_ms,
                error,
            }) => AutoTuneResultView::Failed {
                observation_window_ms: *observation_window_ms,
                error: error.clone(),
            },
            None => AutoTuneResultView::Idle {
                observation_window_ms: self
                    .inner
                    .tuning
                    .read()
                    .await
                    .auto_tune_observation_window_ms,
            },
        }
    }

    pub async fn auto_tune_output_override(&self, fan_id: &str) -> Option<f64> {
        if matches!(
            self.inner.auto_tune.read().await.get(fan_id),
            Some(AutoTuneExecutionState::Running { .. })
        ) {
            Some(100.0)
        } else {
            None
        }
    }

    pub async fn record_auto_tune_sample(&self, fan_id: &str, aggregated_temp_millidegrees: i64) {
        let mut should_emit = false;
        let is_running;
        {
            let mut auto_tune = self.inner.auto_tune.write().await;
            let transition = if let Some(AutoTuneExecutionState::Running {
                started_at,
                observation_window_ms,
                samples,
            }) = auto_tune.get_mut(fan_id)
            {
                let elapsed_ms = started_at.elapsed().as_millis() as u64;
                samples.push(AutoTuneSample {
                    elapsed_ms,
                    aggregated_temp_millidegrees,
                });
                let obs_window = *observation_window_ms;

                if elapsed_ms >= obs_window {
                    let result = proposal_from_auto_tune_samples(obs_window, samples);
                    Some(match result {
                        Ok(proposal) => {
                            should_emit = true;
                            AutoTuneExecutionState::Completed {
                                observation_window_ms: obs_window,
                                proposal,
                            }
                        }
                        Err(error) => AutoTuneExecutionState::Failed {
                            observation_window_ms: obs_window,
                            error,
                        },
                    })
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(new_state) = transition
                && let Some(state) = auto_tune.get_mut(fan_id)
            {
                *state = new_state;
            }

            is_running = matches!(
                auto_tune.get(fan_id),
                Some(AutoTuneExecutionState::Running { .. })
            );
        }

        let locals = self.inner.fan_locals.read().await;
        if let Some(local) = locals.get(fan_id) {
            Self::write_fan_local(local, |status| {
                status.auto_tuning = is_running;
            });
        }

        if should_emit {
            if let Some(local) = locals.get(fan_id) {
                Self::write_fan_local(local, |status| status.auto_tuning = false);
            }
            if let Some(connection) = self.inner.signal_connection.read().await.clone() {
                emit_auto_tune_completed(&connection, fan_id).await;
            }
        }
    }

    pub async fn fail_auto_tune(&self, fan_id: &str, error: String) {
        let observation_window_ms = self
            .inner
            .tuning
            .read()
            .await
            .auto_tune_observation_window_ms;
        self.inner.auto_tune.write().await.insert(
            fan_id.to_string(),
            AutoTuneExecutionState::Failed {
                observation_window_ms,
                error,
            },
        );
        let locals = self.inner.fan_locals.read().await;
        if let Some(local) = locals.get(fan_id) {
            Self::write_fan_local(local, |status| status.auto_tuning = false);
        }
    }

    pub async fn accepted_auto_tune_proposal(&self, fan_id: &str) -> fdo::Result<AutoTuneProposal> {
        match self.inner.auto_tune.read().await.get(fan_id) {
            Some(AutoTuneExecutionState::Completed { proposal, .. }) => Ok(proposal.clone()),
            Some(AutoTuneExecutionState::Failed { error, .. }) => Err(fdo::Error::Failed(format!(
                "auto-tune failed for '{fan_id}': {error}"
            ))),
            Some(AutoTuneExecutionState::Running { .. }) => Err(fdo::Error::Failed(format!(
                "auto-tune is still running for '{fan_id}'"
            ))),
            None => Err(fdo::Error::Failed(format!(
                "no auto-tune result is available for '{fan_id}'"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use kde_fan_control_core::config::{AppConfig, DegradedState};
    use kde_fan_control_core::inventory::ControlMode;
    use kde_fan_control_core::lifecycle::OwnedFanSet;
    use tokio::sync::RwLock;

    use crate::control::supervisor::ControlSupervisor;
    use crate::state::AutoTuneResultView;
    use crate::test_support::{ControlFixture, applied_config_for, test_snapshot};

    async fn auto_tune_test_harness(
        fixture: &ControlFixture,
    ) -> (
        ControlSupervisor,
        Arc<RwLock<AppConfig>>,
        Arc<RwLock<DegradedState>>,
    ) {
        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let applied = applied_config_for(
            "hwmon-test-0000000000000001-fan1",
            "hwmon-test-0000000000000001-temp1",
        );
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));
        let supervisor = ControlSupervisor::new(
            Arc::clone(&snapshot),
            Arc::clone(&config),
            owned,
            Arc::clone(&degraded),
        );
        supervisor.set_auto_tune_observation_window_ms(40).await;
        supervisor.reconcile().await;
        (supervisor, config, degraded)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auto_tune_start_puts_managed_fan_into_bounded_running_state() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, _, _) = auto_tune_test_harness(&fixture).await;
        supervisor
            .start_auto_tune("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune should start");

        let result = supervisor
            .auto_tune_result_json("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune result should serialize");
        assert!(result.contains("running"));
        assert!(result.contains("40"));

        let status = supervisor
            .status_json()
            .await
            .expect("status should serialize");
        assert!(status.contains("\"auto_tuning\":true"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auto_tune_unreadable_temp_records_failure_without_mutating_applied_gains() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        let original_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;

        supervisor
            .start_auto_tune("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune should start");
        tokio::time::sleep(Duration::from_millis(60)).await;

        let result = supervisor
            .auto_tune_result_json("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune result should serialize");
        assert!(result.contains("failed"));

        let applied_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;
        assert_eq!(applied_gains, original_gains);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn auto_tune_completion_exposes_softened_proposal_without_mutating_applied_gains() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        let original_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;

        supervisor
            .start_auto_tune("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune should start");

        tokio::time::sleep(Duration::from_millis(15)).await;
        fixture.write_temp("59000\n");
        tokio::time::sleep(Duration::from_millis(15)).await;
        fixture.write_temp("57500\n");
        tokio::time::sleep(Duration::from_millis(60)).await;

        let result = supervisor
            .auto_tune_result_view("hwmon-test-0000000000000001-fan1")
            .await;
        match result {
            AutoTuneResultView::Completed {
                proposal,
                observation_window_ms,
            } => {
                assert_eq!(observation_window_ms, 40);
                assert!(proposal.proposed_gains.kp > 0.0);
                assert!(proposal.proposed_gains.ki > 0.0);
                assert!(proposal.proposed_gains.kd > 0.0);
            }
            other => panic!("expected completed auto-tune result, got {other:?}"),
        }

        let applied_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;
        assert_eq!(applied_gains, original_gains);
    }
}
