//! Degraded fan detection, panic recovery, stale-data monitoring, and re-assessment.
//!
//! This module contains the `ControlSupervisor` methods responsible for
//! detecting and handling fan control failures: publishing periodic status
//! batches, monitoring for stale sensor data, recovering from task panics,
//! degrading fans on write failures, clearing status, and re-assessing
//! previously degraded fans when hardware conditions improve.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::{MissedTickBehavior, interval};

use kde_fan_control_core::config::{DegradedReason, LifecycleEvent, LifecycleEventLog};
use kde_fan_control_core::lifecycle::ControlRuntimeSnapshot;

use crate::control::helpers::control_snapshot_from_applied;
use crate::control::supervisor::ControlSupervisor;
use crate::control::supervisor::ControlTaskHandle;
use crate::dbus::signals::{
    emit_applied_config_changed, emit_degraded_state_changed, emit_lifecycle_event_appended,
};
use crate::safety::ownership::persist_owned_fans;
use crate::time::format_iso8601_now;

impl ControlSupervisor {
    pub async fn publish_status_batch(&self) {
        let locals = self.inner.fan_locals.read().await;
        {
            let mut status = self.inner.status.write().await;
            for (fan_id, local) in locals.iter() {
                if let Ok(guard) = local.lock()
                    && let Some(entry) = status.get_mut(fan_id)
                {
                    *entry = guard.clone();
                }
            }
        }
        drop(locals);

        let rpm_locals = self.inner.rpm_locals.read().await;
        {
            let mut snapshot = self.inner.snapshot.write().await;
            for (fan_id, rpm_local) in rpm_locals.iter() {
                if let Ok(guard) = rpm_local.lock() {
                    snapshot.update_fan_rpm(fan_id, *guard);
                }
            }
        }
    }

    const STALE_THRESHOLD: u32 = 100;

