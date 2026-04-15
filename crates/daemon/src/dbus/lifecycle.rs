//! DBus Lifecycle interface implementation.
//!
//! Provides the `org.kde.FanControl.Lifecycle` DBus interface for
//! draft/apply, degraded state, events, runtime state, and overview.
//!
//! The `apply_draft` transaction logic lives in [`super::lifecycle_apply`].

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::RwLock;
use zbus::fdo;
use zbus::{interface, object_server::SignalEmitter};

use kde_fan_control_core::config::{
    AppConfig, DegradedState, DraftFanEntry, LifecycleEventLog, validate_draft,
};
use kde_fan_control_core::inventory::InventorySnapshot;
use kde_fan_control_core::lifecycle::OwnedFanSet;
use kde_fan_control_core::overview::{OverviewStructureSnapshot, OverviewTelemetryBatch};

use crate::control::supervisor::ControlSupervisor;
use crate::dbus::auth::require_authorized;

pub struct LifecycleIface {
    pub config: Arc<RwLock<AppConfig>>,
    pub snapshot: Arc<RwLock<InventorySnapshot>>,
    pub degraded: Arc<RwLock<DegradedState>>,
    pub events: Arc<RwLock<LifecycleEventLog>>,
    pub owned: Arc<RwLock<OwnedFanSet>>,
    pub fallback_fan_ids: Arc<RwLock<HashSet<String>>>,
    pub control: ControlSupervisor,
}

#[interface(name = "org.kde.FanControl.Lifecycle")]
impl LifecycleIface {
    async fn get_draft_config(&self) -> fdo::Result<String> {
        let config = self.config.read().await;
        serde_json::to_string(&config.draft)
            .map_err(|e| fdo::Error::Failed(format!("draft serialization error: {e}")))
    }

    async fn get_applied_config(&self) -> fdo::Result<String> {
        let config = self.config.read().await;
        serde_json::to_string(&config.applied)
            .map_err(|e| fdo::Error::Failed(format!("applied serialization error: {e}")))
    }

    async fn get_degraded_summary(&self) -> fdo::Result<String> {
        let degraded = self.degraded.read().await;
        serde_json::to_string(&*degraded)
            .map_err(|e| fdo::Error::Failed(format!("degraded serialization error: {e}")))
    }

    async fn get_lifecycle_events(&self) -> fdo::Result<String> {
        let events = self.events.read().await;
        serde_json::to_string(events.events())
            .map_err(|e| fdo::Error::Failed(format!("events serialization error: {e}")))
    }

    async fn get_runtime_state(&self) -> fdo::Result<String> {
        let fallback_guard = self.fallback_fan_ids.read().await.clone();
        let state = self.control.runtime_state_snapshot(&fallback_guard).await;

        serde_json::to_string(&state)
            .map_err(|e| fdo::Error::Failed(format!("runtime state serialization error: {e}")))
    }

    async fn request_authorization(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await
    }

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

        let parsed_mode = crate::dbus::helpers::parse_control_mode(&control_mode)?;

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

        let config = self.config.read().await;
        serde_json::to_string(&config.draft)
            .map_err(|e| fdo::Error::Failed(format!("draft serialization error: {e}")))
    }

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

    async fn apply_draft(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        self.apply_draft_transaction(connection, header, emitter)
            .await
    }

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

    #[zbus(signal)]
    async fn draft_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn applied_config_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn degraded_state_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn lifecycle_event_appended(
        emitter: &SignalEmitter<'_>,
        event_kind: &str,
        detail: &str,
    ) -> zbus::Result<()>;
}
