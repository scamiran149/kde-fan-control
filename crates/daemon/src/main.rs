use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::RwLock as StdRwLock;
use std::time::Duration;
use std::time::Instant;

use clap::Parser;
use kde_fan_control_core::config::{
    AppConfig, AppliedFanEntry, DegradedReason, DegradedState, DraftFanEntry, LifecycleEvent,
    LifecycleEventLog, app_state_dir, apply_draft, validate_draft,
};
use kde_fan_control_core::control::{
    ActuatorPolicy, AggregationFn, AutoTuneProposal, ControlCadence, PidController, PidGains,
    PidLimits, map_output_percent_to_pwm, startup_kick_required,
};
use kde_fan_control_core::inventory::{ControlMode, InventorySnapshot, discover, discover_from};
use kde_fan_control_core::lifecycle::{
    ControlRuntimeSnapshot, FallbackResult, FanRuntimeStatus, OwnedFanSet, RuntimeState,
    lifecycle_event_from_fallback_incident, perform_boot_reconciliation, write_fallback_for_owned,
};
use kde_fan_control_core::overview::{OverviewStructureSnapshot, OverviewTelemetryBatch};
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval};
use tracing_subscriber::EnvFilter;
use zbus::fdo;
use zbus::{connection::Builder, interface, object_server::SignalEmitter};

use sd_notify::NotifyState;

const BUS_NAME: &str = "org.kde.FanControl";
const BUS_PATH_INVENTORY: &str = "/org/kde/FanControl";
const BUS_PATH_LIFECYCLE: &str = "/org/kde/FanControl/Lifecycle";
const BUS_PATH_CONTROL: &str = "/org/kde/FanControl/Control";

const MAX_NAME_LENGTH: usize = 128;

fn state_dir() -> PathBuf {
    app_state_dir()
}

fn owned_fans_path() -> PathBuf {
    state_dir().join("owned-fans.json")
}

