//! Control supervisor: manages per-fan PID control loops.
//!
//! The `ControlSupervisor` owns the runtime state for all managed fans,
//! spawns control-loop tasks, and coordinates auto-tune, stale-data
//! detection, panic recovery, and degraded-fan re-assessment.
//!
//! The implementation is split across several files using the split-impl
//! pattern — each concern area has its own `impl ControlSupervisor` block:
//!
//! - **autotune**: auto-tune state machine (start, record, fail, accept)
//! - **fan_loop**: per-fan PID control loop and temperature sampling
//! - **recovery**: degraded fan detection, panic recovery, and re-assessment

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use kde_fan_control_core::config::AppConfig;
use kde_fan_control_core::config::DegradedState;
use kde_fan_control_core::inventory::InventorySnapshot;
use kde_fan_control_core::lifecycle::{
    ControlRuntimeSnapshot, FanRuntimeStatus, OwnedFanSet, RuntimeState,
};

use crate::control::helpers::control_snapshot_from_applied;

use crate::safety::panic_hook::{PanicFallbackMirror, sync_panic_fallback_mirror_from_owned};
use crate::state::DaemonTuningSettings;

#[derive(Debug)]
pub struct ControlTaskHandle {
    pub handle: JoinHandle<()>,
}

#[derive(Debug)]
pub struct ControlSupervisorInner {
    pub snapshot: Arc<RwLock<InventorySnapshot>>,
    pub config: Arc<RwLock<AppConfig>>,
    pub owned: Arc<RwLock<OwnedFanSet>>,
    pub degraded: Arc<RwLock<DegradedState>>,
    pub tasks: RwLock<HashMap<String, ControlTaskHandle>>,
    pub status: RwLock<HashMap<String, ControlRuntimeSnapshot>>,
    pub fan_locals: RwLock<HashMap<String, Arc<StdMutex<ControlRuntimeSnapshot>>>>,
    pub rpm_locals: RwLock<HashMap<String, Arc<StdMutex<Option<u64>>>>>,
    pub stale_fan_counters: RwLock<HashMap<String, u32>>,
    pub publish_task: RwLock<Option<JoinHandle<()>>>,
    pub auto_tune: RwLock<HashMap<String, crate::state::AutoTuneExecutionState>>,
    pub tuning: RwLock<DaemonTuningSettings>,
    pub signal_connection: RwLock<Option<zbus::Connection>>,
    pub panic_fallback_mirror: Arc<PanicFallbackMirror>,
}

#[derive(Clone, Debug)]
pub struct ControlSupervisor {
    pub inner: Arc<ControlSupervisorInner>,
}

impl ControlSupervisor {
    pub fn new(
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

    pub fn panic_fallback_mirror(&self) -> Arc<PanicFallbackMirror> {
        Arc::clone(&self.inner.panic_fallback_mirror)
    }

    pub async fn sync_panic_fallback_mirror(&self) {
        let owned = self.inner.owned.read().await;
        sync_panic_fallback_mirror_from_owned(&self.inner.panic_fallback_mirror, &owned);
    }

    pub async fn set_signal_connection(&self, connection: zbus::Connection) {
        *self.inner.signal_connection.write().await = Some(connection);
    }

    pub async fn stop_all(&self) {
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

    pub async fn reconcile(&self) {
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

    pub async fn status_json(&self) -> Result<String, serde_json::Error> {
        let status = self.inner.status.read().await;
        serde_json::to_string(&*status)
    }

    pub async fn runtime_state_snapshot(&self, fallback_fan_ids: &HashSet<String>) -> RuntimeState {
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

    pub fn write_fan_local(
        local: &Arc<StdMutex<ControlRuntimeSnapshot>>,
        update: impl FnOnce(&mut ControlRuntimeSnapshot),
    ) {
        if let Ok(mut guard) = local.lock() {
            update(&mut guard);
        }
    }
}
