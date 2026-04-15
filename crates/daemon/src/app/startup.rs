//! Daemon startup and lifecycle orchestration.
//!
//! This module contains the core daemon `run` function that:
//! - Discovers hardware
//! - Loads configuration
//! - Performs boot reconciliation
//! - Registers DBus interfaces
//! - Delegates background tasks and shutdown to their own modules

use std::sync::Arc;

use kde_fan_control_core::config::{AppConfig, DegradedState, LifecycleEventLog};
use kde_fan_control_core::inventory::{discover, discover_from};
use kde_fan_control_core::lifecycle::{
    OwnedFanSet, lifecycle_event_from_fallback_incident, perform_boot_reconciliation,
};
use sd_notify::NotifyState;
use tokio::sync::RwLock;
use zbus::connection::Builder;

use crate::args::DaemonArgs;
use crate::control::supervisor::ControlSupervisor;
use crate::dbus::constants::{BUS_NAME, BUS_PATH_CONTROL, BUS_PATH_INVENTORY, BUS_PATH_LIFECYCLE};
use crate::dbus::control::ControlIface;
use crate::dbus::inventory::InventoryIface;
use crate::dbus::lifecycle::LifecycleIface;
use crate::safety::ownership::persist_owned_fans;
use crate::safety::panic_hook::install_panic_fallback_hook;

use super::background::BackgroundTasks;
use super::shutdown;

pub async fn run(args: DaemonArgs) -> Result<(), Box<dyn std::error::Error>> {
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
    let panic_mirror = control.panic_fallback_mirror();
    control.sync_panic_fallback_mirror().await;

    install_panic_fallback_hook(
        Arc::clone(&owned),
        Arc::clone(&config),
        Arc::clone(&events),
        Arc::clone(&fallback_fan_ids),
        panic_mirror,
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

    control.sync_panic_fallback_mirror().await;

    control.reconcile().await;

    {
        let owned_guard = owned.read().await;
        if !owned_guard.is_empty() {
            persist_owned_fans(&owned_guard);
        }
    }

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
        config: Arc::clone(&config),
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

    control.set_signal_connection(_connection.clone()).await;

    tracing::info!(
        name = BUS_NAME,
        "D-Bus inventory and lifecycle surfaces ready"
    );

    let _ = sd_notify::notify(&[NotifyState::Ready]);

    BackgroundTasks::spawn(
        Arc::clone(&snapshot),
        Arc::clone(&config),
        Arc::clone(&events),
        control.clone(),
    );

    shutdown::wait_and_shutdown(control, owned, config, events, fallback_fan_ids).await
}
