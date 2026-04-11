use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use kde_fan_control_core::inventory::{
    FanChannel, InventorySnapshot, TemperatureSensor, discover, discover_from,
};
use serde_json::{Value, json};
use zbus::proxy;

const TEMPERATURE_TARGET_PID_NOTE: &str =
    "v1 control is temperature-target PID, not RPM-target tracking.";

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
    Draft {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Show the current applied configuration.
    Applied {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Show the current degraded-state summary.
    Degraded {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Show recent lifecycle events.
    Events {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
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
    State {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        detail: bool,
    },
    /// Stage PID control profile changes in the draft configuration.
    Control {
        #[command(subcommand)]
        command: ControlCommand,
    },
    /// Start, inspect, or accept auto-tune proposals.
    AutoTune {
        #[command(subcommand)]
        command: AutoTuneCommand,
    },
}

#[derive(Subcommand)]
enum ControlCommand {
    /// Stage PID control settings for a managed fan.
    Set {
        fan_id: String,
        #[arg(long)]
        target_temp: f64,
        #[arg(long, value_enum)]
        aggregation: AggregationArg,
        #[arg(long)]
        kp: f64,
        #[arg(long)]
        ki: f64,
        #[arg(long)]
        kd: f64,
        #[arg(long)]
        sample_ms: u64,
        #[arg(long)]
        control_ms: u64,
        #[arg(long)]
        write_ms: u64,
        #[arg(long)]
        deadband_mc: Option<i64>,
    },
}

#[derive(Subcommand)]
enum AutoTuneCommand {
    /// Start a bounded auto-tune run for a managed fan.
    Start { fan_id: String },
    /// Inspect the latest auto-tune result for a fan.
    Result { fan_id: String },
    /// Accept the latest completed auto-tune proposal into draft config.
    Accept { fan_id: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum AggregationArg {
    Average,
    Max,
    Min,
    Median,
}

impl AggregationArg {
    fn as_wire_value(self) -> &'static str {
        match self {
            Self::Average => "average",
            Self::Max => "max",
            Self::Min => "min",
            Self::Median => "median",
        }
    }
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

#[proxy(
    interface = "org.kde.FanControl.Control",
    default_path = "/org/kde/FanControl/Control",
    default_service = "org.kde.FanControl"
)]
trait ControlProxy {
    fn get_control_status(&self) -> zbus::Result<String>;
    fn start_auto_tune(&self, fan_id: &str) -> zbus::Result<()>;
    fn get_auto_tune_result(&self, fan_id: &str) -> zbus::Result<String>;
    fn accept_auto_tune(&self, fan_id: &str) -> zbus::Result<String>;
    fn set_draft_fan_control_profile(
        &self,
        fan_id: &str,
        profile_json: &str,
    ) -> zbus::Result<String>;
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
        Command::Draft { format } => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_draft_config().await?)
            })?;
            match format {
                OutputFormat::Json => println!("{}", json),
                OutputFormat::Text => print_draft_config(&json),
            }
        }
        Command::Applied { format } => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_applied_config().await?)
            })?;
            match format {
                OutputFormat::Json => println!("{}", json),
                OutputFormat::Text => print_applied_config(&json),
            }
        }
        Command::Degraded { format } => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_degraded_summary().await?)
            })?;
            match format {
                OutputFormat::Json => println!("{}", json),
                OutputFormat::Text => print_degraded_summary(&json),
            }
        }
        Command::Events { format } => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_lifecycle_events().await?)
            })?;
            match format {
                OutputFormat::Json => println!("{}", json),
                OutputFormat::Text => print_lifecycle_events(&json),
            }
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
            println!(
                "✓ Staged enrollment change for '{}' (managed={}, mode={}).",
                fan_id, managed, control_mode
            );
            println!("  This change is in the DRAFT — it is NOT live until you run 'apply'.");
            println!();
            print_draft_config(&result);
        }
        Command::Unenroll { fan_id } => {
            run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                proxy.remove_draft_fan(&fan_id).await?;
                Ok(())
            })?;
            println!("✓ Removed '{}' from draft configuration.", fan_id);
            println!("  This change is in the DRAFT — it is NOT live until you run 'apply'.");
        }
        Command::Discard => {
            run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                proxy.discard_draft().await?;
                Ok(())
            })?;
            println!(
                "✓ Draft configuration discarded. No changes were applied to the live configuration."
            );
        }
        Command::Validate => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.validate_draft().await?)
            })?;
            print_validation_result(&json, "validate")?;
        }
        Command::Apply => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.apply_draft().await?)
            })?;
            print_validation_result(&json, "apply")?;
        }
        Command::State { format, detail } => {
            let merged = fetch_state_payload(detail)?;
            match format {
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&merged)?),
                OutputFormat::Text => print_runtime_state(&merged, detail),
            }
        }
        Command::Control { command } => match command {
            ControlCommand::Set {
                fan_id,
                target_temp,
                aggregation,
                kp,
                ki,
                kd,
                sample_ms,
                control_ms,
                write_ms,
                deadband_mc,
            } => {
                let payload = build_control_profile_payload(
                    target_temp,
                    aggregation,
                    kp,
                    ki,
                    kd,
                    sample_ms,
                    control_ms,
                    write_ms,
                    deadband_mc,
                );
                let payload_json = serde_json::to_string(&payload)?;
                run_async(async {
                    let proxy = connect_control_proxy().await?;
                    proxy
                        .set_draft_fan_control_profile(&fan_id, &payload_json)
                        .await?;
                    Ok(())
                })?;
                println!("✓ Staged control profile changes for '{}'.", fan_id);
                println!(
                    "  Target: {:.1} C ({} millidegrees), aggregation={}, gains=kp {:.3} / ki {:.3} / kd {:.3}.",
                    target_temp,
                    celsius_to_millidegrees(target_temp),
                    aggregation.as_wire_value(),
                    kp,
                    ki,
                    kd,
                );
                println!(
                    "  Cadence: sample={} ms, control={} ms, write={} ms{}.",
                    sample_ms,
                    control_ms,
                    write_ms,
                    deadband_mc
                        .map(|value| format!(", deadband={} millidegrees", value))
                        .unwrap_or_default(),
                );
                println!("  This change is STAGED only — run 'apply' to make it live.");
            }
        },
        Command::AutoTune { command } => match command {
            AutoTuneCommand::Start { fan_id } => {
                run_async(async {
                    let proxy = connect_control_proxy().await?;
                    proxy.start_auto_tune(&fan_id).await?;
                    Ok(())
                })?;
                println!("✓ Started auto-tune for '{}'.", fan_id);
                println!("  This run is time-bounded and reviewable before any gains are staged.");
                println!(
                    "  Use 'auto-tune result {}' to check progress and review any proposal.",
                    fan_id
                );
            }
            AutoTuneCommand::Result { fan_id } => {
                let json = run_async(async {
                    let proxy = connect_control_proxy().await?;
                    Ok(proxy.get_auto_tune_result(&fan_id).await?)
                })?;
                print_auto_tune_result(&fan_id, &json)?;
            }
            AutoTuneCommand::Accept { fan_id } => {
                run_async(async {
                    let proxy = connect_control_proxy().await?;
                    proxy.accept_auto_tune(&fan_id).await?;
                    Ok(())
                })?;
                println!("✓ Accepted the latest auto-tune proposal for '{}'.", fan_id);
                println!("  Tuned gains are staged in the draft configuration only.");
                println!("  Run 'apply' to make the staged gains live.");
            }
        },
    }

    Ok(())
}

