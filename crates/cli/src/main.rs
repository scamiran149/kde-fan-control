use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use kde_fan_control_core::inventory::{
    FanChannel, InventorySnapshot, TemperatureSensor, discover, discover_from,
};
use zbus::proxy;

#[derive(Parser)]
#[command(name = "kde-fan-control")]
#[command(about = "Inspect discovered fan-control hardware")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Inventory {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        direct: bool,
    },
    Rename {
        id: String,
        name: String,
        #[arg(long, default_value_t = false)]
        fan: bool,
    },
    Unname {
        id: String,
        #[arg(long, default_value_t = false)]
        fan: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[proxy(
    interface = "org.kde.FanControl.Inventory",
    default_path = "/org/kde/FanControl",
    default_service = "org.kde.FanControl"
)]
trait InventoryProxy {
    fn snapshot(&self) -> zbus::Result<String>;
    fn set_sensor_name(&self, id: &str, name: &str) -> zbus::Result<()>;
    fn set_fan_name(&self, id: &str, name: &str) -> zbus::Result<()>;
    fn remove_sensor_name(&self, id: &str) -> zbus::Result<()>;
    fn remove_fan_name(&self, id: &str) -> zbus::Result<()>;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Inventory {
            format,
            root,
            direct,
        } => {
            let snapshot = if direct || root.is_some() {
                fetch_direct(&root)?
            } else {
                match fetch_dbus_snapshot() {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!(
                            "warning: could not reach daemon over D-Bus ({}), falling back to direct scan",
                            e
                        );
                        fetch_direct(&root)?
                    }
                }
            };

            match format {
                OutputFormat::Text => print_text(&snapshot),
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&snapshot)?),
            }
        }
        Command::Rename { id, name, fan } => {
            run_async(async {
                let proxy = connect_proxy().await?;
                if fan {
                    proxy.set_fan_name(&id, &name).await?;
                } else {
                    proxy.set_sensor_name(&id, &name).await?;
                }
                Ok(())
            })?;
            println!("renamed {} to '{}'", id, name);
        }
        Command::Unname { id, fan } => {
            run_async(async {
                let proxy = connect_proxy().await?;
                if fan {
                    proxy.remove_fan_name(&id).await?;
                } else {
                    proxy.remove_sensor_name(&id).await?;
                }
                Ok(())
            })?;
            println!("removed name for {}", id);
        }
    }

    Ok(())
}

fn run_async<F, R>(future: F) -> Result<R, Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = Result<R, zbus::Error>>,
{
    let rt = tokio::runtime::Runtime::new()?;
    Ok(rt.block_on(future)?)
}

async fn connect_proxy() -> zbus::Result<InventoryProxyProxy<'static>> {
    let connection = match zbus::connection::Builder::session()?.build().await {
        Ok(c) => c,
        Err(_) => zbus::connection::Builder::system()?.build().await?,
    };
    InventoryProxyProxy::new(&connection).await
}

fn fetch_direct(root: &Option<PathBuf>) -> Result<InventorySnapshot, Box<dyn std::error::Error>> {
    match root {
        Some(r) => discover_from(r).map_err(Into::into),
        None => discover().map_err(Into::into),
    }
}

fn fetch_dbus_snapshot() -> Result<InventorySnapshot, Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let proxy = connect_proxy().await?;
        let json_str = tokio::time::timeout(Duration::from_secs(5), proxy.snapshot())
            .await
            .map_err(|_| -> zbus::Error {
                zbus::Error::Address("daemon did not respond within 5 seconds".into())
            })??;
        let snapshot: InventorySnapshot = serde_json::from_str(&json_str)?;
        Ok(snapshot)
    })
}

fn print_text(snapshot: &InventorySnapshot) {
    if snapshot.devices.is_empty() {
        println!("No hwmon devices discovered.");
        return;
    }

    for device in &snapshot.devices {
        println!("{} [{}]", device.name, device.id);
        println!("  path: {}", device.sysfs_path);
        println!("  identity: {}", device.stable_identity);

        if device.temperatures.is_empty() {
            println!("  temperatures: none");
        } else {
            println!("  temperatures:");
            for sensor in &device.temperatures {
                print_temperature(sensor);
            }
        }

        if device.fans.is_empty() {
            println!("  fans: none");
        } else {
            println!("  fans:");
            for fan in &device.fans {
                print_fan(fan);
            }
        }
    }
}

fn print_temperature(sensor: &TemperatureSensor) {
    let label = sensor.label.as_deref().unwrap_or("unlabeled");
    let display_name = sensor.friendly_name.as_deref().unwrap_or(label);
    let value = sensor
        .input_millidegrees_celsius
        .map(|v| format!("{:.1} C", v as f64 / 1000.0))
        .unwrap_or_else(|| "unknown".to_string());

    print!(
        "    - temp{} [{}]: {} ({})",
        sensor.channel, sensor.id, display_name, value
    );
    if sensor.friendly_name.is_some() {
        print!(" [renamed from '{}']", label);
    }
    println!();
}

fn print_fan(fan: &FanChannel) {
    let label = fan.label.as_deref().unwrap_or("unlabeled");
    let display_name = fan.friendly_name.as_deref().unwrap_or(label);
    let rpm = fan
        .current_rpm
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let modes = if fan.control_modes.is_empty() {
        "none".to_string()
    } else {
        fan.control_modes
            .iter()
            .map(|mode| match mode {
                kde_fan_control_core::inventory::ControlMode::Pwm => "pwm",
                kde_fan_control_core::inventory::ControlMode::Voltage => "voltage",
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    print!(
        "    - fan{} [{}]: {} | rpm_feedback={} | rpm={} | modes={} | support={:?}",
        fan.channel, fan.id, display_name, fan.rpm_feedback, rpm, modes, fan.support_state,
    );
    if fan.friendly_name.is_some() {
        print!(" [renamed from '{}']", label);
    }
    println!();

    if let Some(reason) = &fan.support_reason {
        println!("      reason: {}", reason);
    }
}
