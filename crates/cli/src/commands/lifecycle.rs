//! Lifecycle subcommands: draft, applied, degraded, events, enroll, unenroll, discard, validate, apply, auth.
//!
//! All commands in this module call methods on the
//! `org.kde.FanControl.Lifecycle` DBus interface. Write operations
//! (enroll, unenroll, discard, apply) require root authorization.

use serde_json::Value;

use crate::OutputFormat;
use crate::run_async;

pub fn run_draft(format: OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let json = run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        Ok(proxy.get_draft_config().await?)
    })?;
    match format {
        OutputFormat::Json => println!("{}", json),
        OutputFormat::Text => print_draft_config(&json),
    }
    Ok(())
}

pub fn run_applied(format: OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let json = run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        Ok(proxy.get_applied_config().await?)
    })?;
    match format {
        OutputFormat::Json => println!("{}", json),
        OutputFormat::Text => print_applied_config(&json),
    }
    Ok(())
}

pub fn run_degraded(format: OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let json = run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        Ok(proxy.get_degraded_summary().await?)
    })?;
    match format {
        OutputFormat::Json => println!("{}", json),
        OutputFormat::Text => print_degraded_summary(&json),
    }
    Ok(())
}

pub fn run_events(format: OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let json = run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        Ok(proxy.get_lifecycle_events().await?)
    })?;
    match format {
        OutputFormat::Json => println!("{}", json),
        OutputFormat::Text => print_lifecycle_events(&json),
    }
    Ok(())
}

pub fn run_enroll(
    fan_id: &str,
    managed: bool,
    control_mode: &str,
    temp_sources: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let temp_slices: Vec<&str> = temp_sources.iter().map(|s| s.as_str()).collect();
    let result = run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        Ok(proxy
            .set_draft_fan_enrollment(fan_id, managed, control_mode, &temp_slices)
            .await?)
    })?;
    println!(
        "✓ Staged enrollment change for '{}' (managed={}, mode={}).",
        fan_id, managed, control_mode
    );
    println!("  This change is in the DRAFT — it is NOT live until you run 'apply'.");
    println!();
    print_draft_config(&result);
    Ok(())
}

pub fn run_unenroll(fan_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        proxy.remove_draft_fan(fan_id).await?;
        Ok(())
    })?;
    println!("✓ Removed '{}' from draft configuration.", fan_id);
    println!("  This change is in the DRAFT — it is NOT live until you run 'apply'.");
    Ok(())
}

pub fn run_discard() -> Result<(), Box<dyn std::error::Error>> {
    run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        proxy.discard_draft().await?;
        Ok(())
    })?;
    println!("✓ Draft configuration discarded. No changes were applied to the live configuration.");
    Ok(())
}

pub fn run_validate() -> Result<(), Box<dyn std::error::Error>> {
    let json = run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        Ok(proxy.validate_draft().await?)
    })?;
    print_validation_result(&json, "validate")?;
    Ok(())
}

pub fn run_apply() -> Result<(), Box<dyn std::error::Error>> {
    let json = run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        Ok(proxy.apply_draft().await?)
    })?;
    print_validation_result(&json, "apply")?;
    Ok(())
}

pub fn run_auth() -> Result<(), Box<dyn std::error::Error>> {
    run_async(async {
        let proxy = connect_lifecycle_proxy().await?;
        proxy.request_authorization().await?;
        Ok(())
    })?;
    println!("✓ Authorization granted. You may now perform privileged operations.");
    Ok(())
}

async fn connect_lifecycle_proxy() -> zbus::Result<crate::LifecycleProxyProxy<'static>> {
    let connection = crate::connect_dbus().await?;
    crate::LifecycleProxyProxy::new(&connection).await
}

fn print_draft_config(json: &str) {
    if json == "null" || json == "{\"fans\":{}}" {
        println!("Draft configuration is empty.");
        println!("Use 'enroll' to stage fan enrollment changes, then 'apply' to make them live.");
        return;
    }

    match serde_json::from_str::<Value>(json) {
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

fn print_applied_config(json: &str) {
    if json == "null" {
        println!("No applied configuration — no fans are currently managed.");
        println!("Use 'enroll' to stage changes, then 'apply' to make them live.");
        return;
    }

    match serde_json::from_str::<Value>(json) {
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

fn print_degraded_summary(json: &str) {
    if json == "null" || json == "{\"entries\":{}}" {
        println!("No degraded fans — all enrolled fans are healthy.");
        return;
    }

    match serde_json::from_str::<Value>(json) {
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

fn print_lifecycle_events(json: &str) {
    if json == "null" || json == "[]" {
        println!("No lifecycle events recorded.");
        return;
    }

    match serde_json::from_str::<Value>(json) {
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

fn print_validation_result(json: &str, context: &str) -> Result<(), Box<dyn std::error::Error>> {
    let value: Value = serde_json::from_str(json)?;
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
