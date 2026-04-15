//! DBus signal emission helpers.
//!
//! Typed wrappers around zbus object-server lookups that emit named
//! signals on the three fan-control DBus interfaces. Each helper
//! resolves the interface, calls the generated signal method, and
//! logs a warning on failure.

use crate::dbus::constants::{BUS_PATH_CONTROL, BUS_PATH_LIFECYCLE};
use crate::dbus::control::ControlIface;
use crate::dbus::control::ControlIfaceSignals;
use crate::dbus::lifecycle::LifecycleIface;
use crate::dbus::lifecycle::LifecycleIfaceSignals;

pub async fn emit_control_status_changed(connection: &zbus::Connection) {
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

pub async fn emit_auto_tune_completed(connection: &zbus::Connection, fan_id: &str) {
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

pub async fn emit_draft_changed(connection: &zbus::Connection) {
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

pub async fn emit_degraded_state_changed(connection: &zbus::Connection) {
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

pub async fn emit_applied_config_changed(connection: &zbus::Connection) {
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

pub async fn emit_lifecycle_event_appended(
    connection: &zbus::Connection,
    event_kind: &str,
    detail: &str,
) {
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
