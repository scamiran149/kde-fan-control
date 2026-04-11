use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use kde_fan_control_core::config::{
    AppConfig, DegradedReason, DegradedState, DraftFanEntry, LifecycleEvent, LifecycleEventLog,
    apply_draft, validate_draft,
};
use kde_fan_control_core::inventory::{ControlMode, InventorySnapshot, discover, discover_from};
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;
use zbus::fdo;
use zbus::{connection::Builder, interface, object_server::SignalEmitter};

const BUS_NAME: &str = "org.kde.FanControl";
const BUS_PATH_INVENTORY: &str = "/org/kde/FanControl";
const BUS_PATH_LIFECYCLE: &str = "/org/kde/FanControl/Lifecycle";

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
// Lifecycle interface (draft/apply, degraded state, events)
// ---------------------------------------------------------------------------

struct LifecycleIface {
    config: Arc<RwLock<AppConfig>>,
    snapshot: Arc<RwLock<InventorySnapshot>>,
    degraded: Arc<RwLock<DegradedState>>,
    events: Arc<RwLock<LifecycleEventLog>>,
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
    /// to applied config, and reports any rejected fans.
    /// Emits DraftChanged, AppliedConfigChanged, and LifecycleEventAppended
    /// signals on success.
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

        // Persist the applied config.
        {
            let mut config = self.config.write().await;
            config.set_applied(applied);
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }

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
    }
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

    for device in &mut initial.devices {
        for sensor in &mut device.temperatures {
            sensor.friendly_name = config.sensor_name(&sensor.id).map(|n| n.to_string());
        }
        for fan in &mut device.fans {
            fan.friendly_name = config.fan_name(&fan.id).map(|n| n.to_string());
        }
    }

    let snapshot = Arc::new(RwLock::new(initial));
    let config = Arc::new(RwLock::new(config));
    let degraded = Arc::new(RwLock::new(DegradedState::new()));
    let events = Arc::new(RwLock::new(LifecycleEventLog::new()));

    let inventory_iface = InventoryIface {
        snapshot: Arc::clone(&snapshot),
        config: Arc::clone(&config),
    };

    let lifecycle_iface = LifecycleIface {
        config: Arc::clone(&config),
        snapshot: Arc::clone(&snapshot),
        degraded: Arc::clone(&degraded),
        events: Arc::clone(&events),
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
        .build()
        .await?;

    tracing::info!(
        name = BUS_NAME,
        "D-Bus inventory and lifecycle surfaces ready"
    );

    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");

    Ok(())
}