fn run_async<F, R>(future: F) -> Result<R, Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = Result<R, zbus::Error>>,
{
    let rt = tokio::runtime::Runtime::new()?;
    Ok(rt.block_on(future).map_err(|e| {
        let msg = format!("{}", e);
        if msg.contains("AccessDenied") || msg.contains("Access denied") || msg.contains("privileged") || msg.contains("root") {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Access denied: lifecycle changes require root privileges. Run with sudo or as root.",
            )) as Box<dyn std::error::Error>
        } else {
            Box::new(e) as Box<dyn std::error::Error>
        }
    })?)
}

async fn connect_inventory_proxy() -> zbus::Result<InventoryProxyProxy<'static>> {
    let connection = connect_dbus().await?;
    InventoryProxyProxy::new(&connection).await
}

async fn connect_lifecycle_proxy() -> zbus::Result<LifecycleProxyProxy<'static>> {
    let connection = connect_dbus().await?;
    LifecycleProxyProxy::new(&connection).await
}

async fn connect_control_proxy() -> zbus::Result<ControlProxyProxy<'static>> {
    let connection = connect_dbus().await?;
    ControlProxyProxy::new(&connection).await
}

async fn connect_dbus() -> zbus::Result<zbus::Connection> {
    match zbus::connection::Builder::system()?.build().await {
        Ok(c) => Ok(c),
        Err(_) => {
            // System bus is the normal daemon location. Fall back to session bus
            // for local development runs that explicitly use `--session-bus`.
            zbus::connection::Builder::session()?.build().await
        }
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

fn fetch_state_payload(detail: bool) -> Result<Value, Box<dyn std::error::Error>> {
    run_async(async move {
        let connection = connect_dbus().await?;
        let lifecycle = LifecycleProxyProxy::new(&connection).await?;
        let control = ControlProxyProxy::new(&connection).await?;

        let runtime_state = lifecycle.get_runtime_state().await?;
        let control_status = control.get_control_status().await?;
        let applied_config = if detail {
            Some(lifecycle.get_applied_config().await?)
        } else {
            None
        };

        let runtime_value = serde_json::from_str::<Value>(&runtime_state)
            .map_err(|error| zbus::Error::Failure(format!("runtime state parse error: {error}")))?;
        let control_value = serde_json::from_str::<Value>(&control_status).map_err(|error| {
            zbus::Error::Failure(format!("control status parse error: {error}"))
        })?;
        let applied_value = match applied_config {
            Some(json) => Some(serde_json::from_str::<Value>(&json).map_err(|error| {
                zbus::Error::Failure(format!("applied config parse error: {error}"))
            })?),
            None => None,
        };

        let fan_ids = runtime_value
            .get("fan_statuses")
            .and_then(Value::as_object)
            .map(|statuses| statuses.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let mut auto_tune = serde_json::Map::new();
        for fan_id in fan_ids {
            let result = control.get_auto_tune_result(&fan_id).await?;
            let value = serde_json::from_str::<Value>(&result).map_err(|error| {
                zbus::Error::Failure(format!(
                    "auto-tune result parse error for {fan_id}: {error}"
                ))
            })?;
            auto_tune.insert(fan_id, value);
        }

        merge_runtime_payload(
            runtime_value,
            control_value,
            applied_value,
            Value::Object(auto_tune),
        )
        .map_err(|error| zbus::Error::Failure(format!("runtime merge error: {error}")))
    })
}

fn merge_runtime_payload(
    mut runtime_state: Value,
    control_status: Value,
    applied_config: Option<Value>,
    auto_tune_results: Value,
) -> Result<Value, String> {
    let fan_statuses = runtime_state
        .get_mut("fan_statuses")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "runtime state missing fan_statuses".to_string())?;
    let control_map = control_status.as_object();
    let auto_tune_map = auto_tune_results.as_object();
    let applied_fans = applied_config
        .as_ref()
        .and_then(|value| value.get("fans"))
        .and_then(Value::as_object);

    for (fan_id, status) in fan_statuses.iter_mut() {
        if let Some(control) = control_map.and_then(|map| map.get(fan_id)) {
            if let Some(status_obj) = status.as_object_mut() {
                status_obj.insert("control".to_string(), control.clone());
            }
        }
        if let Some(auto_tune) = auto_tune_map.and_then(|map| map.get(fan_id)) {
            if let Some(status_obj) = status.as_object_mut() {
                status_obj.insert("auto_tune".to_string(), auto_tune.clone());
            }
        }
        if let Some(profile) = applied_fans.and_then(|fans| fans.get(fan_id)) {
            if let Some(status_obj) = status.as_object_mut() {
                status_obj.insert("control_profile".to_string(), profile.clone());
            }
        }
    }

    let root = runtime_state
        .as_object_mut()
        .ok_or_else(|| "runtime state root was not an object".to_string())?;
    root.insert(
        "control_model_note".to_string(),
        Value::String(TEMPERATURE_TARGET_PID_NOTE.to_string()),
    );

    Ok(runtime_state)
}

fn build_control_profile_payload(
    target_temp_celsius: f64,
    aggregation: AggregationArg,
    kp: f64,
    ki: f64,
    kd: f64,
    sample_ms: u64,
    control_ms: u64,
    write_ms: u64,
    deadband_mc: Option<i64>,
) -> Value {
    let mut payload = json!({
        "target_temp_millidegrees": celsius_to_millidegrees(target_temp_celsius),
        "aggregation": aggregation.as_wire_value(),
        "pid_gains": {
            "kp": kp,
            "ki": ki,
            "kd": kd,
        },
        "cadence": {
            "sample_interval_ms": sample_ms,
            "control_interval_ms": control_ms,
            "write_interval_ms": write_ms,
        },
    });

    if let Some(deadband_millidegrees) = deadband_mc {
        payload["deadband_millidegrees"] = Value::from(deadband_millidegrees);
    }

    payload
}

fn celsius_to_millidegrees(value: f64) -> i64 {
    (value * 1000.0).round() as i64
}

/// Print the draft configuration with clear STAGED (not yet applied) labeling.
fn print_draft_config(json: &str) {
    if json == "null" || json == "{\"fans\":{}}" {
        println!("Draft configuration is empty.");
        println!("Use 'enroll' to stage fan enrollment changes, then 'apply' to make them live.");
        return;
    }

    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => {
            println!("=== DRAFT CONFIGURATION (staged, NOT yet applied) ===");
            let fans = value.get("fans").and_then(|v| v.as_object());
            match fans {
                Some(fans) if fans.is_empty() => {
                    println!("  No fan entries in draft.");
                }
                Some(fans) => {
                    for (fan_id, entry) in fans {
                        let managed = entry
                            .get("managed")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let mode = entry
                            .get("control_mode")
                            .and_then(|v| v.as_str())
                            .unwrap_or("none");
                        let temps = entry
                            .get("temp_sources")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default();

                        let status = if managed { "MANAGED" } else { "UNMANAGED" };
                        let mode_display = if mode.is_empty() || mode == "none" {
                            "no mode set".to_string()
                        } else {
                            format!("mode={}", mode)
                        };
                        let temp_display = if temps.is_empty() {
                            String::new()
                        } else {
                            format!(" | temp_sources=[{}]", temps)
                        };

                        println!("  {} [{}] {}{}", fan_id, status, mode_display, temp_display);
                    }
                }
                None => {
                    println!("  No fan entries in draft.");
                }
            }
            println!();
            println!("Changes above are STAGED. Run 'apply' to promote to the live configuration.");
        }
        Err(_) => println!("{}", json),
    }
}

/// Print the applied (live) configuration with clear labeling.
fn print_applied_config(json: &str) {
    if json == "null" {
        println!("No applied configuration — no fans are currently managed.");
        println!("Use 'enroll' to stage changes, then 'apply' to make them live.");
        return;
    }

    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => {
            let fans = value.get("fans").and_then(|v| v.as_object());
            let applied_at = value
                .get("applied_at")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            match fans {
                Some(fans) if fans.is_empty() => {
                    println!("=== APPLIED CONFIGURATION (live) ===");
                    println!("  No fans are currently managed.");
                }
                Some(fans) => {
                    println!(
                        "=== APPLIED CONFIGURATION (live, applied at {}) ===",
                        applied_at
                    );
                    for (fan_id, entry) in fans {
                        let mode = entry
                            .get("control_mode")
                            .and_then(|v| v.as_str())
                            .unwrap_or("none");
                        let temps = entry
                            .get("temp_sources")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default();

                        let temp_display = if temps.is_empty() {
                            String::new()
                        } else {
                            format!(" | temp_sources=[{}]", temps)
                        };

                        println!("  {} [MANAGED, mode={}]{}", fan_id, mode, temp_display);
                    }
                }
                None => {
                    println!("=== APPLIED CONFIGURATION (live) ===");
                    println!("  No fan entries.");
                }
            }
        }
        Err(_) => println!("{}", json),
    }
}

