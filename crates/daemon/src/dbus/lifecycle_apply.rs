//! Apply-draft transaction logic for the Lifecycle DBus interface.
//!
//! Contains the full `apply_draft_transaction` method (the actual
//! apply-draft state machine) and the `release_removed_owned_fans`
//! helper, both split out from `lifecycle.rs` for maintainability.

use std::collections::HashSet;

use zbus::fdo;
use zbus::object_server::SignalEmitter;

use kde_fan_control_core::config::{DegradedReason, LifecycleEvent, apply_draft};
use kde_fan_control_core::lifecycle::OwnedFanSet;

use crate::dbus::auth::require_authorized;
use crate::dbus::helpers::validation_error_to_degraded_reason;
use crate::dbus::signals::emit_control_status_changed;
use crate::safety::ownership::persist_owned_fans;
use crate::safety::panic_hook::sync_panic_fallback_mirror_from_owned;
use crate::time::format_iso8601_now;

use super::lifecycle::LifecycleIface;
use super::lifecycle::LifecycleIfaceSignals;

pub fn release_removed_owned_fans(
    owned: &mut OwnedFanSet,
    next_owned: &HashSet<String>,
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use kde_fan_control_core::inventory::ControlMode;
    use kde_fan_control_core::lifecycle::OwnedFanSet;

    use crate::dbus::lifecycle_apply::release_removed_owned_fans;
    use crate::test_support::ControlFixture;

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
}

impl LifecycleIface {
    pub async fn apply_draft_transaction(
        &self,
        connection: &zbus::Connection,
        header: zbus::message::Header<'_>,
        emitter: SignalEmitter<'_>,
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

        {
            let owned = self.owned.read().await;
            let result = kde_fan_control_core::lifecycle::write_fallback_for_owned(&owned);
            if !result.failed.is_empty() {
                tracing::warn!(failed = ?result.failed, "some fans failed to receive fallback PWM during ApplyDraft stop");
            }
        }

        {
            let mut degraded = self.degraded.write().await;
            for (fan_id, error) in &result.rejected {
                degraded.mark_degraded(
                    fan_id.clone(),
                    vec![validation_error_to_degraded_reason(error)],
                );
            }
        }

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

        {
            let snapshot = self.snapshot.read().await;
            let mut owned = self.owned.write().await;
            let next_owned: HashSet<_> = result.enrollable.iter().cloned().collect();
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

                    if let Some(applied_entry) = applied.fans.get(fan_id) {
                        owned.claim_fan(fan_id, applied_entry.control_mode, &sysfs_path);
                        sync_panic_fallback_mirror_from_owned(
                            &self.control.panic_fallback_mirror(),
                            &owned,
                        );
                    }
                }
                self.degraded.write().await.clear_fan(fan_id);
            }

            persist_owned_fans(&owned);
        }

        self.control.reconcile().await;

        if let Err(e) = emitter.draft_changed().await {
            tracing::warn!(error = %e, "failed to emit DraftChanged signal");
        }
        if let Err(e) = emitter.applied_config_changed().await {
            tracing::warn!(error = %e, "failed to emit AppliedConfigChanged signal");
        }
        if had_rejections && let Err(e) = emitter.degraded_state_changed().await {
            tracing::warn!(error = %e, "failed to emit DegradedStateChanged signal");
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
}
