use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use kde_fan_control_core::inventory::{
    FanChannel, InventorySnapshot, TemperatureSensor, discover, discover_from,
};
use zbus::proxy;

#[derive(Parser)]
#[command(name = "kde-fan-control")]
#[command(about = "Inspect and manage fan-control hardware and lifecycle")]
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
    /// Show the current draft configuration.
    Draft,
    /// Show the current applied configuration.
    Applied,
    /// Show the current degraded-state summary.
    Degraded,
    /// Show recent lifecycle events.
    Events,
    /// Stage a fan enrollment change in the draft configuration.
    Enroll {
        /// Stable fan ID to enroll.
        fan_id: String,
        /// Whether the fan should be managed by the daemon.
        #[arg(long, default_value_t = true)]
        managed: bool,
        /// Control mode for the fan (pwm, voltage, or empty for none).
        #[arg(long, default_value = "none")]
        control_mode: String,
        /// Temperature source IDs for this fan's control loop.
        #[arg(long, num_args = 0.., value_delimiter = ',')]
        temp_sources: Vec<String>,
    },
    /// Remove a fan from the draft configuration.
    Unenroll {
        /// Stable fan ID to remove from the draft.
        fan_id: String,
    },
    /// Discard the entire draft configuration.
    Discard,
    /// Validate the current draft without applying it.
    Validate,
    /// Apply the current draft configuration.
    Apply,
    /// Show the current runtime state (managed, degraded, fallback, unmanaged).
    State,
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

#[proxy(
    interface = "org.kde.FanControl.Lifecycle",
    default_path = "/org/kde/FanControl/Lifecycle",
    default_service = "org.kde.FanControl"
)]
trait LifecycleProxy {
    fn get_draft_config(&self) -> zbus::Result<String>;
    fn get_applied_config(&self) -> zbus::Result<String>;
    fn get_degraded_summary(&self) -> zbus::Result<String>;
    fn get_lifecycle_events(&self) -> zbus::Result<String>;
    fn get_runtime_state(&self) -> zbus::Result<String>;
    fn set_draft_fan_enrollment(
        &self,
        fan_id: &str,
        managed: bool,
        control_mode: &str,
        temp_sources: &[&str],
    ) -> zbus::Result<String>;
    fn remove_draft_fan(&self, fan_id: &str) -> zbus::Result<()>;
    fn discard_draft(&self) -> zbus::Result<()>;
    fn validate_draft(&self) -> zbus::Result<String>;
    fn apply_draft(&self) -> zbus::Result<String>;
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
                let proxy = connect_inventory_proxy().await?;
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
                let proxy = connect_inventory_proxy().await?;
                if fan {
                    proxy.remove_fan_name(&id).await?;
                } else {
                    proxy.remove_sensor_name(&id).await?;
                }
                Ok(())
            })?;
            println!("removed name for {}", id);
        }
        Command::Draft => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_draft_config().await?)
            })?;
            print_json_or_text(&json, "No draft configuration.");
        }
        Command::Applied => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_applied_config().await?)
            })?;
            print_json_or_text(&json, "No applied configuration.");
        }
        Command::Degraded => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_degraded_summary().await?)
            })?;
            print_json_or_text(&json, "No degraded fans.");
        }
        Command::Events => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_lifecycle_events().await?)
            })?;
            print_json_or_text(&json, "No lifecycle events recorded.");
        }
        Command::Enroll {
            fan_id,
            managed,
            control_mode,
            temp_sources,
        } => {
            let temp_slices: Vec<&str> = temp_sources.iter().map(|s| s.as_str()).collect();
            let result = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy
                    .set_draft_fan_enrollment(&fan_id, managed, &control_mode, &temp_slices)
                    .await?)
            })?;
            println!("Draft updated. Current draft configuration:");
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(&result)?)?
            );
        }
        Command::Unenroll { fan_id } => {
            run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                proxy.remove_draft_fan(&fan_id).await?;
                Ok(())
            })?;
            println!("Removed {} from draft configuration.", fan_id);
        }
        Command::Discard => {
            run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                proxy.discard_draft().await?;
                Ok(())
            })?;
            println!("Draft configuration discarded.");
        }
        Command::Validate => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.validate_draft().await?)
            })?;
            print_validation_result(&json)?;
        }
        Command::Apply => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.apply_draft().await?)
            })?;
            print_validation_result(&json)?;
        }
        Command::State => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_runtime_state().await?)
            })?;
            print_runtime_state(&json);
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

