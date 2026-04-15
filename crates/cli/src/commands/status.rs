//! Runtime state and control status display.
//!
//! Calls `Lifecycle.GetRuntimeState`, `Control.GetControlStatus`,
//! and optionally `Lifecycle.GetAppliedConfig` and per-fan
//! `Control.GetAutoTuneResult` to build a merged view of the
//! daemon's live fan status. Outputs text or JSON depending on
//! the `--format` flag.

use serde_json::Value;

use crate::ControlProxyProxy;
use crate::LifecycleProxyProxy;
use crate::OutputFormat;
use crate::TEMPERATURE_TARGET_PID_NOTE;
use crate::connect_dbus;
use crate::run_async;

pub fn run(format: OutputFormat, detail: bool) -> Result<(), Box<dyn std::error::Error>> {
    let merged = fetch_state_payload(detail)?;
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&merged)?),
        OutputFormat::Text => print_runtime_state(&merged, detail),
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::TEMPERATURE_TARGET_PID_NOTE;

    use super::*;

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
}
