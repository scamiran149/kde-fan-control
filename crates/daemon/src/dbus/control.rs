//! DBus Control interface implementation.
//!
//! Provides the `org.kde.FanControl.Control` DBus interface for live
//! status, auto-tune, and profile mutations.

use std::sync::Arc;

use tokio::sync::RwLock;
use zbus::fdo;
use zbus::{interface, object_server::SignalEmitter};

use kde_fan_control_core::config::AppConfig;

use crate::control::helpers::draft_entry_from_applied;
use crate::control::supervisor::ControlSupervisor;
use crate::dbus::auth::{require_authorized, require_test_authorized};
use crate::dbus::signals::emit_draft_changed;
use crate::state::DraftFanControlProfilePayload;

pub struct ControlIface {
    pub supervisor: ControlSupervisor,
    pub config: Arc<RwLock<AppConfig>>,
}

impl ControlIface {
    pub async fn accept_auto_tune_inner(&self, fan_id: &str) -> fdo::Result<String> {
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

    pub async fn set_draft_fan_control_profile_inner(
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
            if let Some(target) = value
                && (target <= 0 || target > 150_000)
            {
                return Err(fdo::Error::InvalidArgs(format!(
                    "target_temp_millidegrees {target} is out of bounds (must be 1..=150000)"
                )));
            }
            draft_entry.target_temp_millidegrees = value;
        }
        if let Some(value) = patch.aggregation {
            draft_entry.aggregation = value;
        }
        if let Some(value) = patch.pid_gains {
            if let Some(ref gains) = value
                && !gains.is_finite()
            {
                return Err(fdo::Error::InvalidArgs(
                    "pid_gains contains non-finite values (NaN or Infinity)".into(),
                ));
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
            if let Some(ref limits) = value
                && !limits.is_finite()
            {
                return Err(fdo::Error::InvalidArgs(
                    "pid_limits contains non-finite values (NaN or Infinity)".into(),
                ));
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
    pub async fn accept_auto_tune_for_test(
        &self,
        fan_id: &str,
        authorized: bool,
    ) -> fdo::Result<String> {
        require_test_authorized(authorized)?;
        self.accept_auto_tune_inner(fan_id).await
    }

    #[allow(dead_code)]
    pub async fn set_draft_fan_control_profile_for_test(
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
    pub async fn get_control_status(&self) -> fdo::Result<String> {
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
