use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use kde_fan_control_core::config::AppConfig;
use kde_fan_control_core::inventory::{InventorySnapshot, discover, discover_from};
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;
use zbus::{connection::Builder, interface};

const BUS_NAME: &str = "org.kde.FanControl";
const BUS_PATH: &str = "/org/kde/FanControl";

#[derive(Parser)]
#[command(name = "kde-fan-control-daemon")]
#[command(about = "Daemon for KDE Fan Control")]
struct DaemonArgs {
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    session_bus: bool,
}

struct InventoryIface {
    snapshot: Arc<RwLock<InventorySnapshot>>,
    config: Arc<RwLock<AppConfig>>,
}

#[interface(name = "org.kde.FanControl.Inventory")]
impl InventoryIface {
    async fn snapshot(&self) -> zbus::fdo::Result<String> {
        let snapshot = self.snapshot.read().await;
        serde_json::to_string(&*snapshot)
            .map_err(|e| zbus::fdo::Error::Failed(format!("serialization error: {e}")))
    }

    async fn set_sensor_name(&self, id: &str, name: &str) -> zbus::fdo::Result<()> {
        {
            let mut config = self.config.write().await;
            config.set_sensor_name(id, name.to_string());
            config
                .save()
                .map_err(|e| zbus::fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }

    async fn set_fan_name(&self, id: &str, name: &str) -> zbus::fdo::Result<()> {
        {
            let mut config = self.config.write().await;
            config.set_fan_name(id, name.to_string());
            config
                .save()
                .map_err(|e| zbus::fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }

    async fn remove_sensor_name(&self, id: &str) -> zbus::fdo::Result<()> {
        {
            let mut config = self.config.write().await;
            config.remove_sensor_name(id);
            config
                .save()
                .map_err(|e| zbus::fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }

    async fn remove_fan_name(&self, id: &str) -> zbus::fdo::Result<()> {
        {
            let mut config = self.config.write().await;
            config.remove_fan_name(id);
            config
                .save()
                .map_err(|e| zbus::fdo::Error::Failed(format!("config save error: {e}")))?;
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

    let iface = InventoryIface { snapshot, config };

    let builder = if args.session_bus {
        Builder::session()?
    } else {
        Builder::system()?
    };

    let _connection = builder
        .name(BUS_NAME)?
        .serve_at(BUS_PATH, iface)?
        .build()
        .await?;

    tracing::info!(
        name = BUS_NAME,
        path = BUS_PATH,
        "D-Bus inventory surface ready"
    );

    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");

    Ok(())
}
