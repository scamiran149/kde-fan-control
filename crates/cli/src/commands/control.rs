//! Control profile and auto-tune commands.
//!
//! Calls `Control.SetDraftFanControlProfile` to stage PID
//! parameter changes, `Control.StartAutoTune` to begin a
//! bounded tuning run, `Control.GetAutoTuneResult` to inspect
//! progress, and `Control.AcceptAutoTune` to stage proposed
//! gains into the draft. All operations target the
//! `org.kde.FanControl.Control` DBus interface.

use serde_json::{Value, json};

use crate::AggregationArg;
use crate::ControlProxyProxy;
use crate::run_async;

pub fn run_control_set(
    fan_id: &str,
    target_temp: f64,
    aggregation: AggregationArg,
    kp: f64,
    ki: f64,
    kd: f64,
    sample_ms: u64,
    control_ms: u64,
    write_ms: u64,
    deadband_mc: Option<i64>,
) -> Result<(), Box<dyn std::error::Error>> {
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
            .set_draft_fan_control_profile(fan_id, &payload_json)
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
    Ok(())
}

pub fn run_auto_tune_start(fan_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    run_async(async {
        let proxy = connect_control_proxy().await?;
        proxy.start_auto_tune(fan_id).await?;
        Ok(())
    })?;
    println!("✓ Started auto-tune for '{}'.", fan_id);
    println!("  This run is time-bounded and reviewable before any gains are staged.");
    println!(
        "  Use 'auto-tune result {}' to check progress and review any proposal.",
        fan_id
    );
    Ok(())
}

pub fn run_auto_tune_result(fan_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let json = run_async(async {
        let proxy = connect_control_proxy().await?;
        Ok(proxy.get_auto_tune_result(fan_id).await?)
    })?;
    print_auto_tune_result(fan_id, &json)?;
    Ok(())
}

pub fn run_auto_tune_accept(fan_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    run_async(async {
        let proxy = connect_control_proxy().await?;
        proxy.accept_auto_tune(fan_id).await?;
        Ok(())
    })?;
    println!("✓ Accepted the latest auto-tune proposal for '{}'.", fan_id);
    println!("  Tuned gains are staged in the draft configuration only.");
    println!("  Run 'apply' to make the staged gains live.");
    Ok(())
}

async fn connect_control_proxy() -> zbus::Result<ControlProxyProxy<'static>> {
    let connection = crate::connect_dbus().await?;
    ControlProxyProxy::new(&connection).await
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

fn format_float_value(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_f64)
        .map(|value| format!("{value:.3}"))
        .unwrap_or_else(|| "n/a".to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

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
}
