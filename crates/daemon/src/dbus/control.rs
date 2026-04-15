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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use kde_fan_control_core::config::{AppConfig, DegradedState};
    use kde_fan_control_core::control::AggregationFn;
    use kde_fan_control_core::inventory::ControlMode;
    use kde_fan_control_core::lifecycle::OwnedFanSet;
    use tokio::sync::RwLock;

    use crate::control::supervisor::ControlSupervisor;
    use crate::dbus::control::ControlIface;
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

        use zbus::fdo;
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
}
