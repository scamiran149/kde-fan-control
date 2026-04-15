//! DBus Inventory interface implementation.
//!
//! Provides the `org.kde.FanControl.Inventory` DBus interface for read-only
//! hardware snapshots and writable friendly-name management. All write
//! methods require authorization via `require_authorized`.

use std::sync::Arc;

use kde_fan_control_core::config::AppConfig;
use kde_fan_control_core::inventory::InventorySnapshot;
use tokio::sync::RwLock;
use zbus::fdo;
use zbus::interface;

use crate::dbus::auth::require_authorized;
use crate::dbus::constants::MAX_NAME_LENGTH;

pub struct InventoryIface {
    pub snapshot: Arc<RwLock<InventorySnapshot>>,
    pub config: Arc<RwLock<AppConfig>>,
}

#[interface(name = "org.kde.FanControl.Inventory")]
impl InventoryIface {
    async fn snapshot(&self) -> fdo::Result<String> {
        let snapshot = self.snapshot.read().await;
        serde_json::to_string(&*snapshot)
            .map_err(|e| fdo::Error::Failed(format!("serialization error: {e}")))
    }

    async fn set_sensor_name(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
        name: &str,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;
        if id.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("id exceeds 128 characters".into()));
        }
        if name.is_empty() {
            return Err(fdo::Error::InvalidArgs("name must not be empty".into()));
        }
        if name.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs(
                "name exceeds 128 characters".into(),
            ));
        }
        {
            let mut config = self.config.write().await;
            config.set_sensor_name(id, name.to_string());
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }

    async fn set_fan_name(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
        name: &str,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;
        if id.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("id exceeds 128 characters".into()));
        }
        if name.is_empty() {
            return Err(fdo::Error::InvalidArgs("name must not be empty".into()));
        }
        if name.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs(
                "name exceeds 128 characters".into(),
            ));
        }
        {
            let mut config = self.config.write().await;
            config.set_fan_name(id, name.to_string());
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }

    async fn remove_sensor_name(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;
        if id.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("id exceeds 128 characters".into()));
        }
        {
            let mut config = self.config.write().await;
            config.remove_sensor_name(id);
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }

    async fn remove_fan_name(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
    ) -> fdo::Result<()> {
        require_authorized(connection, &header).await?;
        if id.len() > MAX_NAME_LENGTH {
            return Err(fdo::Error::InvalidArgs("id exceeds 128 characters".into()));
        }
        {
            let mut config = self.config.write().await;
            config.remove_fan_name(id);
            config
                .save()
                .map_err(|e| fdo::Error::Failed(format!("config save error: {e}")))?;
        }
        self.apply_names_to_snapshot().await;
        Ok(())
    }
}

impl InventoryIface {
    pub async fn apply_names_to_snapshot(&self) {
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