fn persist_owned_fans(owned: &OwnedFanSet) {
    let path = owned_fans_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let fans: Vec<serde_json::Value> = owned
        .owned_fan_ids()
        .filter_map(|fan_id| {
            owned.sysfs_path(fan_id).map(|p| {
                serde_json::json!({
                    "fan_id": fan_id,
                    "sysfs_path": p,
                })
            })
        })
        .collect();
    let doc = serde_json::json!({ "fans": fans });
    match serde_json::to_string_pretty(&doc) {
        Ok(json) => {
            let tmp_path = path.with_extension("json.tmp");
            if let Err(e) = fs::write(&tmp_path, &json) {
                tracing::warn!(path = %tmp_path.display(), error = %e, "failed to write owned-fans list (temp)");
                return;
            }
            if let Err(e) = fs::rename(&tmp_path, &path) {
                tracing::warn!(from = %tmp_path.display(), to = %path.display(), error = %e, "failed to rename owned-fans list");
                let _ = fs::remove_file(&tmp_path);
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize owned-fans list");
        }
    }
}

#[derive(Debug)]
struct ControlTaskHandle {
    handle: JoinHandle<()>,
}

#[derive(Debug)]
struct ControlSupervisorInner {
    snapshot: Arc<RwLock<InventorySnapshot>>,
    config: Arc<RwLock<AppConfig>>,
    owned: Arc<RwLock<OwnedFanSet>>,
    degraded: Arc<RwLock<DegradedState>>,
    tasks: RwLock<HashMap<String, ControlTaskHandle>>,
    status: RwLock<HashMap<String, ControlRuntimeSnapshot>>,
    fan_locals: RwLock<HashMap<String, Arc<StdMutex<ControlRuntimeSnapshot>>>>,
    rpm_locals: RwLock<HashMap<String, Arc<StdMutex<Option<u64>>>>>,
    stale_fan_counters: RwLock<HashMap<String, u32>>,
    publish_task: RwLock<Option<JoinHandle<()>>>,
    auto_tune: RwLock<HashMap<String, AutoTuneExecutionState>>,
    tuning: RwLock<DaemonTuningSettings>,
    signal_connection: RwLock<Option<zbus::Connection>>,
    panic_fallback_mirror: Arc<PanicFallbackMirror>,
}

#[derive(Clone, Debug)]
struct ControlSupervisor {
    inner: Arc<ControlSupervisorInner>,
}

#[derive(Debug, Default)]
struct PanicFallbackMirror {
    owned_pwm_paths: StdRwLock<Vec<(String, String)>>,
}

#[derive(Debug, Clone, Copy)]
struct DaemonTuningSettings {
    auto_tune_observation_window_ms: u64,
}

impl Default for DaemonTuningSettings {
    fn default() -> Self {
        Self {
            auto_tune_observation_window_ms: 30_000,
        }
    }
}

#[derive(Debug, Clone)]
struct AutoTuneSample {
    elapsed_ms: u64,
    aggregated_temp_millidegrees: i64,
}

#[derive(Debug, Clone)]
enum AutoTuneExecutionState {
    Running {
        started_at: Instant,
        observation_window_ms: u64,
        samples: Vec<AutoTuneSample>,
    },
    Completed {
        observation_window_ms: u64,
        proposal: AutoTuneProposal,
    },
    Failed {
        observation_window_ms: u64,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum AutoTuneResultView {
    Idle {
        observation_window_ms: u64,
    },
    Running {
        observation_window_ms: u64,
    },
    Completed {
        observation_window_ms: u64,
        proposal: AutoTuneProposal,
    },
    Failed {
        observation_window_ms: u64,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DraftFanControlProfilePayload {
    #[serde(default)]
    temp_sources: Option<Vec<String>>,
    #[serde(default)]
    target_temp_millidegrees: Option<Option<i64>>,
    #[serde(default)]
    aggregation: Option<Option<AggregationFn>>,
    #[serde(default)]
    pid_gains: Option<Option<PidGains>>,
    #[serde(default)]
    cadence: Option<Option<ControlCadence>>,
    #[serde(default)]
    deadband_millidegrees: Option<Option<i64>>,
    #[serde(default)]
    actuator_policy: Option<Option<ActuatorPolicy>>,
    #[serde(default)]
    pid_limits: Option<Option<PidLimits>>,
}

impl ControlSupervisor {
    fn new(
        snapshot: Arc<RwLock<InventorySnapshot>>,
        config: Arc<RwLock<AppConfig>>,
        owned: Arc<RwLock<OwnedFanSet>>,
        degraded: Arc<RwLock<DegradedState>>,
    ) -> Self {
        Self {
            inner: Arc::new(ControlSupervisorInner {
                snapshot,
                config,
                owned,
                degraded,
                tasks: RwLock::new(HashMap::new()),
                status: RwLock::new(HashMap::new()),
                fan_locals: RwLock::new(HashMap::new()),
                rpm_locals: RwLock::new(HashMap::new()),
                stale_fan_counters: RwLock::new(HashMap::new()),
                publish_task: RwLock::new(None),
                auto_tune: RwLock::new(HashMap::new()),
                tuning: RwLock::new(DaemonTuningSettings::default()),
                signal_connection: RwLock::new(None),
                panic_fallback_mirror: Arc::new(PanicFallbackMirror::default()),
            }),
        }
    }

    fn panic_fallback_mirror(&self) -> Arc<PanicFallbackMirror> {
        Arc::clone(&self.inner.panic_fallback_mirror)
    }

    async fn sync_panic_fallback_mirror(&self) {
        let owned = self.inner.owned.read().await;
        sync_panic_fallback_mirror_from_owned(&self.inner.panic_fallback_mirror, &owned);
    }

    async fn set_signal_connection(&self, connection: zbus::Connection) {
        *self.inner.signal_connection.write().await = Some(connection);
    }

    #[allow(dead_code)]
    async fn set_auto_tune_observation_window_ms(&self, observation_window_ms: u64) {
        self.inner
            .tuning
            .write()
            .await
            .auto_tune_observation_window_ms = observation_window_ms;
    }

    async fn stop_all(&self) {
        let mut tasks = self.inner.tasks.write().await;
        for (_, task) in tasks.drain() {
            task.handle.abort();
        }
        drop(tasks);

        if let Some(handle) = self.inner.publish_task.write().await.take() {
            handle.abort();
        }

        self.inner.fan_locals.write().await.clear();
        self.inner.rpm_locals.write().await.clear();
        self.inner.stale_fan_counters.write().await.clear();
        self.inner.status.write().await.clear();
    }

    async fn reconcile(&self) {
        let desired = {
            let config = self.inner.config.read().await;
            let owned = self.inner.owned.read().await;
            config
                .applied
                .as_ref()
                .map(|applied| {
                    applied
                        .fans
                        .iter()
                        .filter(|(fan_id, _)| owned.owns(fan_id))
                        .map(|(fan_id, entry)| (fan_id.clone(), entry.clone()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };

        let publish_cadence_ms = desired
            .first()
            .map(|(_, e)| e.cadence.control_interval_ms.max(100))
            .unwrap_or(250);

        self.stop_all().await;

        for (fan_id, entry) in desired {
            self.inner.degraded.write().await.clear_fan(&fan_id);

            let initial = control_snapshot_from_applied(&entry);
            self.inner
                .status
                .write()
                .await
                .insert(fan_id.clone(), initial.clone());

            let local: Arc<StdMutex<ControlRuntimeSnapshot>> = Arc::new(StdMutex::new(initial));
            self.inner
                .fan_locals
                .write()
                .await
                .insert(fan_id.clone(), Arc::clone(&local));

            let rpm_local: Arc<StdMutex<Option<u64>>> = Arc::new(StdMutex::new(None));
            self.inner
                .rpm_locals
                .write()
                .await
                .insert(fan_id.clone(), Arc::clone(&rpm_local));

            let supervisor = self.clone();
            let fan_id_for_task = fan_id.clone();
            let local_for_task = Arc::clone(&local);
            let rpm_local_for_task = Arc::clone(&rpm_local);
            let handle = tokio::spawn(async move {
                supervisor
                    .run_fan_loop(fan_id_for_task, entry, local_for_task, rpm_local_for_task)
                    .await;
            });

            self.inner
                .tasks
                .write()
                .await
                .insert(fan_id, ControlTaskHandle { handle });
        }

        let supervisor = self.clone();
        let publish_handle = tokio::spawn(async move {
            supervisor.run_publish_loop(publish_cadence_ms).await;
        });
        *self.inner.publish_task.write().await = Some(publish_handle);
    }

    async fn status_json(&self) -> Result<String, serde_json::Error> {
        let status = self.inner.status.read().await;
        serde_json::to_string(&*status)
    }

    async fn runtime_state_snapshot(&self, fallback_fan_ids: &HashSet<String>) -> RuntimeState {
        let (owned_guard, applied_guard, snapshot_guard, degraded_guard, live_status) = {
            let owned = self.inner.owned.read().await;
            let config = self.inner.config.read().await;
            let snapshot = self.inner.snapshot.read().await;
            let degraded = self.inner.degraded.read().await;
            let status = self.inner.status.read().await;
            (
                owned.clone(),
                config.applied.clone(),
                snapshot.clone(),
                degraded.clone(),
                status.clone(),
            )
        };

        let mut state = RuntimeState::build(
            &owned_guard,
            applied_guard.as_ref(),
            &degraded_guard,
            fallback_fan_ids,
            &snapshot_guard,
        );

        for (fan_id, control) in live_status {
            if let Some(FanRuntimeStatus::Managed {
                control: existing, ..
            }) = state.fan_statuses.get_mut(&fan_id)
            {
                *existing = control;
            }
        }

        state
    }

    fn write_fan_local(
        local: &Arc<StdMutex<ControlRuntimeSnapshot>>,
        update: impl FnOnce(&mut ControlRuntimeSnapshot),
    ) {
        if let Ok(mut guard) = local.lock() {
            update(&mut guard);
        }
    }

    async fn publish_status_batch(&self) {
        let locals = self.inner.fan_locals.read().await;
        {
            let mut status = self.inner.status.write().await;
            for (fan_id, local) in locals.iter() {
                if let Ok(guard) = local.lock() {
                    if let Some(entry) = status.get_mut(fan_id) {
                        *entry = guard.clone();
                    }
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

    async fn run_publish_loop(&self, cadence_ms: u64) {
        let mut tick = interval(Duration::from_millis(cadence_ms));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            self.publish_status_batch().await;
            self.check_stale_fans().await;
            self.check_task_panics().await;
        }
    }

    async fn check_stale_fans(&self) {
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

    /// Check whether any control task has panicked.
    ///
    /// Control tasks are spawned with `tokio::spawn` and their `JoinHandle`
    /// is stored in `self.inner.tasks`. If a task panics, the handle
    /// completes with `Err(JoinError)`, but nothing currently polls for
    /// this — the fan stays in `OwnedFanSet` but receives no control
    /// updates until the stale-data detector fires (~25 s).
    ///
    /// This method is called every publish-loop tick so that panics are
    /// detected within one cadence period.
    async fn check_task_panics(&self) {
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

    async fn start_auto_tune(&self, fan_id: &str) -> fdo::Result<()> {
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
                started_at: Instant::now(),
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

        if let Some(snapshot) = snapshot_to_publish {
            if let Some(entry) = self.inner.status.write().await.get_mut(fan_id) {
                *entry = snapshot;
            }
        }

        Ok(())
    }

    async fn auto_tune_result_json(&self, fan_id: &str) -> Result<String, serde_json::Error> {
        serde_json::to_string(&self.auto_tune_result_view(fan_id).await)
    }

    async fn auto_tune_result_view(&self, fan_id: &str) -> AutoTuneResultView {
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

    async fn auto_tune_output_override(&self, fan_id: &str) -> Option<f64> {
        if matches!(
            self.inner.auto_tune.read().await.get(fan_id),
            Some(AutoTuneExecutionState::Running { .. })
        ) {
            Some(100.0)
        } else {
            None
        }
    }

    async fn record_auto_tune_sample(&self, fan_id: &str, aggregated_temp_millidegrees: i64) {
        let mut should_emit = false;
        let is_running;
        {
            let mut auto_tune = self.inner.auto_tune.write().await;
            if let Some(AutoTuneExecutionState::Running {
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

                if elapsed_ms >= *observation_window_ms {
                    let result = proposal_from_auto_tune_samples(*observation_window_ms, samples);
                    match result {
                        Ok(proposal) => {
                            *auto_tune.get_mut(fan_id).expect("state should exist") =
                                AutoTuneExecutionState::Completed {
                                    observation_window_ms: *observation_window_ms,
                                    proposal,
                                };
                            should_emit = true;
                        }
                        Err(error) => {
                            *auto_tune.get_mut(fan_id).expect("state should exist") =
                                AutoTuneExecutionState::Failed {
                                    observation_window_ms: *observation_window_ms,
                                    error,
                                };
                        }
                    }
                }
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

    async fn fail_auto_tune(&self, fan_id: &str, error: String) {
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

    async fn accepted_auto_tune_proposal(&self, fan_id: &str) -> fdo::Result<AutoTuneProposal> {
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

    async fn run_fan_loop(
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

                    if let Some(ref path) = rpm_path {
                        if let Some(rpm) = fs::read_to_string(path)
                            .ok()
                            .and_then(|v| v.trim().parse::<u64>().ok())
                        {
                            if let Ok(mut guard) = rpm_local.lock() {
                                *guard = Some(rpm);
                            }
                        }
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

    async fn sample_temperatures(
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

    async fn clear_status(&self, fan_id: &str) {
        self.inner.status.write().await.remove(fan_id);
        self.inner.tasks.write().await.remove(fan_id);
    }

    async fn degrade_and_stop(&self, fan_id: &str, reason: DegradedReason) {
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

    async fn handle_live_write_failure(&self, fan_id: &str, error: &str) {
        let degraded_reason = DegradedReason::FanNoLongerEnrollable {
            fan_id: fan_id.to_string(),
            support_state: kde_fan_control_core::inventory::SupportState::Unavailable,
            reason: format!("live pwm write failed: {error}"),
        };
        self.degrade_and_stop(fan_id, degraded_reason).await;
    }

    async fn reassess_degraded_fans(&self, events: &Arc<RwLock<LifecycleEventLog>>) {
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

                    self.inner
                        .owned
                        .write()
                        .await
                        .claim_fan(&recovered_id, control_mode, &sysfs_path);

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

                    let rpm_local: Arc<StdMutex<Option<u64>>> =
                        Arc::new(StdMutex::new(None));
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
                        timestamp: kde_fan_control_core::lifecycle::format_iso8601_now(),
                        reason: DegradedReason::FanRecovered {
                            fan_id: recovered_id.clone(),
                        },
                        detail: Some(format!(
                            "fan {} recovered from degraded state via re-assessment",
                            recovered_id
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

        if !recovered_ids.is_empty() {
            if let Some(connection) = self.inner.signal_connection.read().await.clone() {
                emit_degraded_state_changed(&connection).await;
                emit_applied_config_changed(&connection).await;
                for fan_id in &recovered_ids {
                    emit_lifecycle_event_appended(&connection, "fan_recovered", &format!("fan {} recovered from degraded state", fan_id)).await;
                }
            }
        }
    }
}

fn control_snapshot_from_applied(entry: &AppliedFanEntry) -> ControlRuntimeSnapshot {
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

fn draft_entry_from_applied(entry: &AppliedFanEntry) -> DraftFanEntry {
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

fn proposal_from_auto_tune_samples(
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

fn resolve_temp_sources(
    snapshot: &InventorySnapshot,
    temp_sources: &[String],
) -> Vec<(String, PathBuf)> {
    temp_sources
        .iter()
        .filter_map(|temp_id| {
            snapshot.devices.iter().find_map(|device| {
                device
                    .temperatures
                    .iter()
                    .find(|sensor| &sensor.id == temp_id)
                    .map(|sensor| {
                        (
                            temp_id.clone(),
                            PathBuf::from(&device.sysfs_path)
                                .join(format!("temp{}_input", sensor.channel)),
                        )
                    })
            })
        })
        .collect()
}

fn resolve_fan_rpm_path(snapshot: &InventorySnapshot, fan_id: &str) -> Option<PathBuf> {
    snapshot.devices.iter().find_map(|device| {
        device
            .fans
            .iter()
            .find(|fan| fan.id == fan_id && fan.rpm_feedback)
            .map(|fan| PathBuf::from(&device.sysfs_path).join(format!("fan{}_input", fan.channel)))
    })
}

fn write_pwm_value(pwm_path: &str, pwm_value: u16) -> std::io::Result<()> {
    let pwm_enable_path = format!("{}_enable", pwm_path);
    if let Err(error) = fs::write(
        &pwm_enable_path,
        kde_fan_control_core::lifecycle::PWM_ENABLE_MANUAL.to_string(),
    ) {
        tracing::warn!(path = %pwm_enable_path, error = %error, "failed to set pwm channel to manual mode before write");
    }
    fs::write(pwm_path, pwm_value.to_string())
}

#[allow(dead_code)]
fn require_test_authorized(authorized: bool) -> fdo::Result<()> {
    if authorized {
        Ok(())
    } else {
        Err(fdo::Error::AccessDenied(
            "privileged operations require root access".into(),
        ))
    }
}

#[derive(Parser)]
#[command(name = "kde-fan-control-daemon")]
#[command(about = "Daemon for KDE Fan Control")]
struct DaemonArgs {
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    session_bus: bool,
}

// ---------------------------------------------------------------------------
// Authorization boundary
// ---------------------------------------------------------------------------

/// Check whether the caller of a DBus method is authorized for privileged
/// operations. Tries polkit CheckAuthorization first; falls back to UID 0
/// if the polkit authority is unavailable.
const POLKIT_ACTION_ID: &str = "org.kde.fancontrol.write-config";

async fn require_authorized(
    connection: &zbus::Connection,
    header: &zbus::message::Header<'_>,
) -> fdo::Result<()> {
    let sender = header
        .sender()
        .ok_or_else(|| fdo::Error::AccessDenied("no sender in message header".into()))?;

    let dbus_proxy = fdo::DBusProxy::new(connection).await.map_err(|e| {
        fdo::Error::AccessDenied(format!(
            "could not connect to DBus daemon for auth check: {e}"
        ))
    })?;

    let bus_name = zbus::names::BusName::Unique(sender.clone());
    let uid: u32 = dbus_proxy
        .get_connection_unix_user(bus_name.clone())
        .await
        .map_err(|e| fdo::Error::AccessDenied(format!("could not resolve caller identity: {e}")))?;

    let pid: u32 = dbus_proxy
        .get_connection_unix_process_id(bus_name)
        .await
        .unwrap_or(0);

    match check_polkit_authorization(connection, uid, pid).await {
        Ok(true) => Ok(()),
        Ok(false) => {
            tracing::warn!(caller_uid = uid, "polkit authorization denied");
            Err(fdo::Error::AccessDenied("authentication required".into()))
        }
        Err(e) => {
            tracing::warn!(error = %e, caller_uid = uid, "polkit unavailable, falling back to UID-0 check");
            if uid != 0 {
                tracing::warn!(caller_uid = uid, "unauthorized write attempt (no polkit)");
                return Err(fdo::Error::AccessDenied(
                    "privileged operations require root access (polkit unavailable)".into(),
                ));
            }
            Ok(())
        }
    }
}

async fn check_polkit_authorization(
    _connection: &zbus::Connection,
    uid: u32,
    pid: u32,
) -> Result<bool, String> {
    use std::collections::HashMap;
    use zbus::zvariant::Value;

    // Polkit lives on the system bus. The daemon may be running on the
    // session bus (dev mode), so always open a system-bus connection
    // for the polkit call rather than reusing the daemon's connection.
    let system_bus = zbus::connection::Builder::system()
        .map_err(|e| format!("system bus builder failed: {e}"))?
        .build()
        .await
        .map_err(|e| format!("system bus connection failed: {e}"))?;

    let subject_dict: HashMap<&str, Value<'_>> = {
        let mut m = HashMap::new();
        m.insert("pid", Value::from(pid));
        m.insert("uid", Value::from(uid));
        m.insert("start-time", Value::from(0u64));
        m
    };

    let reply = system_bus
        .call_method(
            Some("org.freedesktop.PolicyKit1"),
            "/org/freedesktop/PolicyKit1/Authority",
            Some("org.freedesktop.PolicyKit1.Authority"),
            "CheckAuthorization",
            &(
                ("unix-process", subject_dict),
                POLKIT_ACTION_ID,
                HashMap::<&str, &str>::new(),
                1u32,
                "",
            ),
        )
        .await
        .map_err(|e| format!("CheckAuthorization call failed: {e}"))?;

    let body = reply.body();
    let result: (bool, bool, HashMap<String, String>) = body
        .deserialize()
        .map_err(|e| format!("CheckAuthorization deserialize failed: {e}"))?;

    if result.0 {
        tracing::debug!(caller_uid = uid, "polkit authorized");
    }
    Ok(result.0)
}

// ---------------------------------------------------------------------------
// Inventory interface (read-only hardware snapshot + friendly names)
// ---------------------------------------------------------------------------

struct InventoryIface {
    snapshot: Arc<RwLock<InventorySnapshot>>,
    config: Arc<RwLock<AppConfig>>,
}

#[interface(name = "org.kde.FanControl.Inventory")]
impl InventoryIface {
    async fn snapshot(&self) -> fdo::Result<String> {
        let snapshot = self.snapshot.read().await;
        serde_json::to_string(&*snapshot)
            .map_err(|e| fdo::Error::Failed(format!("serialization error: {e}")))
    }

    async fn set_sensor_name(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
        name: &str,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;
        if id.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("id exceeds 128 characters".into()));
        }
        if name.is_empty() {
            return Err(fdo::Error::InvalidArgs("name must not be empty".into()));
        }
        if name.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("name exceeds 128 characters".into()));
        }
        {
            let mut config = self.config.write().await;
            config.set_sensor_name(id, name.to_string());
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }

    async fn set_fan_name(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
        name: &str,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;
        if id.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("id exceeds 128 characters".into()));
        }
        if name.is_empty() {
            return Err(fdo::Error::InvalidArgs("name must not be empty".into()));
        }
        if name.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("name exceeds 128 characters".into()));
        }
        {
            let mut config = self.config.write().await;
            config.set_fan_name(id, name.to_string());
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }

    async fn remove_sensor_name(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;
        if id.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("id exceeds 128 characters".into()));
        }
        {
            let mut config = self.config.write().await;
            config.remove_sensor_name(id);
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }

    async fn remove_fan_name(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;
        if id.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("id exceeds 128 characters".into()));
        }
        {
            let mut config = self.config.write().await;
            config.remove_fan_name(id);
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }
}

impl InventoryIface {
    async fn apply_names_to_snapshot(&self) {
        let config = self.config.read().await;
        let mut snapshot = self.snapshot.write().await;
        let sensor_names: Vec<(String, String)> = config
            .friendly_names
            .sensors
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let fan_names: Vec<(String, String)> = config
            .friendly_names
            .fans
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        drop(config);

        for device in &mut snapshot.devices {
            for sensor in &mut device.temperatures {
                sensor.friendly_name = sensor_names
                    .iter()
                    .find(|(id, _)| id == &sensor.id)
                    .map(|(_, name)| name.clone());
            }
            for fan in &mut device.fans {
                fan.friendly_name = fan_names
                    .iter()
                    .find(|(id, _)| id == &fan.id)
                    .map(|(_, name)| name.clone());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Lifecycle interface (draft/apply, degraded state, events, runtime state)
// ---------------------------------------------------------------------------

struct LifecycleIface {
    config: Arc<RwLock<AppConfig>>,
    snapshot: Arc<RwLock<InventorySnapshot>>,
    degraded: Arc<RwLock<DegradedState>>,
    events: Arc<RwLock<LifecycleEventLog>>,
    owned: Arc<RwLock<OwnedFanSet>>,
    fallback_fan_ids: Arc<RwLock<HashSet<String>>>,
    control: ControlSupervisor,
}

fn release_removed_owned_fans(
    owned: &mut OwnedFanSet,
    next_owned: &std::collections::HashSet<String>,
) -> Vec<(String, String)> {
    let mut failures = Vec::new();
    for fan_id in owned
        .owned_fan_ids()
        .map(str::to_string)
        .collect::<Vec<_>>()
    {
        if !next_owned.contains(&fan_id) {
            match kde_fan_control_core::lifecycle::write_fallback_single(&fan_id, owned) {
                Ok(()) => owned.release_fan(&fan_id),
                Err(error) => failures.push((fan_id.clone(), error)),
            }
        }
    }
    failures
}

fn sync_panic_fallback_mirror_from_owned(mirror: &PanicFallbackMirror, owned: &OwnedFanSet) {
    let mut paths = Vec::new();
    for fan_id in owned.owned_fan_ids() {
        if let Some(path) = owned.sysfs_path(fan_id) {
            paths.push((fan_id.to_string(), path.to_string()));
        }
    }

    if let Ok(mut guard) = mirror.owned_pwm_paths.write() {
        *guard = paths;
    }
}

fn write_fallback_from_panic_mirror(
    mirror: &PanicFallbackMirror,
) -> (Vec<String>, Vec<(String, String)>) {
    let paths = match mirror.owned_pwm_paths.read() {
        Ok(guard) => guard.clone(),
        Err(_) => {
            return (
                Vec::new(),
                vec![(
                    "mirror".to_string(),
                    "poisoned panic fallback mirror".to_string(),
                )],
            );
        }
    };

    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for (fan_id, pwm_path) in paths {
        let pwm_enable_path = format!("{}_enable", pwm_path);
        if let Err(error) = std::fs::write(
            &pwm_enable_path,
            kde_fan_control_core::lifecycle::PWM_ENABLE_MANUAL.to_string(),
        ) {
            eprintln!(
                "panic fallback: could not set manual mode for {fan_id} at {pwm_enable_path}: {error}"
            );
        }

        match std::fs::write(
            &pwm_path,
            kde_fan_control_core::lifecycle::PWM_SAFE_MAX.to_string(),
        ) {
            Ok(()) => succeeded.push(fan_id),
            Err(error) => failed.push((fan_id, format!("pwm write failed: {error}"))),
        }
    }

    (succeeded, failed)
}

struct ControlIface {
    supervisor: ControlSupervisor,
    config: Arc<RwLock<AppConfig>>,
}

impl ControlIface {
    async fn accept_auto_tune_inner(&self, fan_id: &str) -> fdo::Result<String> {
        let proposal = self.supervisor.accepted_auto_tune_proposal(fan_id).await?;
        let mut config = self.config.write().await;

        let applied_entry = config
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get(fan_id))
            .cloned()
            .ok_or_else(|| {
                fdo::Error::Failed(format!(
                    "fan '{fan_id}' is not managed by the applied config"
                ))
            })?;

        let draft_entry = config
            .draft
            .fans
            .entry(fan_id.to_string())
            .or_insert_with(|| draft_entry_from_applied(&applied_entry));
        draft_entry.pid_gains = Some(proposal.proposed_gains);
        let response = serde_json::to_string(&*draft_entry)
            .map_err(|e| fdo::Error::Failed(format!("draft serialization error: {e}")))?;

        config
            .save()
            .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;

        Ok(response)
    }

    async fn set_draft_fan_control_profile_inner(
        &self,
        fan_id: &str,
        profile_json: &str,
    ) -> fdo::Result<String> {
        let patch: DraftFanControlProfilePayload = serde_json::from_str(profile_json)
            .map_err(|e| fdo::Error::Failed(format!("invalid control profile json: {e}")))?;

        let mut config = self.config.write().await;
        let applied_entry = config
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get(fan_id))
            .cloned();

        let draft_entry = if let Some(existing) = config.draft.fans.get_mut(fan_id) {
            existing
        } else {
            let applied_entry = applied_entry.ok_or_else(|| {
                fdo::Error::Failed(format!(
                    "fan '{fan_id}' has no existing draft or applied control profile"
                ))
            })?;
            config
                .draft
                .fans
                .entry(fan_id.to_string())
                .or_insert_with(|| draft_entry_from_applied(&applied_entry))
        };

        if let Some(value) = patch.temp_sources {
            draft_entry.temp_sources = value;
        }
        if let Some(value) = patch.target_temp_millidegrees {
            // Validate that target temperature is within sensible bounds.
            if let Some(target) = value {
                if target <= 0 || target > 150_000 {
                    return Err(fdo::Error::InvalidArgs(
                        format!("target_temp_millidegrees {target} is out of bounds (must be 1..=150000)").into(),
                    ));
                }
            }
            draft_entry.target_temp_millidegrees = value;
        }
        if let Some(value) = patch.aggregation {
            draft_entry.aggregation = value;
        }
        if let Some(value) = patch.pid_gains {
            // Validate that PID gains are finite (not NaN or Infinity).
            // serde_json deserializes JSON NaN/Infinity as actual f64 special values.
            if let Some(ref gains) = value {
                if !gains.is_finite() {
                    return Err(fdo::Error::InvalidArgs(
                        "pid_gains contains non-finite values (NaN or Infinity)".into(),
                    ));
                }
            }
            draft_entry.pid_gains = value;
        }
        if let Some(value) = patch.cadence {
            draft_entry.cadence = value;
        }
        if let Some(value) = patch.deadband_millidegrees {
            draft_entry.deadband_millidegrees = value;
        }
        if let Some(value) = patch.actuator_policy {
            draft_entry.actuator_policy = value;
        }
        if let Some(value) = patch.pid_limits {
            // Validate that PID limits are finite (not NaN or Infinity).
            if let Some(ref limits) = value {
                if !limits.is_finite() {
                    return Err(fdo::Error::InvalidArgs(
                        "pid_limits contains non-finite values (NaN or Infinity)".into(),
                    ));
                }
            }
            draft_entry.pid_limits = value;
        }
        let response = serde_json::to_string(&*draft_entry)
            .map_err(|e| fdo::Error::Failed(format!("draft serialization error: {e}")))?;

        config
            .save()
            .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;

        Ok(response)
    }

    #[allow(dead_code)]
    async fn accept_auto_tune_for_test(
        &self,
        fan_id: &str,
        authorized: bool,
    ) -> fdo::Result<String> {
        require_test_authorized(authorized)?;
        self.accept_auto_tune_inner(fan_id).await
    }

    #[allow(dead_code)]
    async fn set_draft_fan_control_profile_for_test(
        &self,
        fan_id: &str,
        profile_json: &str,
        authorized: bool,
    ) -> fdo::Result<String> {
        require_test_authorized(authorized)?;
        self.set_draft_fan_control_profile_inner(fan_id, profile_json)
            .await
    }
}

#[interface(name = "org.kde.FanControl.Control")]
impl ControlIface {
    async fn get_control_status(&self) -> fdo::Result<String> {
        self.supervisor
            .status_json()
            .await
            .map_err(|e| fdo::Error::Failed(format!("control status serialization error: {e}")))
    }

    async fn get_auto_tune_result(&self, fan_id: String) -> fdo::Result<String> {
        self.supervisor
            .auto_tune_result_json(&fan_id)
            .await
            .map_err(|e| fdo::Error::Failed(format!("auto-tune result serialization error: {e}")))
    }

    async fn start_auto_tune(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        fan_id: String,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;
        self.supervisor.start_auto_tune(&fan_id).await
    }

    async fn accept_auto_tune(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        fan_id: String,
    ) -> fdo::Result<String> {
        require_authorized(connection, &header).await?;
        let updated = self.accept_auto_tune_inner(&fan_id).await?;
        emit_draft_changed(connection).await;
        Ok(updated)
    }

    async fn set_draft_fan_control_profile(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        fan_id: String,
        profile_json: String,
    ) -> fdo::Result<String> {
        require_authorized(connection, &header).await?;
        let updated = self
            .set_draft_fan_control_profile_inner(&fan_id, &profile_json)
            .await?;
        emit_draft_changed(connection).await;
        Ok(updated)
    }

    #[zbus(signal)]
    async fn control_status_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal, name = "AutoTuneCompleted")]
    async fn auto_tune_completed(emitter: &SignalEmitter<'_>, fan_id: &str) -> zbus::Result<()>;
}

#[interface(name = "org.kde.FanControl.Lifecycle")]
impl LifecycleIface {
    // -------------------------------------------------------------------
    // Read methods (accessible to all local users)
    // -------------------------------------------------------------------

    /// Return the current draft configuration as a JSON string.
    async fn get_draft_config(&self) -> fdo::Result<String> {
        let config = self.config.read().await;
        serde_json::to_string(&config.draft)
            .map_err(|e| fdo::Error::Failed(format!("draft serialization error: {e}")))
    }

    /// Return the current applied configuration as a JSON string.
    /// Returns "null" if no configuration has been applied yet.
    async fn get_applied_config(&self) -> fdo::Result<String> {
        let config = self.config.read().await;
        serde_json::to_string(&config.applied)
            .map_err(|e| fdo::Error::Failed(format!("applied serialization error: {e}")))
    }

    /// Return the current degraded-state summary as a JSON string.
    async fn get_degraded_summary(&self) -> fdo::Result<String> {
        let degraded = self.degraded.read().await;
        serde_json::to_string(&*degraded)
            .map_err(|e| fdo::Error::Failed(format!("degraded serialization error: {e}")))
    }

    /// Return recent lifecycle events as a JSON string.
    async fn get_lifecycle_events(&self) -> fdo::Result<String> {
        let events = self.events.read().await;
        serde_json::to_string(events.events())
            .map_err(|e| fdo::Error::Failed(format!("events serialization error: {e}")))
    }

    /// Return the current runtime state as a JSON string.
    /// Shows which fans are managed, degraded, in fallback, or unmanaged.
    async fn get_runtime_state(&self) -> fdo::Result<String> {
        let fallback_guard = self.fallback_fan_ids.read().await.clone();
        let state = self.control.runtime_state_snapshot(&fallback_guard).await;

        serde_json::to_string(&state)
            .map_err(|e| fdo::Error::Failed(format!("runtime state serialization error: {e}")))
    }

    /// Proactively check whether the caller is authorized for privileged
    /// operations. This triggers a polkit authentication dialog if the
    /// caller is not yet authorized, allowing the GUI to present an
    /// "Unlock" button that obtains credentials before performing writes.
    async fn request_authorization(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await
    }

    // -------------------------------------------------------------------
    // Write methods (require privileged authorization)
    // -------------------------------------------------------------------

    /// Stage a fan enrollment change in the draft configuration.
    /// The caller must be privileged (UID 0).
    /// Changes are not live until explicitly applied via ApplyDraft.
    async fn set_draft_fan_enrollment(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        fan_id: String,
        managed: bool,
        control_mode: String,
        temp_sources: Vec<String>,
    ) -> fdo::Result<String> {
        require_authorized(connection, &header).await?;

        let parsed_mode = parse_control_mode(&control_mode)?;

        let entry = DraftFanEntry {
            managed,
            control_mode: parsed_mode,
            temp_sources,
            target_temp_millidegrees: None,
            aggregation: None,
            pid_gains: None,
            cadence: None,
            deadband_millidegrees: None,
            actuator_policy: None,
            pid_limits: None,
        };

        {
            let mut config = self.config.write().await;
            config.set_draft_fan(&fan_id, entry);
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }

        if let Err(e) = emitter.draft_changed().await {
            tracing::warn!(error = %e, "failed to emit DraftChanged signal");
        }

        // Return the updated draft for confirmation.
        let config = self.config.read().await;
        serde_json::to_string(&config.draft)
            .map_err(|e| fdo::Error::Failed(format!("draft serialization error: {e}")))
    }

    /// Remove a fan from the draft configuration.
    async fn remove_draft_fan(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        fan_id: String,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;

        {
            let mut config = self.config.write().await;
            config.remove_draft_fan(&fan_id);
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }

        if let Err(e) = emitter.draft_changed().await {
            tracing::warn!(error = %e, "failed to emit DraftChanged signal");
        }

        Ok(())
    }

    /// Discard the entire draft configuration.
    async fn discard_draft(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;

        {
            let mut config = self.config.write().await;
            config.draft.fans.clear();
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }

        if let Err(e) = emitter.draft_changed().await {
            tracing::warn!(error = %e, "failed to emit DraftChanged signal");
        }

        Ok(())
    }

    /// Validate the current draft against live inventory and return
    /// a ValidationResult as a JSON string. Does not modify any state.
    async fn validate_draft(&self) -> fdo::Result<String> {
        let (draft, snapshot) = {
            let config = self.config.read().await;
            let snapshot = self.snapshot.read().await;
            (config.draft.clone(), snapshot.clone())
        };

        let result = validate_draft(&draft, &snapshot);
        serde_json::to_string(&result)
            .map_err(|e| fdo::Error::Failed(format!("validation serialization error: {e}")))
    }

    /// Apply the current draft configuration.
    /// Validates the draft against live inventory, promotes passing fans
    /// to applied config, claims them in the owned set, and reports any
    /// rejected fans. Emits appropriate signals on completion.
    async fn apply_draft(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        require_authorized(connection, &header).await?;

        let (applied, result) = {
            let (draft, snapshot, previous_applied) = {
                let config = self.config.read().await;
                let snapshot = self.snapshot.read().await;
                (
                    config.draft.clone(),
                    snapshot.clone(),
                    config.applied.clone(),
                )
            };
            let timestamp = format_iso8601_now();
            apply_draft(&draft, &snapshot, timestamp, previous_applied.as_ref())
        };

        let mut had_rejections = !result.rejected.is_empty();

        // Persist first. This is the commit point: runtime ownership and task
        // orchestration changes happen only after the new applied config is
        // durably saved.
        {
            let mut config = self.config.write().await;
            let previous_applied = config.applied.clone();
            config.set_applied(applied.clone());
            config.clear_fallback_incident();
            if let Err(error) = config.save() {
                config.applied = previous_applied;
                return Err(fdo::Error::Failed(format!("config save error: {error}")));
            }
        }
        self.fallback_fan_ids.write().await.clear();

        self.control.stop_all().await;

        // Update degraded state for any rejected fans.
        {
            let mut degraded = self.degraded.write().await;
            for (fan_id, error) in &result.rejected {
                degraded.mark_degraded(
                    fan_id.clone(),
                    vec![validation_error_to_degraded_reason(error)],
                );
            }
        }

        // Record lifecycle events for any rejections.
        {
            let mut events = self.events.write().await;
            for (fan_id, error) in &result.rejected {
                events.push(LifecycleEvent {
                    timestamp: format_iso8601_now(),
                    reason: validation_error_to_degraded_reason(error),
                    detail: Some(format!("draft apply rejected fan {fan_id}: {error}")),
                });
            }
            if !result.enrollable.is_empty() && had_rejections {
                events.push(LifecycleEvent {
                    timestamp: format_iso8601_now(),
                    reason: DegradedReason::PartialBootRecovery {
                        failed_count: result.rejected.len() as u32,
                        recovered_count: result.enrollable.len() as u32,
                    },
                    detail: Some("partial apply during draft promotion".into()),
                });
            }
        }

        // Claim successfully applied fans in the owned set and release any
        // previously owned fans that are no longer part of the newly applied set.
        {
            let snapshot = self.snapshot.read().await;
            let mut owned = self.owned.write().await;
            let next_owned: std::collections::HashSet<_> =
                result.enrollable.iter().cloned().collect();
            let release_failures = release_removed_owned_fans(&mut owned, &next_owned);
            if !release_failures.is_empty() {
                had_rejections = true;
                let mut degraded = self.degraded.write().await;
                let mut events = self.events.write().await;
                for (fan_id, error) in release_failures {
                    let reason = DegradedReason::FanNoLongerEnrollable {
                        fan_id: fan_id.clone(),
                        support_state: kde_fan_control_core::inventory::SupportState::Unavailable,
                        reason: format!("failed to release fan safely: {error}"),
                    };
                    degraded.mark_degraded(fan_id.clone(), vec![reason.clone()]);
                    events.push(LifecycleEvent {
                        timestamp: format_iso8601_now(),
                        reason,
                        detail: Some(format!(
                            "apply draft could not release {fan_id} safely; ownership retained"
                        )),
                    });
                }
            }
            for fan_id in &result.enrollable {
                // Find the fan's sysfs path from the current inventory.
                let fan = snapshot
                    .devices
                    .iter()
                    .flat_map(|d| d.fans.iter())
                    .find(|f| f.id == *fan_id);
                if let Some(fan) = fan {
                    let device = snapshot
                        .devices
                        .iter()
                        .find(|d| d.fans.iter().any(|f| f.id == *fan_id));
                    let sysfs_path = device
                        .map(|d| format!("{}/pwm{}", d.sysfs_path, fan.channel))
                        .unwrap_or_default();

                    // Find the applied entry to get the control mode.
                    if let Some(applied_entry) = applied.fans.get(fan_id) {
                        owned.claim_fan(fan_id, applied_entry.control_mode, &sysfs_path);
                        // Sync the panic fallback mirror after EACH claim so that the
                        // panic hook always sees a complete set of owned-fan PWM paths.
                        // Without this, a panic between claims would leave newly claimed
                        // fans invisible to the hook, bypassing PWM-255 fallback.
                        sync_panic_fallback_mirror_from_owned(&self.control.panic_fallback_mirror(), &owned);
                    }
                }
                // Clear degraded state for this fan.
                self.degraded.write().await.clear_fan(fan_id);
            }

            persist_owned_fans(&owned);
        }

        self.control.reconcile().await;

        // Emit signals.
        if let Err(e) = emitter.draft_changed().await {
            tracing::warn!(error = %e, "failed to emit DraftChanged signal");
        }
        if let Err(e) = emitter.applied_config_changed().await {
            tracing::warn!(error = %e, "failed to emit AppliedConfigChanged signal");
        }
        if had_rejections {
            if let Err(e) = emitter.degraded_state_changed().await {
                tracing::warn!(error = %e, "failed to emit DegradedStateChanged signal");
            }
        }
        if let Err(e) = emitter
            .lifecycle_event_appended(
                "apply_draft",
                &format!(
                    "{} fans promoted, {} rejected",
                    result.enrollable.len(),
                    result.rejected.len()
                ),
            )
            .await
        {
            tracing::warn!(error = %e, "failed to emit LifecycleEventAppended signal");
        }
        emit_control_status_changed(connection).await;

        serde_json::to_string(&result)
            .map_err(|e| fdo::Error::Failed(format!("validation serialization error: {e}")))
    }

    // -------------------------------------------------------------------
    // Signals
    // -------------------------------------------------------------------
    // Overview-specific read methods (split structure + telemetry)

    /// Return the overview structure snapshot as JSON.
    async fn get_overview_structure(&self) -> fdo::Result<String> {
        let (snapshot_guard, config_guard) = {
            let snapshot = self.snapshot.read().await;
            let config = self.config.read().await;
            (snapshot.clone(), config.clone())
        };
        let fallback_guard = self.fallback_fan_ids.read().await.clone();

        let runtime = self.control.runtime_state_snapshot(&fallback_guard).await;

        let structure = OverviewStructureSnapshot::build(&snapshot_guard, &runtime, &config_guard);
        serde_json::to_string(&structure)
            .map_err(|e| fdo::Error::Failed(format!("overview structure serialization error: {e}")))
    }

    async fn get_overview_telemetry(&self) -> fdo::Result<String> {
        let snapshot_guard = self.snapshot.read().await.clone();
        let fallback_guard = self.fallback_fan_ids.read().await.clone();
        let runtime = self.control.runtime_state_snapshot(&fallback_guard).await;

        let telemetry = OverviewTelemetryBatch::build(&snapshot_guard, &runtime);
        serde_json::to_string(&telemetry)
            .map_err(|e| fdo::Error::Failed(format!("overview telemetry serialization error: {e}")))
    }

    // -------------------------------------------------------------------
    // Signals
    // -------------------------------------------------------------------

    /// Emitted when the draft configuration changes.
    #[zbus(signal)]
    async fn draft_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Emitted when the applied configuration changes.
    #[zbus(signal)]
    async fn applied_config_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Emitted when the degraded-state summary changes.
    #[zbus(signal)]
    async fn degraded_state_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    /// Emitted when a lifecycle event is appended to the history.
    #[zbus(signal)]
    async fn lifecycle_event_appended(
        emitter: &SignalEmitter<'_>,
        event_kind: &str,
        detail: &str,
    ) -> zbus::Result<()>;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a control mode string into a ControlMode enum value.
/// Returns an error if the string is not a recognized control mode.
fn parse_control_mode(mode: &str) -> fdo::Result<Option<ControlMode>> {
    match mode {
        "" | "none" => Ok(None),
        "pwm" => Ok(Some(ControlMode::Pwm)),
        "voltage" => Ok(Some(ControlMode::Voltage)),
        _ => Err(fdo::Error::Failed(format!(
            "unknown control mode '{mode}'; expected 'pwm', 'voltage', or empty"
        ))),
    }
}

/// Map a ValidationError to a DegradedReason for degraded-state tracking.
fn validation_error_to_degraded_reason(
    error: &kde_fan_control_core::config::ValidationError,
) -> DegradedReason {
    match error {
        kde_fan_control_core::config::ValidationError::FanNotFound { fan_id } => {
            DegradedReason::FanMissing {
                fan_id: fan_id.clone(),
            }
        }
        kde_fan_control_core::config::ValidationError::FanNotEnrollable {
            fan_id,
            support_state,
            reason,
        } => DegradedReason::FanNoLongerEnrollable {
            fan_id: fan_id.clone(),
            support_state: *support_state,
            reason: reason.clone(),
        },
        kde_fan_control_core::config::ValidationError::UnsupportedControlMode {
            fan_id,
            requested,
            ..
        } => DegradedReason::ControlModeUnavailable {
            fan_id: fan_id.clone(),
            mode: *requested,
        },
        kde_fan_control_core::config::ValidationError::MissingControlMode { fan_id } => {
            DegradedReason::FanNoLongerEnrollable {
                fan_id: fan_id.clone(),
                support_state: kde_fan_control_core::inventory::SupportState::Unavailable,
                reason: "no control mode selected".into(),
            }
        }
        kde_fan_control_core::config::ValidationError::TempSourceNotFound { fan_id, temp_id } => {
            DegradedReason::TempSourceMissing {
                fan_id: fan_id.clone(),
                temp_id: temp_id.clone(),
            }
        }
        kde_fan_control_core::config::ValidationError::MissingTargetTemp { fan_id }
        | kde_fan_control_core::config::ValidationError::NoSensorForManagedFan { fan_id }
        | kde_fan_control_core::config::ValidationError::InvalidCadence { fan_id, .. }
        | kde_fan_control_core::config::ValidationError::InvalidActuatorPolicy { fan_id, .. }
        | kde_fan_control_core::config::ValidationError::InvalidPidLimits { fan_id, .. }
        | kde_fan_control_core::config::ValidationError::InvalidPidGains { fan_id, .. }
        | kde_fan_control_core::config::ValidationError::InvalidTargetTemperature { fan_id, .. } => {
            DegradedReason::FanNoLongerEnrollable {
                fan_id: fan_id.clone(),
                support_state: kde_fan_control_core::inventory::SupportState::Unavailable,
                reason: error.to_string(),
            }
        }
    }
}

async fn emit_control_status_changed(connection: &zbus::Connection) {
    match connection
        .object_server()
        .interface::<_, ControlIface>(BUS_PATH_CONTROL)
        .await
    {
        Ok(iface_ref) => {
            if let Err(error) = iface_ref.control_status_changed().await {
                tracing::warn!(error = %error, "failed to emit ControlStatusChanged signal");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to access ControlIface for signal emission");
        }
    }
}

async fn wait_for_shutdown_signal() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        let mut sigterm = signal(SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = sigterm.recv() => {}
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}

async fn emit_draft_changed(connection: &zbus::Connection) {
    match connection
        .object_server()
        .interface::<_, LifecycleIface>(BUS_PATH_LIFECYCLE)
        .await
    {
        Ok(iface_ref) => {
            if let Err(error) = iface_ref.draft_changed().await {
                tracing::warn!(error = %error, "failed to emit DraftChanged signal");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to access LifecycleIface for DraftChanged emission");
        }
    }
}

async fn emit_auto_tune_completed(connection: &zbus::Connection, fan_id: &str) {
    match connection
        .object_server()
        .interface::<_, ControlIface>(BUS_PATH_CONTROL)
        .await
    {
        Ok(iface_ref) => {
            if let Err(error) = iface_ref.auto_tune_completed(fan_id).await {
                tracing::warn!(error = %error, fan_id = %fan_id, "failed to emit AutoTuneCompleted signal");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, fan_id = %fan_id, "failed to access ControlIface for AutoTuneCompleted emission");
        }
    }
}

async fn emit_degraded_state_changed(connection: &zbus::Connection) {
    match connection
        .object_server()
        .interface::<_, LifecycleIface>(BUS_PATH_LIFECYCLE)
        .await
    {
        Ok(iface_ref) => {
            if let Err(error) = iface_ref.degraded_state_changed().await {
                tracing::warn!(error = %error, "failed to emit DegradedStateChanged signal");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to access LifecycleIface for DegradedStateChanged emission");
        }
    }
}

async fn emit_applied_config_changed(connection: &zbus::Connection) {
    match connection
        .object_server()
        .interface::<_, LifecycleIface>(BUS_PATH_LIFECYCLE)
        .await
    {
        Ok(iface_ref) => {
            if let Err(error) = iface_ref.applied_config_changed().await {
                tracing::warn!(error = %error, "failed to emit AppliedConfigChanged signal");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to access LifecycleIface for AppliedConfigChanged emission");
        }
    }
}

async fn emit_lifecycle_event_appended(connection: &zbus::Connection, event_kind: &str, detail: &str) {
    match connection
        .object_server()
        .interface::<_, LifecycleIface>(BUS_PATH_LIFECYCLE)
        .await
    {
        Ok(iface_ref) => {
            if let Err(error) = iface_ref.lifecycle_event_appended(event_kind, detail).await {
                tracing::warn!(error = %error, "failed to emit LifecycleEventAppended signal");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to access LifecycleIface for LifecycleEventAppended emission");
        }
    }
}

fn record_fallback_incident_for_owned(
    owned: &OwnedFanSet,
    config: &mut AppConfig,
    events: &mut LifecycleEventLog,
    fallback_fan_ids: &mut HashSet<String>,
    trigger: String,
) -> FallbackResult {
    let result = write_fallback_for_owned(owned);
    let timestamp = format_iso8601_now();
    let detail = Some(format!(
        "{trigger}; {} write(s) succeeded, {} failed",
        result.succeeded.len(),
        result.failed.len()
    ));
    let incident = kde_fan_control_core::config::FallbackIncident::from_owned_and_result(
        timestamp, owned, &result, detail,
    );

    fallback_fan_ids.clear();
    fallback_fan_ids.extend(incident.fallback_fan_ids());

    if incident.affected_fans.is_empty() {
        config.clear_fallback_incident();
        return result;
    }

    events.push(lifecycle_event_from_fallback_incident(&incident));
    config.set_fallback_incident(incident);

    result
}

async fn run_fallback_recorder(
    owned: &Arc<RwLock<OwnedFanSet>>,
    config: &Arc<RwLock<AppConfig>>,
    events: &Arc<RwLock<LifecycleEventLog>>,
    fallback_fan_ids: &Arc<RwLock<HashSet<String>>>,
    trigger: String,
) -> FallbackResult {
    let owned_guard = owned.read().await;
    let mut config_guard = config.write().await;
    let mut events_guard = events.write().await;
    let mut fallback_guard = fallback_fan_ids.write().await;

    let result = record_fallback_incident_for_owned(
        &owned_guard,
        &mut config_guard,
        &mut events_guard,
        &mut fallback_guard,
        trigger,
    );

    if let Err(error) = config_guard.save() {
        tracing::error!(error = %error, "failed to persist fallback incident");
    }

    result
}

fn run_panic_fallback_recorder(
    owned: &Arc<RwLock<OwnedFanSet>>,
    config: &Arc<RwLock<AppConfig>>,
    events: &Arc<RwLock<LifecycleEventLog>>,
    fallback_fan_ids: &Arc<RwLock<HashSet<String>>>,
    trigger: String,
) -> bool {
    let Ok(owned_guard) = owned.try_read() else {
        eprintln!("panic fallback skipped: owned-fan state lock unavailable");
        return false;
    };
    let Ok(mut config_guard) = config.try_write() else {
        eprintln!("panic fallback skipped: config lock unavailable");
        return false;
    };
    let Ok(mut events_guard) = events.try_write() else {
        eprintln!("panic fallback skipped: lifecycle-event lock unavailable");
        return false;
    };
    let Ok(mut fallback_guard) = fallback_fan_ids.try_write() else {
        eprintln!("panic fallback skipped: fallback-state lock unavailable");
        return false;
    };

    let result = record_fallback_incident_for_owned(
        &owned_guard,
        &mut config_guard,
        &mut events_guard,
        &mut fallback_guard,
        trigger,
    );

    if let Err(error) = config_guard.save() {
        eprintln!("panic fallback save failed: {error}");
    }

    !result.succeeded.is_empty() || !result.failed.is_empty()
}

fn install_panic_fallback_hook(
    owned: Arc<RwLock<OwnedFanSet>>,
    config: Arc<RwLock<AppConfig>>,
    events: Arc<RwLock<LifecycleEventLog>>,
    fallback_fan_ids: Arc<RwLock<HashSet<String>>>,
    panic_mirror: Arc<PanicFallbackMirror>,
) {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let trigger = if let Some(message) = panic_info.payload().downcast_ref::<&str>() {
            format!("panic hook triggered fallback: {message}")
        } else if let Some(message) = panic_info.payload().downcast_ref::<String>() {
            format!("panic hook triggered fallback: {message}")
        } else {
            "panic hook triggered fallback".to_string()
        };

        let (succeeded, failed) = write_fallback_from_panic_mirror(&panic_mirror);
        if !succeeded.is_empty() {
            eprintln!(
                "panic fallback wrote safe maximum for fans: {:?}",
                succeeded
            );
        }
        if !failed.is_empty() {
            eprintln!("panic fallback failed for fans: {:?}", failed);
        }

        let _ = run_panic_fallback_recorder(&owned, &config, &events, &fallback_fan_ids, trigger);
        previous_hook(panic_info);
    }));
}

/// Return the current time as an ISO 8601 string (UTC).
fn format_iso8601_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple UTC timestamp: YYYY-MM-DDThh:mm:ssZ
    // Calculate from unix epoch without external crate dependency.
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year, month, day from days since epoch.
    // Algorithm based on Howard Hinnant's civil_from_days.
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
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = DaemonArgs::parse();

    let mut initial = match args.root {
        Some(ref root) => discover_from(root)?,
        None => discover()?,
    };

    let config = match AppConfig::load() {
        Ok(c) => {
            tracing::info!("loaded configuration");
            c
        }
        Err(e) => {
            tracing::warn!(error = %e, "could not load configuration, using defaults");
            AppConfig::default()
        }
    };

    let persisted_fallback_fan_ids = config
        .fallback_incident
        .as_ref()
        .map(|incident| incident.fallback_fan_ids())
        .unwrap_or_default();

    for device in &mut initial.devices {
        for sensor in &mut device.temperatures {
            sensor.friendly_name = config.sensor_name(&sensor.id).map(|n| n.to_string());
        }
        for fan in &mut device.fans {
            fan.friendly_name = config.fan_name(&fan.id).map(|n| n.to_string());
        }
    }

    // Shared state.
    let snapshot = Arc::new(RwLock::new(initial));
    let config = Arc::new(RwLock::new(config));
    let degraded = Arc::new(RwLock::new(DegradedState::new()));
    let events = Arc::new(RwLock::new(LifecycleEventLog::new()));
    let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
    let fallback_fan_ids = Arc::new(RwLock::new(persisted_fallback_fan_ids));
    let control = ControlSupervisor::new(
        Arc::clone(&snapshot),
        Arc::clone(&config),
        Arc::clone(&owned),
        Arc::clone(&degraded),
    );
    let panic_mirror = control.panic_fallback_mirror();
    control.sync_panic_fallback_mirror().await;

    install_panic_fallback_hook(
        Arc::clone(&owned),
        Arc::clone(&config),
        Arc::clone(&events),
        Arc::clone(&fallback_fan_ids),
        panic_mirror,
    );

    {
        let config_guard = config.read().await;
        if let Some(incident) = config_guard.fallback_incident.as_ref() {
            events
                .write()
                .await
                .push(lifecycle_event_from_fallback_incident(incident));
        }
    }

    // -----------------------------------------------------------------------
    // Boot reconciliation: restore managed fans from applied config
    // -----------------------------------------------------------------------
    {
        let config_guard = config.read().await;
        let snapshot_guard = snapshot.read().await;
        let mut owned_guard = owned.write().await;
        let mut degraded_guard = degraded.write().await;
        let mut events_guard = events.write().await;

        let result = perform_boot_reconciliation(
            config_guard.applied.as_ref(),
            &snapshot_guard,
            &mut owned_guard,
            &mut degraded_guard,
            &mut events_guard,
        );

        tracing::info!(
            restored = result.restored.len(),
            skipped = result.skipped.len(),
            "boot reconciliation complete"
        );

        for outcome in &result.restored {
            if let kde_fan_control_core::lifecycle::ReconcileOutcome::Restored { fan_id, .. } =
                outcome
            {
                tracing::info!(fan_id = %fan_id, "restored managed fan on boot");
            }
        }

        for outcome in &result.skipped {
            tracing::warn!(outcome = ?outcome, "skipped fan during boot reconciliation");
        }

        // If any fans were restored successfully, replace the applied config
        // with the reconciled subset and persist it. A successful boot restore
        // supersedes any previously persisted fallback incident: the fallback
        // remains visible in lifecycle history, but the current runtime state
        // should return to managed rather than stay latched in FALLBACK.
        if !result.restored.is_empty() {
            let reconciled_config = result.reconciled_config.clone();
            drop(config_guard);
            let mut config_mut = config.write().await;
            config_mut.set_applied(reconciled_config);
            config_mut.clear_fallback_incident();
            if let Err(e) = config_mut.save() {
                tracing::error!(error = %e, "failed to persist reconciled config after boot");
            } else {
                tracing::info!("persisted reconciled applied config");
            }
            drop(config_mut);

            fallback_fan_ids.write().await.clear();
        }
    }

    control.sync_panic_fallback_mirror().await;

    control.reconcile().await;

    {
        let owned_guard = owned.read().await;
        if !owned_guard.is_empty() {
            persist_owned_fans(&owned_guard);
        }
    }

    let inventory_iface = InventoryIface {
        snapshot: Arc::clone(&snapshot),
        config: Arc::clone(&config),
    };

    let lifecycle_iface = LifecycleIface {
        config: Arc::clone(&config),
        snapshot: Arc::clone(&snapshot),
        degraded: Arc::clone(&degraded),
        events: Arc::clone(&events),
        owned: Arc::clone(&owned),
        fallback_fan_ids: Arc::clone(&fallback_fan_ids),
        control: control.clone(),
    };

    let control_iface = ControlIface {
        supervisor: control.clone(),
        config: Arc::clone(&config),
    };

    let builder = if args.session_bus {
        Builder::session()?
    } else {
        Builder::system()?
    };

    let _connection = builder
        .name(BUS_NAME)?
        .serve_at(BUS_PATH_INVENTORY, inventory_iface)?
        .serve_at(BUS_PATH_LIFECYCLE, lifecycle_iface)?
        .serve_at(BUS_PATH_CONTROL, control_iface)?
        .build()
        .await?;

    control.set_signal_connection(_connection.clone()).await;

    tracing::info!(
        name = BUS_NAME,
        "D-Bus inventory and lifecycle surfaces ready"
    );

    let _ = sd_notify::notify(&[NotifyState::Ready]);

    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(20));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let _ = sd_notify::notify(&[NotifyState::Watchdog]);
        }
    });

    let rpm_snapshot = Arc::clone(&snapshot);
    tokio::spawn(async move {
        let mut tick = interval(Duration::from_secs(2));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let paths = {
                let snapshot = rpm_snapshot.read().await;
                snapshot
                    .devices
                    .iter()
                    .flat_map(|device| {
                        device
                            .fans
                            .iter()
                            .filter(|fan| fan.rpm_feedback)
                            .map(|fan| {
                                (
                                    fan.id.clone(),
                                    PathBuf::from(&device.sysfs_path)
                                        .join(format!("fan{}_input", fan.channel)),
                                )
                            })
                    })
                    .collect::<Vec<_>>()
            };
            let updates: Vec<(String, u64)> = paths
                .iter()
                .filter_map(|(fan_id, path)| {
                    fs::read_to_string(path)
                        .ok()
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .map(|rpm| (fan_id.clone(), rpm))
                })
                .collect();
            if !updates.is_empty() {
                let mut snapshot = rpm_snapshot.write().await;
                for (fan_id, rpm) in updates {
                    snapshot.update_fan_rpm(&fan_id, Some(rpm));
                }
            }
        }
    });

    let reassess_control = control.clone();
    let reassess_events = Arc::clone(&events);
    let reassess_config = Arc::clone(&config);
    tokio::spawn(async move {
        let interval_ms = reassess_config
            .read()
            .await
            .reassess_degraded_interval_ms
            .max(1000);
        let mut tick = interval(Duration::from_millis(interval_ms));
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            reassess_control.reassess_degraded_fans(&reassess_events).await;
        }
    });

    // Wait for shutdown signal, stop control loops, then drive fallback for
    // any remaining owned fans.
    wait_for_shutdown_signal().await?;
    let _ = sd_notify::notify(&[NotifyState::Stopping]);
    tracing::info!("shutting down — driving owned fans to safe maximum");

    control.stop_all().await;

    // -----------------------------------------------------------------------
    // Fallback: drive all owned fans to safe maximum before exit
    // -----------------------------------------------------------------------
    let fallback_result: FallbackResult = run_fallback_recorder(
        &owned,
        &config,
        &events,
        &fallback_fan_ids,
        "ctrl-c shutdown".to_string(),
    )
    .await;

    if !fallback_result.succeeded.is_empty() {
        tracing::info!(
            fans = ?fallback_result.succeeded,
            "fallback: set fans to safe maximum"
        );
    }
    if !fallback_result.failed.is_empty() {
        tracing::error!(
            failures = ?fallback_result.failed,
            "fallback: some fans could NOT be set to safe maximum"
        );
    }

    tracing::info!("shutdown complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kde_fan_control_core::config::LifecycleEventLog;
    use kde_fan_control_core::config::{AppliedConfig, AppliedFanEntry, DegradedState};
    use kde_fan_control_core::control::{
        ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits,
    };
    use kde_fan_control_core::inventory::{HwmonDevice, TemperatureSensor};
    use kde_fan_control_core::lifecycle::OwnedFanSet;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn applied_entry(temp_sources: Vec<String>) -> AppliedFanEntry {
        AppliedFanEntry {
            control_mode: ControlMode::Pwm,
            temp_sources,
            target_temp_millidegrees: 50_000,
            aggregation: AggregationFn::Average,
            pid_gains: PidGains {
                kp: 1.0,
                ki: 0.0,
                kd: 0.0,
            },
            cadence: ControlCadence {
                sample_interval_ms: 20,
                control_interval_ms: 20,
                write_interval_ms: 20,
            },
            deadband_millidegrees: 0,
            actuator_policy: ActuatorPolicy {
                output_min_percent: 0.0,
                output_max_percent: 100.0,
                pwm_min: 0,
                pwm_max: 255,
                startup_kick_percent: 35.0,
                startup_kick_ms: 1,
            },
            pid_limits: PidLimits::default(),
        }
    }

    fn applied_config_for(fan_id: &str, temp_id: &str) -> AppliedConfig {
        AppliedConfig {
            fans: HashMap::from([(fan_id.to_string(), applied_entry(vec![temp_id.to_string()]))]),
            applied_at: Some("2026-04-11T12:00:00Z".to_string()),
        }
    }

    fn test_snapshot(root: &Path) -> InventorySnapshot {
        InventorySnapshot {
            devices: vec![HwmonDevice {
                id: "hwmon-test-0000000000000001".to_string(),
                name: "testchip".to_string(),
                sysfs_path: root.display().to_string(),
                stable_identity: "/sys/devices/platform/testchip".to_string(),
                temperatures: vec![TemperatureSensor {
                    id: "hwmon-test-0000000000000001-temp1".to_string(),
                    channel: 1,
                    label: Some("CPU".to_string()),
                    friendly_name: None,
                    input_millidegrees_celsius: Some(55_000),
                }],
                fans: vec![kde_fan_control_core::inventory::FanChannel {
                    id: "hwmon-test-0000000000000001-fan1".to_string(),
                    channel: 1,
                    label: Some("CPU Fan".to_string()),
                    friendly_name: None,
                    rpm_feedback: true,
                    current_rpm: Some(1200),
                    control_modes: vec![ControlMode::Pwm],
                    support_state: kde_fan_control_core::inventory::SupportState::Available,
                    support_reason: None,
                }],
            }],
        }
    }

    struct ControlFixture {
        root: PathBuf,
    }

    impl ControlFixture {
        fn new() -> Self {
            let unique = format!(
                "kde-fan-control-daemon-control-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock should be after epoch")
                    .as_nanos(),
                TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed)
            );
            let root = std::env::temp_dir().join(unique);
            fs::create_dir_all(&root).expect("fixture root should be created");
            Self { root }
        }

        fn root(&self) -> &Path {
            &self.root
        }

        fn write_temp(&self, value: &str) {
            fs::write(self.root.join("temp1_input"), value).expect("temp input should be written");
        }

        fn write_pwm_seed(&self, value: &str) {
            fs::write(self.root.join("pwm1"), value).expect("pwm file should be written");
        }

        fn pwm_path(&self) -> PathBuf {
            self.root.join("pwm1")
        }
    }

    impl Drop for ControlFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn shared_fallback_recorder_persists_incident_for_graceful_shutdown() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan("fan-1", ControlMode::Pwm, "/definitely/missing/pwm1");
        let mut config = AppConfig::default();
        let mut events = LifecycleEventLog::new();
        let mut fallback_fan_ids = HashSet::new();

        let result = record_fallback_incident_for_owned(
            &owned,
            &mut config,
            &mut events,
            &mut fallback_fan_ids,
            "ctrl-c shutdown".to_string(),
        );

        assert_eq!(result.failed.len(), 1);
        let incident = config
            .fallback_incident
            .as_ref()
            .expect("fallback incident");
        assert_eq!(incident.affected_fans, vec!["fan-1"]);
        assert!(fallback_fan_ids.contains("fan-1"));
        assert!(matches!(
            events.events().last().map(|event| &event.reason),
            Some(DegradedReason::FallbackActive { affected_fans }) if affected_fans == &vec!["fan-1".to_string()]
        ));
    }

    #[test]
    fn panic_path_uses_same_fallback_recorder() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan("fan-1", ControlMode::Pwm, "/definitely/missing/pwm1");
        let owned = Arc::new(RwLock::new(owned));
        let config = Arc::new(RwLock::new(AppConfig::default()));
        let events = Arc::new(RwLock::new(LifecycleEventLog::new()));
        let fallback_fan_ids = Arc::new(RwLock::new(HashSet::new()));

        assert!(run_panic_fallback_recorder(
            &owned,
            &config,
            &events,
            &fallback_fan_ids,
            "panic: simulated".to_string(),
        ));

        let config = config.try_read().unwrap();
        assert!(config.fallback_incident.is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_runs_managed_fan_loops_and_writes_pwm() {
        let fixture = ControlFixture::new();
        fixture.write_temp("55000\n");
        fixture.write_pwm_seed("0\n");

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

        let supervisor = ControlSupervisor::new(snapshot, config, owned, degraded);
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;

        let status = supervisor
            .status_json()
            .await
            .expect("status should serialize");
        assert!(status.contains("hwmon-test-0000000000000001-fan1"));
        assert!(status.contains("logical_output_percent"));

        let pwm = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        assert_ne!(pwm.trim(), "0");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_degrades_when_all_temp_sources_fail() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

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

        let supervisor = ControlSupervisor::new(snapshot, config, owned, Arc::clone(&degraded));
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;

        let degraded = degraded.read().await;
        let reasons = degraded
            .entries
            .get("hwmon-test-0000000000000001-fan1")
            .expect("fan should be degraded");
        assert!(matches!(
            reasons.first(),
            Some(DegradedReason::TempSourceMissing { .. })
        ));

        let status = supervisor
            .status_json()
            .await
            .expect("status should serialize");
        assert!(!status.contains("hwmon-test-0000000000000001-fan1"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_skips_unowned_fans_and_stops_after_ownership_loss() {
        let fixture = ControlFixture::new();
        fixture.write_temp("57000\n");
        fixture.write_pwm_seed("0\n");

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
        let degraded = Arc::new(RwLock::new(DegradedState::new()));

        let supervisor = ControlSupervisor::new(
            Arc::clone(&snapshot),
            Arc::clone(&config),
            Arc::clone(&owned),
            degraded,
        );
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(
            fs::read_to_string(fixture.pwm_path())
                .expect("pwm should be readable")
                .trim(),
            "0"
        );

        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        let written = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        assert_ne!(written.trim(), "0");

        owned
            .write()
            .await
            .release_fan("hwmon-test-0000000000000001-fan1");
        tokio::time::sleep(Duration::from_millis(80)).await;
        let after_release = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        tokio::time::sleep(Duration::from_millis(80)).await;
        let final_pwm = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        assert_eq!(after_release, final_pwm);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_iface_get_control_status_serializes_live_snapshots() {
        let fixture = ControlFixture::new();
        fixture.write_temp("56000\n");
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            )),
            ..AppConfig::default()
        }));
        let owned = Arc::new(RwLock::new(OwnedFanSet::new()));
        owned.write().await.claim_fan(
            "hwmon-test-0000000000000001-fan1",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        let degraded = Arc::new(RwLock::new(DegradedState::new()));
        let supervisor = ControlSupervisor::new(snapshot, config, owned, degraded);
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;

        let iface = ControlIface {
            supervisor,
            config: Arc::new(RwLock::new(AppConfig::default())),
        };
        let status = iface
            .get_control_status()
            .await
            .expect("control status should serialize");
        assert!(status.contains("hwmon-test-0000000000000001-fan1"));
        assert!(status.contains("aggregated_temp_millidegrees"));
        assert!(status.contains("mapped_pwm"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_reconciles_after_applied_config_changes() {
        let fixture = ControlFixture::new();
        fixture.write_temp("56500\n");
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig::default()));
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
            Arc::clone(&owned),
            degraded,
        );

        supervisor.reconcile().await;
        assert_eq!(supervisor.status_json().await.expect("status"), "{}");

        {
            let mut config = config.write().await;
            config.applied = Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            ));
        }
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(80)).await;
        let started = supervisor.status_json().await.expect("status");
        assert!(started.contains("hwmon-test-0000000000000001-fan1"));

        {
            let mut config = config.write().await;
            config.applied = Some(AppliedConfig {
                fans: HashMap::new(),
                applied_at: Some("2026-04-11T12:05:00Z".to_string()),
            });
        }
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(supervisor.status_json().await.expect("status"), "{}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_supervisor_degrades_on_pwm_write_failure_keeps_owned() {
        let fixture = ControlFixture::new();
        fixture.write_temp("56000\n");
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            )),
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
            Arc::clone(&owned),
            Arc::clone(&degraded),
        );
        supervisor.reconcile().await;
        tokio::time::sleep(Duration::from_millis(40)).await;

        fs::remove_file(fixture.pwm_path()).expect("should remove pwm file to force write failure");
        fs::create_dir(fixture.pwm_path())
            .expect("should replace pwm file with directory to force write failure");
        tokio::time::sleep(Duration::from_millis(80)).await;

        // After PWM write failure, the fan should stay in OwnedFanSet (not released)
        // so that fallback writes remain possible for the degraded fan.
        assert!(owned.read().await.owns("hwmon-test-0000000000000001-fan1"));
        let degraded = degraded.read().await;
        assert!(
            degraded
                .entries
                .contains_key("hwmon-test-0000000000000001-fan1")
        );
        let status = supervisor
            .status_json()
            .await
            .expect("status should serialize");
        assert!(!status.contains("hwmon-test-0000000000000001-fan1"));
    }

    #[test]
    fn release_removed_owned_fans_drops_fans_not_in_next_applied_set() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

        let mut owned = OwnedFanSet::new();
        owned.claim_fan(
            "fan-a",
            ControlMode::Pwm,
            fixture.pwm_path().to_string_lossy().as_ref(),
        );
        owned.claim_fan("fan-b", ControlMode::Pwm, "/sys/class/hwmon/hwmon0/pwm2");

        let next_owned = HashSet::from(["fan-b".to_string()]);
        let failures = release_removed_owned_fans(&mut owned, &next_owned);

        assert!(failures.is_empty());
        assert!(!owned.owns("fan-a"));
        assert!(owned.owns("fan-b"));
    }

    #[test]
    fn release_removed_owned_fans_keeps_ownership_on_fallback_failure() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan("fan-a", ControlMode::Pwm, "/definitely/missing/pwm1");

        let next_owned = HashSet::new();
        let failures = release_removed_owned_fans(&mut owned, &next_owned);

        assert_eq!(failures.len(), 1);
        assert!(owned.owns("fan-a"));
    }

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

    #[tokio::test(flavor = "current_thread")]
    async fn control_iface_accept_auto_tune_stages_proposed_gains_into_draft() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        supervisor
            .start_auto_tune("hwmon-test-0000000000000001-fan1")
            .await
            .expect("auto-tune should start");
        tokio::time::sleep(Duration::from_millis(15)).await;
        fixture.write_temp("59000\n");
        tokio::time::sleep(Duration::from_millis(15)).await;
        fixture.write_temp("57500\n");
        tokio::time::sleep(Duration::from_millis(60)).await;

        let applied_gains = config
            .read()
            .await
            .applied
            .as_ref()
            .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
            .expect("applied entry should exist")
            .pid_gains;

        let iface = ControlIface {
            supervisor,
            config: Arc::clone(&config),
        };
        let updated = iface
            .accept_auto_tune_for_test("hwmon-test-0000000000000001-fan1", true)
            .await
            .expect("accepted gains should stage into draft");
        assert!(updated.contains("pid_gains"));

        let config_guard = config.read().await;
        let draft_entry = config_guard
            .draft
            .fans
            .get("hwmon-test-0000000000000001-fan1")
            .expect("draft entry should exist");
        assert!(draft_entry.pid_gains.is_some());
        assert_ne!(draft_entry.pid_gains.expect("gains"), applied_gains);
        assert_eq!(
            config_guard
                .applied
                .as_ref()
                .and_then(|applied| applied.fans.get("hwmon-test-0000000000000001-fan1"))
                .expect("applied entry should exist")
                .pid_gains,
            applied_gains
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_iface_profile_mutations_enforce_authorization_and_stage_draft_updates() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        let iface = ControlIface {
            supervisor,
            config: Arc::clone(&config),
        };

        let unauthorized_accept = iface
            .accept_auto_tune_for_test("hwmon-test-0000000000000001-fan1", false)
            .await;
        assert!(matches!(
            unauthorized_accept,
            Err(fdo::Error::AccessDenied(_))
        ));

        let profile_json = serde_json::json!({
            "target_temp_millidegrees": 68000,
            "aggregation": "max",
            "pid_gains": { "kp": 2.5, "ki": 0.3, "kd": 0.9 },
            "cadence": {
                "sample_interval_ms": 500,
                "control_interval_ms": 1000,
                "write_interval_ms": 1500
            },
            "deadband_millidegrees": 2000,
            "actuator_policy": {
                "output_min_percent": 10.0,
                "output_max_percent": 95.0,
                "pwm_min": 15,
                "pwm_max": 240,
                "startup_kick_percent": 45.0,
                "startup_kick_ms": 1200
            },
            "pid_limits": {
                "integral_min": -20.0,
                "integral_max": 20.0,
                "derivative_min": -6.0,
                "derivative_max": 6.0
            }
        })
        .to_string();

        let unauthorized_profile = iface
            .set_draft_fan_control_profile_for_test(
                "hwmon-test-0000000000000001-fan1",
                &profile_json,
                false,
            )
            .await;
        assert!(matches!(
            unauthorized_profile,
            Err(fdo::Error::AccessDenied(_))
        ));

        let updated = iface
            .set_draft_fan_control_profile_for_test(
                "hwmon-test-0000000000000001-fan1",
                &profile_json,
                true,
            )
            .await
            .expect("authorized profile update should succeed");
        assert!(updated.contains("68000"));

        let config_guard = config.read().await;
        let draft_entry = config_guard
            .draft
            .fans
            .get("hwmon-test-0000000000000001-fan1")
            .expect("draft entry should exist");
        assert_eq!(draft_entry.target_temp_millidegrees, Some(68_000));
        assert_eq!(draft_entry.aggregation, Some(AggregationFn::Max));
        assert_eq!(draft_entry.pid_gains.expect("pid gains").kp, 2.5);
        assert_eq!(
            draft_entry.cadence.expect("cadence").write_interval_ms,
            1_500
        );
        assert_eq!(
            draft_entry
                .actuator_policy
                .expect("actuator policy")
                .startup_kick_percent,
            45.0
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn control_iface_partial_profile_updates_preserve_unspecified_draft_fields() {
        let fixture = ControlFixture::new();
        fixture.write_temp("60000\n");
        fixture.write_pwm_seed("0\n");

        let (supervisor, config, _) = auto_tune_test_harness(&fixture).await;
        let iface = ControlIface {
            supervisor,
            config: Arc::clone(&config),
        };

        iface
            .set_draft_fan_control_profile_for_test(
                "hwmon-test-0000000000000001-fan1",
                &serde_json::json!({
                    "target_temp_millidegrees": 68000,
                    "aggregation": "max",
                    "pid_gains": { "kp": 2.5, "ki": 0.3, "kd": 0.9 }
                })
                .to_string(),
                true,
            )
            .await
            .expect("seed profile update should succeed");

        iface
            .set_draft_fan_control_profile_for_test(
                "hwmon-test-0000000000000001-fan1",
                &serde_json::json!({
                    "deadband_millidegrees": 3500
                })
                .to_string(),
                true,
            )
            .await
            .expect("partial profile update should succeed");

        let config_guard = config.read().await;
        let draft_entry = config_guard
            .draft
            .fans
            .get("hwmon-test-0000000000000001-fan1")
            .expect("draft entry should exist");
        assert_eq!(draft_entry.target_temp_millidegrees, Some(68_000));
        assert_eq!(draft_entry.aggregation, Some(AggregationFn::Max));
        assert_eq!(draft_entry.pid_gains.expect("pid gains").kp, 2.5);
        assert_eq!(draft_entry.deadband_millidegrees, Some(3_500));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn degrade_and_stop_writes_fallback_pwm() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            )),
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
            Arc::clone(&owned),
            Arc::clone(&degraded),
        );

        supervisor
            .degrade_and_stop(
                "hwmon-test-0000000000000001-fan1",
                DegradedReason::TempSourceMissing {
                    fan_id: "hwmon-test-0000000000000001-fan1".to_string(),
                    temp_id: "hwmon-test-0000000000000001-temp1".to_string(),
                },
            )
            .await;

        let pwm = fs::read_to_string(fixture.pwm_path()).expect("pwm should be readable");
        assert_eq!(pwm.trim(), "255");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn degrade_and_stop_keeps_fan_owned() {
        let fixture = ControlFixture::new();
        fixture.write_pwm_seed("0\n");

        let snapshot = Arc::new(RwLock::new(test_snapshot(fixture.root())));
        let config = Arc::new(RwLock::new(AppConfig {
            applied: Some(applied_config_for(
                "hwmon-test-0000000000000001-fan1",
                "hwmon-test-0000000000000001-temp1",
            )),
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
            Arc::clone(&owned),
            Arc::clone(&degraded),
        );

        supervisor
            .degrade_and_stop(
                "hwmon-test-0000000000000001-fan1",
                DegradedReason::TempSourceMissing {
                    fan_id: "hwmon-test-0000000000000001-fan1".to_string(),
                    temp_id: "hwmon-test-0000000000000001-temp1".to_string(),
                },
            )
            .await;

        assert!(owned.read().await.owns("hwmon-test-0000000000000001-fan1"));
    }
}
