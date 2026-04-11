use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use kde_fan_control_core::config::{
    AppConfig, AppliedFanEntry, DegradedReason, DegradedState, DraftFanEntry, LifecycleEvent,
    LifecycleEventLog, apply_draft, validate_draft,
};
use kde_fan_control_core::control::{
    PidController, map_output_percent_to_pwm, startup_kick_required,
};
use kde_fan_control_core::inventory::{ControlMode, InventorySnapshot, discover, discover_from};
use kde_fan_control_core::lifecycle::{
    ControlRuntimeSnapshot, FallbackResult, OwnedFanSet, RuntimeState,
    lifecycle_event_from_fallback_incident, perform_boot_reconciliation, write_fallback_for_owned,
};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::{MissedTickBehavior, interval};
use tracing_subscriber::EnvFilter;
use zbus::fdo;
use zbus::{connection::Builder, interface, object_server::SignalEmitter};

const BUS_NAME: &str = "org.kde.FanControl";
const BUS_PATH_INVENTORY: &str = "/org/kde/FanControl";
const BUS_PATH_LIFECYCLE: &str = "/org/kde/FanControl/Lifecycle";
const BUS_PATH_CONTROL: &str = "/org/kde/FanControl/Control";

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
}

#[derive(Clone, Debug)]
struct ControlSupervisor {
    inner: Arc<ControlSupervisorInner>,
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
            }),
        }
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

        {
            let mut tasks = self.inner.tasks.write().await;
            for (_, task) in tasks.drain() {
                task.handle.abort();
            }
        }
        self.inner.status.write().await.clear();

        for (fan_id, entry) in desired {
            self.inner.degraded.write().await.clear_fan(&fan_id);
            self.inner
                .status
                .write()
                .await
                .insert(fan_id.clone(), control_snapshot_from_applied(&entry));

            let supervisor = self.clone();
            let fan_id_for_task = fan_id.clone();
            let handle = tokio::spawn(async move {
                supervisor.run_fan_loop(fan_id_for_task, entry).await;
            });

            self.inner
                .tasks
                .write()
                .await
                .insert(fan_id, ControlTaskHandle { handle });
        }
    }

    async fn status_json(&self) -> Result<String, serde_json::Error> {
        let status = self.inner.status.read().await;
        serde_json::to_string(&*status)
    }

    async fn run_fan_loop(&self, fan_id: String, entry: AppliedFanEntry) {
        let pwm_path = match self.inner.owned.read().await.sysfs_path(&fan_id) {
            Some(path) => path.to_string(),
            None => {
                self.clear_status(&fan_id).await;
                return;
            }
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
            tokio::select! {
                _ = sample_interval.tick() => {
                    match self.sample_temperatures(&fan_id, &entry).await {
                        Ok(aggregated_temp) => {
                            latest_aggregated_temp = Some(aggregated_temp);
                            self.update_status(&fan_id, |status| {
                                status.aggregated_temp_millidegrees = Some(aggregated_temp);
                                status.alert_high_temp = aggregated_temp >= status.target_temp_millidegrees + 5_000;
                            }).await;
                        }
                        Err(reason) => {
                            self.degrade_and_stop(&fan_id, reason).await;
                            break;
                        }
                    }
                }
                _ = control_interval.tick() => {
                    if let Some(aggregated_temp) = latest_aggregated_temp {
                        let output = controller.update(
                            aggregated_temp,
                            entry.cadence.control_interval_ms as f64 / 1_000.0,
                        );
                        latest_output_percent = Some(output.logical_output_percent);
                        self.update_status(&fan_id, |status| {
                            status.aggregated_temp_millidegrees = Some(aggregated_temp);
                            status.logical_output_percent = Some(output.logical_output_percent);
                            status.last_error_millidegrees = Some(output.error_millidegrees.round() as i64);
                            status.alert_high_temp = aggregated_temp >= status.target_temp_millidegrees + 5_000;
                        }).await;
                    }
                }
                _ = write_interval.tick() => {
                    if !self.inner.owned.read().await.owns(&fan_id) {
                        self.clear_status(&fan_id).await;
                        break;
                    }

                    let Some(output_percent) = latest_output_percent else {
                        continue;
                    };

                    if startup_kick_required(last_written_percent, output_percent) {
                        let kick_percent = entry
                            .actuator_policy
                            .startup_kick_percent
                            .max(output_percent);
                        let kick_pwm = map_output_percent_to_pwm(kick_percent, &entry.actuator_policy);
                        if let Err(error) = write_pwm_value(&pwm_path, kick_pwm) {
                            tracing::warn!(fan_id = %fan_id, path = %pwm_path, error = %error, "failed startup-kick pwm write");
                        }
                        tokio::time::sleep(Duration::from_millis(entry.actuator_policy.startup_kick_ms)).await;
                    }

                    let mapped_pwm = map_output_percent_to_pwm(output_percent, &entry.actuator_policy);
                    match write_pwm_value(&pwm_path, mapped_pwm) {
                        Ok(()) => {
                            latest_pwm = Some(mapped_pwm);
                            last_written_percent = Some(output_percent);
                            self.update_status(&fan_id, |status| {
                                status.logical_output_percent = Some(output_percent);
                                status.mapped_pwm = Some(mapped_pwm);
                            }).await;
                        }
                        Err(error) => {
                            tracing::warn!(fan_id = %fan_id, path = %pwm_path, error = %error, "failed pwm control write");
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

        let mut readings = Vec::new();
        let mut first_missing = entry
            .temp_sources
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown-temp".to_string());

        for (temp_id, path) in resolved_sources {
            first_missing = temp_id.clone();
            match fs::read_to_string(&path)
                .ok()
                .and_then(|value| value.trim().parse::<i64>().ok())
            {
                Some(reading) => readings.push(reading),
                None => {
                    tracing::warn!(fan_id = %fan_id, temp_id = %temp_id, path = %path.display(), "failed to read live temperature input");
                }
            }
        }

        entry
            .aggregation
            .compute_millidegrees(&readings)
            .ok_or_else(|| DegradedReason::TempSourceMissing {
                fan_id: fan_id.to_string(),
                temp_id: first_missing,
            })
    }

    async fn update_status<F>(&self, fan_id: &str, update: F)
    where
        F: FnOnce(&mut ControlRuntimeSnapshot),
    {
        let mut status = self.inner.status.write().await;
        if let Some(snapshot) = status.get_mut(fan_id) {
            update(snapshot);
        }
    }

    async fn clear_status(&self, fan_id: &str) {
        self.inner.status.write().await.remove(fan_id);
        self.inner.tasks.write().await.remove(fan_id);
    }

    async fn degrade_and_stop(&self, fan_id: &str, reason: DegradedReason) {
        self.inner
            .degraded
            .write()
            .await
            .mark_degraded(fan_id.to_string(), vec![reason]);
        self.clear_status(fan_id).await;
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

fn resolve_temp_sources(snapshot: &InventorySnapshot, temp_sources: &[String]) -> Vec<(String, PathBuf)> {
    temp_sources
        .iter()
        .filter_map(|temp_id| {
            snapshot.devices.iter().find_map(|device| {
                device.temperatures.iter().find(|sensor| &sensor.id == temp_id).map(|sensor| {
                    (
                        temp_id.clone(),
                        PathBuf::from(&device.sysfs_path).join(format!("temp{}_input", sensor.channel)),
                    )
                })
            })
        })
        .collect()
}

fn write_pwm_value(pwm_path: &str, pwm_value: u16) -> std::io::Result<()> {
    let pwm_enable_path = format!("{}_enable", pwm_path);
    if let Err(error) = fs::write(&pwm_enable_path, kde_fan_control_core::lifecycle::PWM_ENABLE_MANUAL.to_string()) {
        tracing::warn!(path = %pwm_enable_path, error = %error, "failed to set pwm channel to manual mode before write");
    }
    fs::write(pwm_path, pwm_value.to_string())
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
/// operations. The current policy requires UID 0 (root). This function is
/// explicitly structured so that a future `polkit` check can replace the
/// UID comparison without changing the DBus method contract.
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
        .get_connection_unix_user(bus_name)
        .await
        .map_err(|e| fdo::Error::AccessDenied(format!("could not resolve caller identity: {e}")))?;

    if uid != 0 {
        tracing::warn!(caller_uid = uid, "unauthorized write attempt");
        return Err(fdo::Error::AccessDenied(
            "privileged operations require root access".into(),
        ));
    }

    Ok(())
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

    async fn set_sensor_name(&self, id: &str, name: &str) -> fdo::Result<()> {
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

    async fn set_fan_name(&self, id: &str, name: &str) -> fdo::Result<()> {
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

    async fn remove_sensor_name(&self, id: &str) -> fdo::Result<()> {
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

    async fn remove_fan_name(&self, id: &str) -> fdo::Result<()> {
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

struct ControlIface {
    supervisor: ControlSupervisor,
}

#[interface(name = "org.kde.FanControl.Control")]
impl ControlIface {
    async fn get_control_status(&self) -> fdo::Result<String> {
        self.supervisor
            .status_json()
            .await
            .map_err(|e| fdo::Error::Failed(format!("control status serialization error: {e}")))
    }

    #[zbus(signal)]
    async fn control_status_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
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
        let (owned_guard, config_guard, snapshot_guard, degraded_guard, fallback_guard) = {
            let owned = self.owned.read().await;
            let config = self.config.read().await;
            let snapshot = self.snapshot.read().await;
            let degraded = self.degraded.read().await;
            let fallback_fan_ids = self.fallback_fan_ids.read().await;
            (
                owned.clone(),
                config.applied.clone(),
                snapshot.clone(),
                degraded.clone(),
                fallback_fan_ids.clone(),
            )
            // All guards dropped here
        };

        let state = RuntimeState::build(
            &owned_guard,
            config_guard.as_ref(),
            &degraded_guard,
            &fallback_guard,
            &snapshot_guard,
        );

        serde_json::to_string(&state)
            .map_err(|e| fdo::Error::Failed(format!("runtime state serialization error: {e}")))
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
            let (draft, snapshot) = {
                let config = self.config.read().await;
                let snapshot = self.snapshot.read().await;
                (config.draft.clone(), snapshot.clone())
            };
            let timestamp = format_iso8601_now();
            apply_draft(&draft, &snapshot, timestamp)
        };

        let had_rejections = !result.rejected.is_empty();

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

        // Claim successfully applied fans in the owned set and clear
        // degraded state for fans that passed validation.
        {
            let snapshot = self.snapshot.read().await;
            let mut owned = self.owned.write().await;
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
                    }
                }
                // Clear degraded state for this fan.
                self.degraded.write().await.clear_fan(fan_id);
            }
        }

        // Persist the applied config.
        {
            let mut config = self.config.write().await;
            config.set_applied(applied);
            config.clear_fallback_incident();
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.fallback_fan_ids.write().await.clear();
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
        | kde_fan_control_core::config::ValidationError::InvalidPidLimits { fan_id, .. } => {
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

    install_panic_fallback_hook(
        Arc::clone(&owned),
        Arc::clone(&config),
        Arc::clone(&events),
        Arc::clone(&fallback_fan_ids),
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

    control.reconcile().await;

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

    tracing::info!(
        name = BUS_NAME,
        "D-Bus inventory and lifecycle surfaces ready"
    );

    // Wait for shutdown signal, then drive fallback for owned fans.
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down — driving owned fans to safe maximum");

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
    use kde_fan_control_core::config::{AppliedConfig, AppliedFanEntry, DegradedState};
    use kde_fan_control_core::control::{ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits};
    use kde_fan_control_core::config::LifecycleEventLog;
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
                kp: 0.01,
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

        let status = supervisor.status_json().await.expect("status should serialize");
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

        let status = supervisor.status_json().await.expect("status should serialize");
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
}
