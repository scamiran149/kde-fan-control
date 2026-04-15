//! Friendly-name management (rename/unname).
//!
//! Calls `Inventory.SetSensorName`, `Inventory.SetFanName`,
//! `Inventory.RemoveSensorName`, and `Inventory.RemoveFanName`
//! over DBus. These are the only commands that write to the
//! Inventory interface; all other inventory operations are
//! read-only.

use crate::InventoryProxyProxy;
use crate::run_async;

pub fn run_rename(id: &str, name: &str, fan: bool) -> Result<(), Box<dyn std::error::Error>> {
    run_async(async {
        let proxy = connect_inventory_proxy().await?;
        if fan {
            proxy.set_fan_name(id, name).await?;
        } else {
            proxy.set_sensor_name(id, name).await?;
        }
        Ok(())
    })?;
    println!("renamed {} to '{}'", id, name);
    Ok(())
}

pub fn run_unname(id: &str, fan: bool) -> Result<(), Box<dyn std::error::Error>> {
    run_async(async {
        let proxy = connect_inventory_proxy().await?;
        if fan {
            proxy.remove_fan_name(id).await?;
        } else {
            proxy.remove_sensor_name(id).await?;
        }
        Ok(())
    })?;
    println!("removed name for {}", id);
    Ok(())
}

async fn connect_inventory_proxy() -> zbus::Result<InventoryProxyProxy<'static>> {
    let connection = crate::connect_dbus().await?;
    InventoryProxyProxy::new(&connection).await
}
