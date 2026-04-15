//! Panic-time fallback to safe fan speeds.
//!
//! When the daemon process panics, the standard Rust panic hook runs
//! before the process terminates. This module installs a custom panic
//! hook that:
//!
//! 1. Writes PWM 255 to all owned fans via a lock-free mirror
//!    (`PanicFallbackMirror`), so fans fail to high speed even if
//!    async locks are poisoned.
//! 2. Records the fallback incident so it's visible after restart.
//!
//! See `docs/safety-model.md` for the full fail-safe design.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;

use kde_fan_control_core::config::{AppConfig, LifecycleEventLog};
use kde_fan_control_core::lifecycle::{OwnedFanSet, PWM_ENABLE_MANUAL, PWM_SAFE_MAX};
use tokio::sync::RwLock;

use crate::safety::fallback::record_fallback_incident_for_owned;

#[derive(Debug, Default)]
pub struct PanicFallbackMirror {
    pub(crate) owned_pwm_paths: StdRwLock<Vec<(String, String)>>,
}

pub fn sync_panic_fallback_mirror_from_owned(mirror: &PanicFallbackMirror, owned: &OwnedFanSet) {
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

pub fn write_fallback_from_panic_mirror(
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
        let pwm_enable_path = format!("{pwm_path}_enable");
        if let Err(error) = std::fs::write(&pwm_enable_path, PWM_ENABLE_MANUAL.to_string()) {
            eprintln!(
                "panic fallback: could not set manual mode for {fan_id} at {pwm_enable_path}: {error}"
            );
        }

        match std::fs::write(&pwm_path, PWM_SAFE_MAX.to_string()) {
            Ok(()) => succeeded.push(fan_id),
            Err(error) => failed.push((fan_id, format!("pwm write failed: {error}"))),
        }
    }

    (succeeded, failed)
}

pub fn run_panic_fallback_recorder(
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

pub fn install_panic_fallback_hook(
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
            eprintln!("panic fallback wrote safe maximum for fans: {succeeded:?}");
        }
        if !failed.is_empty() {
            eprintln!("panic fallback failed for fans: {failed:?}");
        }

        let _ = run_panic_fallback_recorder(&owned, &config, &events, &fallback_fan_ids, trigger);
        previous_hook(panic_info);
    }));
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;

    use kde_fan_control_core::config::{AppConfig, LifecycleEventLog};
    use kde_fan_control_core::inventory::ControlMode;
    use kde_fan_control_core::lifecycle::OwnedFanSet;
    use tokio::sync::RwLock;

    use super::run_panic_fallback_recorder;

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

        let config = config
            .try_read()
            .expect("config lock should be available after panic recorder completes");
        assert!(config.fallback_incident.is_some());
    }
}
