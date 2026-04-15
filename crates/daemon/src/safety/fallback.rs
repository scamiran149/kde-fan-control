//! Graceful fallback incident recording.
//!
//! When the daemon shuts down or encounters a condition that requires
//! driving owned fans to PWM 255, this module records the incident
//! in config and lifecycle events for post-mortem visibility.
//!
//! See `docs/safety-model.md` for the full fail-safe design.

use std::collections::HashSet;
use std::sync::Arc;

use kde_fan_control_core::config::{AppConfig, LifecycleEventLog};
use kde_fan_control_core::lifecycle::{
    FallbackResult, OwnedFanSet, lifecycle_event_from_fallback_incident, write_fallback_for_owned,
};
use tokio::sync::RwLock;

use crate::time::format_iso8601_now;

pub fn record_fallback_incident_for_owned(
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

pub async fn run_fallback_recorder(
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

#[cfg(test)]
mod tests {
    use kde_fan_control_core::config::{AppConfig, LifecycleEventLog};
    use kde_fan_control_core::inventory::ControlMode;
    use kde_fan_control_core::lifecycle::OwnedFanSet;

    use super::record_fallback_incident_for_owned;

    #[test]
    fn shared_fallback_recorder_persists_incident_for_graceful_shutdown() {
        let mut owned = OwnedFanSet::new();
        owned.claim_fan("fan-1", ControlMode::Pwm, "/definitely/missing/pwm1");
        let mut config = AppConfig::default();
        let mut events = LifecycleEventLog::new();
        let mut fallback_fan_ids = std::collections::HashSet::new();

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
            Some(kde_fan_control_core::config::DegradedReason::FallbackActive { affected_fans }) if affected_fans == &vec!["fan-1".to_string()]
        ));
    }
}
