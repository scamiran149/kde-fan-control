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
        Command::State { format } => {
            let json = run_async(async {
                let proxy = connect_lifecycle_proxy().await?;
                Ok(proxy.get_runtime_state().await?)
            })?;
            match format {
                OutputFormat::Json => println!("{}", json),
                OutputFormat::Text => print_runtime_state(&json),
            }
        }
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
            // The DBus returns rejected as an array of [fan_id, error_object]
            // or as a tagged enum object depending on serialization.
            let fan_id = rejection
                .get("0")
                .or_else(|| rejection.get("fan_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let reason = rejection
                .get("1")
                .or_else(|| rejection.get("reason"))
                .and_then(|v| v.as_str());
            let kind = rejection
                .get("1")
                .and_then(|v| v.get("kind"))
                .and_then(|v| v.as_str());

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
                        let support = rejection
                            .get("1")
                            .and_then(|v| v.get("support_state"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        println!(
                            "  ✗ {}: fan is not enrollable (support state: {})",
                            fan_id, support
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

/// Print the runtime state from the daemon, showing managed, degraded,
/// fallback, and unmanaged fan statuses with clear lifecycle context.
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

            println!("=== FAN LIFECYCLE STATE ===");

            // Owned fans list
            if let Some(owned) = obj.get("owned_fans").and_then(|v| v.as_array()) {
                if owned.is_empty() {
                    println!("Daemon-owned fans: none (no fans under active management)");
                } else {
                    println!("Daemon-owned fans (actively controlled):");
                    for fan_id in owned {
                        println!("  • {}", fan_id);
                    }
                }
            }

            // Per-fan statuses
            if let Some(statuses) = obj.get("fan_statuses").and_then(|v| v.as_object()) {
                if statuses.is_empty() {
                    println!("\nFan statuses: no fans detected.");
                } else {
                    println!("\nFan statuses:");

                    // Group by status kind for easier scanning
                    let mut managed = Vec::new();
                    let mut degraded = Vec::new();
                    let mut fallback = Vec::new();
                    let mut unmanaged = Vec::new();

                    for (fan_id, status) in statuses {
                        let status_kind = status
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");

                        match status_kind {
                            "managed" => managed.push((fan_id.clone(), status.clone())),
                            "degraded" => degraded.push((fan_id.clone(), status.clone())),
                            "fallback" => fallback.push((fan_id.clone(), status.clone())),
                            "unmanaged" => unmanaged.push((fan_id.clone(), status.clone())),
                            _ => unmanaged.push((fan_id.clone(), status.clone())),
                        }
                    }

                    for (fan_id, status) in &managed {
                        let mode = status
                            .get("control_mode")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        println!("  ✓ {} — managed (control mode: {})", fan_id, mode);
                    }
                    for (fan_id, status) in &degraded {
                        let reasons = status.get("reasons").and_then(|v| v.as_array());
                        println!("  ⚠ {} — DEGRADED", fan_id);
                        if let Some(reasons) = reasons {
                            for reason in reasons {
                                println!("    • {}", format_degraded_reason(reason));
                            }
                        }
                        println!(
                            "    This fan was previously managed but cannot currently be controlled safely."
                        );
                    }
                    for (fan_id, _status) in &fallback {
                        println!("  ↗ {} — FALLBACK (driven to safe maximum speed)", fan_id);
                        println!(
                            "    The daemon set this fan to maximum speed before shutting down."
                        );
                    }
                    for (fan_id, _status) in &unmanaged {
                        println!("  – {} — unmanaged (under BIOS/auto control)", fan_id);
                    }
                }
            }

            println!();
            println!(
                "Use 'degraded' for detailed degraded-state reasons, 'events' for recent lifecycle history."
            );
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