/// Print the degraded state summary with human-readable reasons.
fn print_degraded_summary(json: &str) {
    if json == "null" || json == "{\"entries\":{}}" {
        println!("No degraded fans — all enrolled fans are healthy.");
        return;
    }

    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => {
            let entries = value.get("entries").and_then(|v| v.as_object());
            match entries {
                Some(entries) if entries.is_empty() => {
                    println!("No degraded fans — all enrolled fans are healthy.");
                }
                Some(entries) => {
                    println!("=== DEGRADED FANS ===");
                    for (fan_id, reasons) in entries {
                        let reason_list = reasons
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .map(|r| format_degraded_reason(r))
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            })
                            .unwrap_or_else(|| "unknown reason".to_string());
                        println!("  ⚠ {}: {}", fan_id, reason_list);
                    }
                }
                None => {
                    println!("No degraded fans — all enrolled fans are healthy.");
                }
            }
        }
        Err(_) => println!("{}", json),
    }
}

/// Format a single degraded reason into a human-readable string.
fn format_degraded_reason(reason: &serde_json::Value) -> String {
    let kind = reason
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    match kind {
        "fan_missing" => {
            let fan_id = reason
                .get("fan_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("fan missing from hardware (was: {})", fan_id)
        }
        "fan_no_longer_enrollable" => {
            let fan_id = reason
                .get("fan_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let support = reason
                .get("support_state")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let desc = reason.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            format!(
                "no longer enrollable — {} (state: {}, reason: {})",
                fan_id, support, desc
            )
        }
        "control_mode_unavailable" => {
            let fan_id = reason
                .get("fan_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let mode = reason
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!(
                "{} — configured control mode '{}' no longer supported",
                fan_id, mode
            )
        }
        "temp_source_missing" => {
            let fan_id = reason
                .get("fan_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let temp_id = reason
                .get("temp_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("{} — temperature source '{}' missing", fan_id, temp_id)
        }
        "partial_boot_recovery" => {
            let failed = reason
                .get("failed_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let recovered = reason
                .get("recovered_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!(
                "partial boot recovery: {} recovered, {} failed",
                recovered, failed
            )
        }
        "fallback_active" => {
            let fans = reason
                .get("affected_fans")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            format!("fallback active for fans: {}", fans)
        }
        _ => format!("unknown degraded reason: {}", kind),
    }
}

/// Print lifecycle events as a readable log with timestamps and descriptions.
fn print_lifecycle_events(json: &str) {
    if json == "null" || json == "[]" {
        println!("No lifecycle events recorded.");
        return;
    }

    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(value) => {
            let events = value.as_array();
            match events {
                Some(events) if events.is_empty() => {
                    println!("No lifecycle events recorded.");
                }
                Some(events) => {
                    println!("=== LIFECYCLE EVENTS (most recent last) ===");
                    for event in events {
                        let timestamp = event
                            .get("timestamp")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown time");
                        let reason = event.get("reason");
                        let detail = event.get("detail").and_then(|v| v.as_str()).unwrap_or("");

                        let reason_desc = reason
                            .map(|r| format_degraded_reason(r))
                            .unwrap_or_else(|| "unknown event".to_string());

                        print!("  [{}] {}", timestamp, reason_desc);
                        if !detail.is_empty() {
                            print!(" — {}", detail);
                        }
                        println!();
                    }
                }
                None => {
                    println!("No lifecycle events recorded.");
                }
            }
        }
        Err(_) => println!("{}", json),
    }
}

/// Print a validation result from the daemon, with context for validate vs apply.
fn print_validation_result(json: &str, context: &str) -> Result<(), Box<dyn std::error::Error>> {
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
        println!("Draft is empty — nothing to {}.", context);
        return Ok(());
    }

    match context {
        "validate" => println!("=== DRAFT VALIDATION RESULTS ==="),
        "apply" => println!("=== APPLY RESULTS ==="),
        _ => {}
    }

    if !enrollable.is_empty() {
        if context == "apply" {
            println!("Promoted to APPLIED configuration (now live):");
        } else {
            println!("Enrollable (would be promoted on apply):");
        }
        for fan_id in &enrollable {
            let id = fan_id.as_str().unwrap_or("unknown");
            println!("  ✓ {}", id);
        }
    }

    if !rejected.is_empty() {
        if context == "apply" {
            println!("Rejected (NOT promoted, still in draft):");
        } else {
            println!("Rejected (would block promotion on apply):");
        }
        for rejection in &rejected {
            let (fan_id, reason, kind, support) = parse_rejection_entry(rejection);

            if let Some(reason) = reason {
                println!("  ✗ {}: {}", fan_id, reason);
            } else if let Some(kind) = kind {
                match kind {
                    "fan_not_found" => {
                        println!(
                            "  ✗ {}: fan not found in current hardware inventory",
                            fan_id
                        );
                    }
                    "fan_not_enrollable" => {
                        println!(
                            "  ✗ {}: fan is not enrollable (support state: {})",
                            fan_id,
                            support.unwrap_or("unknown")
                        );
                    }
                    "unsupported_control_mode" => {
                        println!(
                            "  ✗ {}: requested control mode not supported by this fan",
                            fan_id
                        );
                    }
                    "missing_control_mode" => {
                        println!("  ✗ {}: no control mode selected for managed fan", fan_id);
                    }
                    "temp_source_not_found" => {
                        println!("  ✗ {}: referenced temperature source not found", fan_id);
                    }
                    _ => {
                        println!("  ✗ {}: {} (unknown rejection kind)", fan_id, kind);
                    }
                }
            } else {
                println!("  ✗ {}: rejected", fan_id);
            }
        }
    }

    if context == "apply" && !enrollable.is_empty() {
        println!();
        println!("Changes are now LIVE. Use 'state' to see current fan statuses.");
    } else if context == "apply" && enrollable.is_empty() && !rejected.is_empty() {
        println!();
        println!(
            "No fans were promoted. Fix the issues above and re-try, or use 'degraded' to inspect."
        );
    } else if context != "apply" && !rejected.is_empty() {
        println!();
        println!("Use 'apply' to promote the enrollable fans. Rejected fans will remain in draft.");
    }

    Ok(())
}

fn parse_rejection_entry<'a>(
    rejection: &'a Value,
) -> (&'a str, Option<&'a str>, Option<&'a str>, Option<&'a str>) {
    let tuple = rejection.as_array();
    let detail = tuple
        .and_then(|v| v.get(1))
        .or_else(|| rejection.get("reason"));
    let fan_id = tuple
        .and_then(|v| v.first())
        .or_else(|| rejection.get("fan_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let reason = detail.and_then(|v| v.as_str());
    let kind = detail.and_then(|v| v.get("kind")).and_then(|v| v.as_str());
    let support = detail
        .and_then(|v| v.get("support_state"))
        .and_then(|v| v.as_str());

    (fan_id, reason, kind, support)
}

/// Print the runtime state from the daemon, showing managed, degraded,
/// fallback, and unmanaged fan statuses with clear lifecycle context.
fn print_runtime_state(value: &Value, detail: bool) {
    if value.is_null() {
        println!("No runtime state available.");
        return;
    }

    println!("{}", render_runtime_state_text(value, detail));
}

fn render_runtime_state_text(value: &Value, detail: bool) -> String {
    let Some(obj) = value.as_object() else {
        return value.to_string();
    };

    let mut lines = vec!["=== FAN RUNTIME STATE ===".to_string()];

    if let Some(owned) = obj.get("owned_fans").and_then(Value::as_array) {
        if owned.is_empty() {
            lines.push("Daemon-owned fans: none".to_string());
        } else {
            lines.push(format!(
                "Daemon-owned fans: {}",
                owned
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    let Some(statuses) = obj.get("fan_statuses").and_then(Value::as_object) else {
        lines.push(TEMPERATURE_TARGET_PID_NOTE.to_string());
        return lines.join("\n");
    };

    if statuses.is_empty() {
        lines.push("No fans detected.".to_string());
        lines.push(TEMPERATURE_TARGET_PID_NOTE.to_string());
        return lines.join("\n");
    }

    lines.push("".to_string());
    lines.push("Fan statuses:".to_string());

    let mut fan_ids = statuses.keys().cloned().collect::<Vec<_>>();
    fan_ids.sort();

    for fan_id in fan_ids {
        let status = &statuses[&fan_id];
        lines.push(render_runtime_summary_line(&fan_id, status));
        if detail {
            lines.extend(render_runtime_detail_lines(status));
        }
    }

    lines.push("".to_string());
    lines.push(TEMPERATURE_TARGET_PID_NOTE.to_string());
    lines.push("Use 'degraded' for detailed degraded-state reasons, 'events' for recent lifecycle history.".to_string());
    lines.join("\n")
}

fn render_runtime_summary_line(fan_id: &str, status: &Value) -> String {
    let state = status
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let control = status.get("control");
    let auto_tune = status.get("auto_tune");

    format!(
        "  {} — {} | temp={} | target={} | output={} | pwm={} | auto-tune={}",
        fan_id,
        state_label(state),
        control
            .and_then(|value| value.get("aggregated_temp_millidegrees"))
            .map(format_millidegrees_value)
            .unwrap_or_else(|| "n/a".to_string()),
        control
            .and_then(|value| value.get("target_temp_millidegrees"))
            .map(format_millidegrees_value)
            .unwrap_or_else(|| "n/a".to_string()),
        control
            .and_then(|value| value.get("logical_output_percent"))
            .map(format_percent_value)
            .unwrap_or_else(|| "n/a".to_string()),
        control
            .and_then(|value| value.get("mapped_pwm"))
            .and_then(Value::as_u64)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_string()),
        auto_tune_state_label(control, auto_tune),
    )
}

fn render_runtime_detail_lines(status: &Value) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(control) = status.get("control") {
        let sensors = control
            .get("sensor_ids")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "none".to_string());
        let aggregation = control
            .get("aggregation")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let high_temp = control
            .get("alert_high_temp")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        lines.push(format!("      sensors: {}", sensors));
        lines.push(format!("      aggregation: {}", aggregation));
        lines.push(format!(
            "      high-temp alert: {}",
            if high_temp { "yes" } else { "no" }
        ));
    }

    if let Some(profile) = status.get("control_profile") {
        if let Some(gains) = profile.get("pid_gains") {
            lines.push(format!(
                "      gains: kp={} ki={} kd={}",
                format_float_value(gains.get("kp")),
                format_float_value(gains.get("ki")),
                format_float_value(gains.get("kd")),
            ));
        }
        if let Some(cadence) = profile.get("cadence") {
            lines.push(format!(
                "      cadence: sample={} ms, control={} ms, write={} ms",
                cadence
                    .get("sample_interval_ms")
                    .and_then(Value::as_u64)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
                cadence
                    .get("control_interval_ms")
                    .and_then(Value::as_u64)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
                cadence
                    .get("write_interval_ms")
                    .and_then(Value::as_u64)
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
            ));
        }
        if let Some(deadband) = profile.get("deadband_millidegrees").and_then(Value::as_i64) {
            lines.push(format!("      deadband: {} millidegrees", deadband));
        }
    }

    if let Some(reasons) = status.get("reasons").and_then(Value::as_array) {
        for reason in reasons {
            lines.push(format!(
                "      degraded: {}",
                format_degraded_reason(reason)
            ));
        }
    }

    lines
}

fn state_label(status: &str) -> &'static str {
    match status {
        "managed" => "managed",
        "degraded" => "DEGRADED",
        "fallback" => "FALLBACK",
        "unmanaged" => "unmanaged",
        _ => "unknown",
    }
}

fn auto_tune_state_label(control: Option<&Value>, auto_tune: Option<&Value>) -> String {
    if let Some(status) = auto_tune
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
    {
        return status.replace('_', "-");
    }

    if control
        .and_then(|value| value.get("auto_tuning"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "running".to_string()
    } else {
        "idle".to_string()
    }
}

fn format_millidegrees_value(value: &Value) -> String {
    value
        .as_i64()
        .map(|raw| format!("{:.1} C", raw as f64 / 1000.0))
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_percent_value(value: &Value) -> String {
    value
        .as_f64()
        .map(|raw| format!("{raw:.1}%"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_float_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_f64)
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "n/a".to_string())
}

fn print_auto_tune_result(fan_id: &str, json: &str) -> Result<(), Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(json)?;
    println!("{}", render_auto_tune_result_text(fan_id, &value));
    Ok(())
}

fn render_auto_tune_result_text(fan_id: &str, value: &Value) -> String {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let observation_window_ms = value
        .get("observation_window_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let mut lines = vec![format!("=== AUTO-TUNE RESULT FOR {} ===", fan_id)];

    match status {
        "idle" => lines.push(format!(
            "State: idle (default observation window: {} ms)",
            observation_window_ms
        )),
        "running" => lines.push(format!(
            "State: running (bounded observation window: {} ms)",
            observation_window_ms
        )),
        "completed" => {
            lines.push(format!(
                "State: completed (observation window: {} ms)",
                observation_window_ms
            ));
            if let Some(proposal) = value.get("proposal") {
                let gains = proposal.get("proposed_gains").unwrap_or(&Value::Null);
                lines.push(format!(
                    "Proposed gains: Kp={} Ki={} Kd={}",
                    format_float_value(gains.get("kp")),
                    format_float_value(gains.get("ki")),
                    format_float_value(gains.get("kd")),
                ));
            }
            lines.push(
                "Use 'auto-tune accept <fan_id>' to stage these gains for review before apply."
                    .to_string(),
            );
        }
        "failed" => {
            let error = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            lines.push(format!(
                "State: failed (observation window: {} ms)",
                observation_window_ms
            ));
            lines.push(format!("Error: {}", error));
        }
        _ => lines.push(format!("State: {}", status)),
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_control_profile_payload_with_millidegree_conversion() {
        let payload = build_control_profile_payload(
            57.5,
            AggregationArg::Median,
            1.2,
            0.3,
            0.8,
            1000,
            2000,
            2500,
            Some(1200),
        );

        assert_eq!(payload["target_temp_millidegrees"], Value::from(57_500));
        assert_eq!(payload["aggregation"], Value::from("median"));
        assert_eq!(payload["pid_gains"]["kp"], Value::from(1.2));
        assert_eq!(payload["cadence"]["write_interval_ms"], Value::from(2500));
        assert_eq!(payload["deadband_millidegrees"], Value::from(1200));
    }

    #[test]
    fn merges_runtime_payload_and_renders_detail_note() {
        let runtime = json!({
            "owned_fans": ["fan0"],
            "fan_statuses": {
                "fan0": {
                    "status": "managed",
                    "control_mode": "pwm"
                }
            }
        });
        let control = json!({
            "fan0": {
                "sensor_ids": ["temp0", "temp1"],
                "aggregation": "max",
                "target_temp_millidegrees": 60000,
                "aggregated_temp_millidegrees": 55250,
                "logical_output_percent": 42.5,
                "mapped_pwm": 108,
                "auto_tuning": false,
                "alert_high_temp": false,
                "last_error_millidegrees": -4750
            }
        });
        let applied = Some(json!({
            "fans": {
                "fan0": {
                    "pid_gains": { "kp": 1.0, "ki": 0.2, "kd": 0.5 },
                    "cadence": {
                        "sample_interval_ms": 1000,
                        "control_interval_ms": 2000,
                        "write_interval_ms": 2000
                    },
                    "deadband_millidegrees": 1000
                }
            }
        }));
        let auto_tune = json!({
            "fan0": {
                "status": "completed",
                "observation_window_ms": 30000,
                "proposal": {
                    "proposed_gains": { "kp": 1.1, "ki": 0.4, "kd": 0.9 }
                }
            }
        });

        let merged = merge_runtime_payload(runtime, control, applied, auto_tune).unwrap();
        let text = render_runtime_state_text(&merged, true);

        assert!(text.contains(TEMPERATURE_TARGET_PID_NOTE));
        assert!(text.contains("fan0 — managed | temp=55.2 C | target=60.0 C | output=42.5% | pwm=108 | auto-tune=completed"));
        assert!(text.contains("sensors: temp0, temp1"));
        assert!(text.contains("gains: kp=1.000 ki=0.200 kd=0.500"));
        assert!(text.contains("cadence: sample=1000 ms, control=2000 ms, write=2000 ms"));
    }

    #[test]
    fn renders_completed_auto_tune_result_with_proposed_gains() {
        let value = json!({
            "status": "completed",
            "observation_window_ms": 30000,
            "proposal": {
                "proposed_gains": {
                    "kp": 1.234,
                    "ki": 0.456,
                    "kd": 0.789
                }
            }
        });

        let text = render_auto_tune_result_text("fan0", &value);

        assert!(text.contains("State: completed"));
        assert!(text.contains("Proposed gains: Kp=1.234 Ki=0.456 Kd=0.789"));
        assert!(text.contains("auto-tune accept <fan_id>"));
    }

    #[test]
    fn parses_tuple_shaped_rejection_entries() {
        let rejection = json!([
            "fan0",
            {
                "kind": "fan_not_enrollable",
                "support_state": "partial"
            }
        ]);

        let (fan_id, reason, kind, support) = parse_rejection_entry(&rejection);
        assert_eq!(fan_id, "fan0");
        assert_eq!(reason, None);
        assert_eq!(kind, Some("fan_not_enrollable"));
        assert_eq!(support, Some("partial"));
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