    pub async fn run_publish_loop(&self, cadence_ms: u64) {
        let mut tick = interval(Duration::from_millis(cadence_ms));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            self.publish_status_batch().await;
            self.check_stale_fans().await;
            self.check_task_panics().await;
        }
    }

    pub async fn check_stale_fans(&self) {
        let locals = self.inner.fan_locals.read().await;
        let mut stale_counters = self.inner.stale_fan_counters.write().await;
        let mut to_degrade = Vec::new();

        for (fan_id, local) in locals.iter() {
            let is_stale = if let Ok(guard) = local.lock() {
                guard.aggregated_temp_millidegrees.unwrap_or(0) == 0
            } else {
                true
            };

            if is_stale {
                let count = stale_counters.entry(fan_id.clone()).or_insert(0);
                *count += 1;
                if *count == Self::STALE_THRESHOLD / 2 {
                    tracing::warn!(
                        fan_id = %fan_id,
                        stale_ticks = *count,
                        "managed fan has produced no valid sensor data for an extended period"
                    );
                }
                if *count >= Self::STALE_THRESHOLD {
                    to_degrade.push(fan_id.clone());
                }
            } else {
                stale_counters.insert(fan_id.clone(), 0);
            }
        }

        drop(stale_counters);
        drop(locals);

        for fan_id in to_degrade {
            tracing::error!(
                fan_id = %fan_id,
                "auto-degrading managed fan: no valid sensor data for extended period"
            );
            self.degrade_and_stop(
                &fan_id,
                DegradedReason::StaleSensorData {
                    fan_id: fan_id.clone(),
                },
            )
            .await;

            self.inner.fan_locals.write().await.remove(&fan_id);
            self.inner.rpm_locals.write().await.remove(&fan_id);
            self.inner.stale_fan_counters.write().await.remove(&fan_id);
        }
    }

    pub async fn check_task_panics(&self) {
        let mut panicked = Vec::new();
        {
            let tasks = self.inner.tasks.read().await;
            for (fan_id, task_handle) in tasks.iter() {
                if task_handle.handle.is_finished() {
                    panicked.push(fan_id.clone());
                }
            }
        }

        if panicked.is_empty() {
            return;
        }

        for fan_id in panicked {
            let task_handle = self.inner.tasks.write().await.remove(&fan_id);
            if let Some(handle) = task_handle {
                let result = handle.handle.await;
                if let Err(_) = result {
                    tracing::error!(fan_id = %fan_id, "control task panicked — writing fallback and degrading");
                    let reason = DegradedReason::FanNoLongerEnrollable {
                        fan_id: fan_id.clone(),
                        support_state: kde_fan_control_core::inventory::SupportState::Unavailable,
                        reason: "control task panicked".to_string(),
                    };
                    self.degrade_and_stop(&fan_id, reason).await;
                }
            }
        }
    }

    pub async fn clear_status(&self, fan_id: &str) {
        self.inner.status.write().await.remove(fan_id);
        self.inner.tasks.write().await.remove(fan_id);
    }

    pub async fn degrade_and_stop(&self, fan_id: &str, reason: DegradedReason) {
        {
            let owned = self.inner.owned.read().await;
            if let Err(e) = kde_fan_control_core::lifecycle::write_fallback_single(fan_id, &owned) {
                tracing::error!(fan_id = %fan_id, error = %e, "failed to write fallback PWM before degrading fan");
            }
        }
        self.inner
            .degraded
            .write()
            .await
            .mark_degraded(fan_id.to_string(), vec![reason]);
        self.clear_status(fan_id).await;
    }

    pub async fn handle_live_write_failure(&self, fan_id: &str, error: &str) {
        let degraded_reason = DegradedReason::FanNoLongerEnrollable {
            fan_id: fan_id.to_string(),
            support_state: kde_fan_control_core::inventory::SupportState::Unavailable,
            reason: format!("live pwm write failed: {error}"),
        };
        self.degrade_and_stop(fan_id, degraded_reason).await;
    }

    pub async fn reassess_degraded_fans(&self, events: &Arc<RwLock<LifecycleEventLog>>) {
        let recoverable_fans: Vec<String> = {
            let degraded = self.inner.degraded.read().await;
            degraded
                .degraded_fan_ids()
                .filter(|fan_id| degraded.is_fan_recoverable(fan_id))
                .map(|s| s.to_string())
                .collect()
        };

        if recoverable_fans.is_empty() {
            return;
        }

        let snapshot = self.inner.snapshot.read().await.clone();
        let applied = {
            let config = self.inner.config.read().await;
            config.applied.clone()
        };

        let Some(applied) = applied else {
            return;
        };

        let mut recovered_ids = Vec::new();

        for fan_id in recoverable_fans {
            let Some(applied_entry) = applied.fans.get(&fan_id) else {
                continue;
            };

            let outcome = kde_fan_control_core::lifecycle::reassess_single_fan(
                &fan_id,
                applied_entry,
                &snapshot,
            );

            match outcome {
                kde_fan_control_core::lifecycle::ReassessOutcome::Recoverable {
                    fan_id: recovered_id,
                    control_mode,
                    temp_sources: _,
                } => {
                    let sysfs_path = snapshot
                        .devices
                        .iter()
                        .flat_map(|d| d.fans.iter())
                        .find(|f| f.id == recovered_id)
                        .map(|f| {
                            let device_path = snapshot
                                .devices
                                .iter()
                                .find(|d| d.fans.iter().any(|fc| fc.id == recovered_id))
                                .map(|d| d.sysfs_path.as_str())
                                .unwrap_or("");
                            format!("{}/pwm{}", device_path, f.channel)
                        })
                        .unwrap_or_default();

                    self.inner.degraded.write().await.clear_fan(&recovered_id);

                    self.inner.owned.write().await.claim_fan(
                        &recovered_id,
                        control_mode,
                        &sysfs_path,
                    );

                    self.sync_panic_fallback_mirror().await;

                    {
                        let owned = self.inner.owned.read().await;
                        persist_owned_fans(&owned);
                    }

                    let initial = control_snapshot_from_applied(applied_entry);
                    self.inner
                        .status
                        .write()
                        .await
                        .insert(recovered_id.clone(), initial.clone());

                    let local: Arc<StdMutex<ControlRuntimeSnapshot>> =
                        Arc::new(StdMutex::new(initial.clone()));
                    self.inner
                        .fan_locals
                        .write()
                        .await
                        .insert(recovered_id.clone(), Arc::clone(&local));

                    let rpm_local: Arc<StdMutex<Option<u64>>> = Arc::new(StdMutex::new(None));
                    self.inner
                        .rpm_locals
                        .write()
                        .await
                        .insert(recovered_id.clone(), Arc::clone(&rpm_local));

                    let supervisor = self.clone();
                    let fan_id_for_task = recovered_id.clone();
                    let local_for_task = Arc::clone(&local);
                    let rpm_local_for_task = Arc::clone(&rpm_local);
                    let entry_clone = applied_entry.clone();
                    let handle = tokio::spawn(async move {
                        supervisor
                            .run_fan_loop(
                                fan_id_for_task,
                                entry_clone,
                                local_for_task,
                                rpm_local_for_task,
                            )
                            .await;
                    });

                    self.inner
                        .tasks
                        .write()
                        .await
                        .insert(recovered_id.clone(), ControlTaskHandle { handle });

                    events.write().await.push(LifecycleEvent {
                        timestamp: format_iso8601_now(),
                        reason: DegradedReason::FanRecovered {
                            fan_id: recovered_id.clone(),
                        },
                        detail: Some(format!(
                            "fan {recovered_id} recovered from degraded state via re-assessment"
                        )),
                    });

                    tracing::info!(fan_id = %recovered_id, "recovered degraded fan via re-assessment");

                    recovered_ids.push(recovered_id);
                }
                kde_fan_control_core::lifecycle::ReassessOutcome::StillDegraded {
                    fan_id: _,
                    reason: _,
                } => {}
            }
        }

        if !recovered_ids.is_empty()
            && let Some(connection) = self.inner.signal_connection.read().await.clone()
        {
            emit_degraded_state_changed(&connection).await;
            emit_applied_config_changed(&connection).await;
            for fan_id in &recovered_ids {
                emit_lifecycle_event_appended(
                    &connection,
                    "fan_recovered",
                    &format!("fan {fan_id} recovered from degraded state"),
                )
                .await;
            }
        }
    }
}