async fn connect_inventory_proxy() -> zbus::Result<InventoryProxyProxy<'static>> {
    let connection = connect_dbus().await?;
    InventoryProxyProxy::new(&connection).await
}

async fn connect_lifecycle_proxy() -> zbus::Result<LifecycleProxyProxy<'static>> {
    let connection = connect_dbus().await?;
    LifecycleProxyProxy::new(&connection).await
}

async fn connect_dbus() -> zbus::Result<zbus::Connection> {
    match zbus::connection::Builder::session()?.build().await {
        Ok(c) => Ok(c),
        Err(_) => zbus::connection::Builder::system()?.build().await,
    }
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
        let proxy = connect_inventory_proxy().await?;
        let json_str = tokio::time::timeout(Duration::from_secs(5), proxy.snapshot())
            .await
            .map_err(|_| -> zbus::Error {
                zbus::Error::Address("daemon did not respond within 5 seconds".into())
            })??;
        let snapshot: InventorySnapshot = serde_json::from_str(&json_str)?;
        Ok(snapshot)
    })
}

/// Print a JSON string from the daemon, with a fallback message if it's
/// empty or null. Handles access-denied errors with a user-actionable message.
fn print_json_or_text(json: &str, empty_message: &str) {
    if json == "null" {
        println!("{}", empty_message);
        return;
    }
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or(json.to_string())
        ),
        Err(_) => println!("{}", json),
    }
}

/// Print a validation result from the daemon, showing which fans were
/// enrollable and which were rejected with reasons.
fn print_validation_result(json: &str) -> Result<(), Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    let enrollable = value
        .get("enrollable")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let rejected = value
        .get("rejected")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if enrollable.is_empty() && rejected.is_empty() {
        println!("Draft is empty — nothing to validate.");
        return Ok(());
    }

    if !enrollable.is_empty() {
        println!("Enrollable fans:");
        for fan_id in enrollable {
            println!("  ✓ {}", fan_id);
        }
    }

    if !rejected.is_empty() {
        println!("Rejected fans:");
        for rejection in rejected {
            let fan_id = rejection
                .get("0")
                .or_else(|| rejection.get("fan_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let reason = rejection
                .get("1")
                .or_else(|| rejection.get("reason"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown reason");
            println!("  ✗ {}: {}", fan_id, reason);
        }
    }

    Ok(())
}

/// Print the runtime state from the daemon, showing managed, degraded,
/// fallback, and unmanaged fan statuses.
fn print_runtime_state(json: &str) {
    if json == "null" {
        println!("No runtime state available.");
        return;
    }

    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => {
            let obj = match value.as_object() {
                Some(o) => o,
                None => {
                    println!("{}", json);
                    return;
                }
            };

            // Owned fans list
            if let Some(owned) = obj.get("owned_fans").and_then(|v| v.as_array()) {
                if owned.is_empty() {
                    println!("Owned fans: none");
                } else {
                    println!("Owned fans:");
                    for fan_id in owned {
                        println!("  • {}", fan_id);
                    }
                }
            }

            // Per-fan statuses
            if let Some(statuses) = obj.get("fan_statuses").and_then(|v| v.as_object()) {
                if statuses.is_empty() {
                    println!("Fan statuses: none");
                } else {
                    println!("\nFan statuses:");
                    for (fan_id, status) in statuses {
                        let status_obj = status.as_object();
                        let status_kind = status
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");

                        match status_kind {
                            "managed" => {
                                let mode = status_obj
                                    .and_then(|o| o.get("control_mode"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("?");
                                println!("  ✓ {} — managed (mode: {})", fan_id, mode);
                            }
                            "degraded" => {
                                let reasons = status_obj
                                    .and_then(|o| o.get("reasons"))
                                    .and_then(|v| v.as_array());
                                println!("  ⚠ {} — degraded", fan_id);
                                if let Some(reasons) = reasons {
                                    for reason in reasons {
                                        let kind = reason
                                            .get("kind")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown");
                                        println!("    reason: {}", kind);
                                    }
                                }
                            }
                            "fallback" => {
                                println!("  ↗ {} — fallback (safe maximum)", fan_id);
                            }
                            "unmanaged" => {
                                println!("  – {} — unmanaged", fan_id);
                            }
                            _ => {
                                println!("  ? {} — {}", fan_id, status_kind);
                            }
                        }
                    }
                }
            }
        }
        Err(_) => println!("{}", json),
    }
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
