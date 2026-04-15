//! Background daemon tasks.
//!
//! Spawns long-running tokio tasks for:
//! - systemd watchdog keep-alive
//! - RPM feedback polling from sysfs
//! - Periodic degraded-fan reassessment

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use kde_fan_control_core::config::{AppConfig, LifecycleEventLog};
use sd_notify::NotifyState;
use tokio::sync::RwLock;
use tokio::time::{MissedTickBehavior, interval};

use crate::control::supervisor::ControlSupervisor;
use kde_fan_control_core::inventory::InventorySnapshot;

pub struct BackgroundTasks;

impl BackgroundTasks {
    pub fn spawn(
        snapshot: Arc<RwLock<InventorySnapshot>>,
        config: Arc<RwLock<AppConfig>>,
        events: Arc<RwLock<LifecycleEventLog>>,
        control: ControlSupervisor,
    ) {
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(20));
            tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let _ = sd_notify::notify(&[NotifyState::Watchdog]);
            }
        });

        let rpm_snapshot = Arc::clone(&snapshot);
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(2));
            tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let paths = {
                    let snapshot = rpm_snapshot.read().await;
                    snapshot
                        .devices
                        .iter()
                        .flat_map(|device| {
                            device
                                .fans
                                .iter()
                                .filter(|fan| fan.rpm_feedback)
                                .map(|fan| {
                                    (
                                        fan.id.clone(),
                                        std::path::PathBuf::from(&device.sysfs_path)
                                            .join(format!("fan{}_input", fan.channel)),
                                    )
                                })
                        })
                        .collect::<Vec<_>>()
                };
                let updates: Vec<(String, u64)> = paths
                    .iter()
                    .filter_map(|(fan_id, path)| {
                        fs::read_to_string(path)
                            .ok()
                            .and_then(|v| v.trim().parse::<u64>().ok())
                            .map(|rpm| (fan_id.clone(), rpm))
                    })
                    .collect();
                if !updates.is_empty() {
                    let mut snapshot = rpm_snapshot.write().await;
                    for (fan_id, rpm) in updates {
                        snapshot.update_fan_rpm(&fan_id, Some(rpm));
                    }
                }
            }
        });

        let reassess_control = control.clone();
        let reassess_events = Arc::clone(&events);
        let reassess_config = Arc::clone(&config);
        tokio::spawn(async move {
            let interval_ms = reassess_config
                .read()
                .await
                .reassess_degraded_interval_ms
                .max(1000);
            let mut tick = interval(Duration::from_millis(interval_ms));
            tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                reassess_control
                    .reassess_degraded_fans(&reassess_events)
                    .await;
            }
        });
    }
}
