use serde::{Deserialize, Serialize};

use crate::config::AppConfig;
use crate::inventory::{FanChannel, InventorySnapshot};
use crate::lifecycle::{FanRuntimeStatus, RuntimeState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewStructureSnapshot {
    pub rows: Vec<OverviewStructureRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewStructureRow {
    pub fan_id: String,
    pub display_name: String,
    pub friendly_name: Option<String>,
    pub hardware_label: Option<String>,
    pub support_state: String,
    pub control_mode: Option<String>,
    pub has_tach: bool,
    pub support_reason: Option<String>,
    pub ordering_bucket: String,
    pub state_text: String,
    pub state_icon_name: String,
    pub state_color: String,
    pub show_support_reason: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewTelemetryBatch {
    pub rows: Vec<OverviewTelemetryRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewTelemetryRow {
    pub fan_id: String,
    pub temperature_millidegrees: i64,
    pub temperature_text: String,
    pub rpm: i64,
    pub rpm_text: String,
    pub output_percent: f64,
    pub output_text: String,
    pub output_fill_ratio: f64,
    pub high_temp_alert: bool,
    pub show_rpm: bool,
    pub show_output: bool,
    pub visual_state: String,
}

fn ordering_bucket(status: &FanRuntimeStatus, high_temp_alert: bool) -> &'static str {
    match status {
        FanRuntimeStatus::Fallback => "fallback",
        FanRuntimeStatus::Degraded { .. } => "degraded",
        FanRuntimeStatus::Managed { .. } if high_temp_alert => "managed_hot",
        FanRuntimeStatus::Managed { .. } => "managed",
        FanRuntimeStatus::Unmanaged => "unmanaged",
    }
}

fn bucket_sort_key(bucket: &str) -> u8 {
    match bucket {
        "fallback" => 0,
        "degraded" => 1,
        "managed_hot" => 2,
        "managed" => 3,
        "unmanaged" => 4,
        _ => 5,
    }
}

fn state_text(status: &FanRuntimeStatus, high_temp_alert: bool) -> String {
    match status {
        FanRuntimeStatus::Fallback => "Fallback".into(),
        FanRuntimeStatus::Degraded { .. } => "Degraded".into(),
        FanRuntimeStatus::Managed { .. } if high_temp_alert => "Managed".into(),
        FanRuntimeStatus::Managed { .. } => "Managed".into(),
        FanRuntimeStatus::Unmanaged => "Unmanaged".into(),
    }
}

fn state_icon_name(status: &FanRuntimeStatus) -> &'static str {
    match status {
        FanRuntimeStatus::Fallback => "dialog-error-symbolic",
        FanRuntimeStatus::Degraded { .. } => "data-warning-symbolic",
        FanRuntimeStatus::Managed { .. } => "emblem-ok-symbolic",
        FanRuntimeStatus::Unmanaged => "dialog-information-symbolic",
    }
}

fn state_color(status: &FanRuntimeStatus, high_temp_alert: bool) -> &'static str {
    match status {
        FanRuntimeStatus::Fallback => "#e53935",
        FanRuntimeStatus::Degraded { .. } => "#ff9800",
        FanRuntimeStatus::Managed { .. } if high_temp_alert => "#e53935",
        FanRuntimeStatus::Managed { .. } => "#43a047",
        FanRuntimeStatus::Unmanaged => "#9e9e9e",
    }
}

fn fan_display_name(fan: &FanChannel) -> String {
    if let Some(ref name) = fan.friendly_name {
        if !name.is_empty() {
            return name.clone();
        }
    }
    if let Some(ref label) = fan.label {
        if !label.is_empty() {
            return label.clone();
        }
    }
    fan.id.clone()
}

fn format_temp(millideg: i64) -> String {
    format!("{:.1} °C", millideg as f64 / 1000.0)
}

fn format_rpm(rpm: i64) -> String {
    if rpm > 0 {
        format!("{} RPM", rpm)
    } else {
        "0 RPM".into()
    }
}

fn format_output(pct: f64) -> String {
    format!("{:.1}%", pct)
}

impl OverviewStructureSnapshot {
    pub fn build(
        snapshot: &InventorySnapshot,
        runtime: &RuntimeState,
        _config: &AppConfig,
    ) -> Self {
        let mut rows: Vec<OverviewStructureRow> = Vec::new();

        for device in &snapshot.devices {
            for fan in &device.fans {
                let status = runtime.fan_statuses.get(&fan.id);
                let rt_status = status.cloned().unwrap_or(FanRuntimeStatus::Unmanaged);

                let high_temp = match &rt_status {
                    FanRuntimeStatus::Managed { control, .. } => control.alert_high_temp,
                    _ => false,
                };

                let bucket = ordering_bucket(&rt_status, high_temp).to_string();
                let control_mode = match &rt_status {
                    FanRuntimeStatus::Managed { control_mode, .. } => {
                        Some(format!("{:?}", control_mode).to_lowercase())
                    }
                    _ => None,
                };

                let show_support_reason = matches!(
                    &rt_status,
                    FanRuntimeStatus::Unmanaged | FanRuntimeStatus::Degraded { .. }
                ) && fan.support_reason.is_some();

                rows.push(OverviewStructureRow {
                    fan_id: fan.id.clone(),
                    display_name: fan_display_name(fan),
                    friendly_name: fan.friendly_name.clone(),
                    hardware_label: fan.label.clone(),
                    support_state: format!("{:?}", fan.support_state).to_lowercase(),
                    control_mode,
                    has_tach: fan.rpm_feedback,
                    support_reason: fan.support_reason.clone(),
                    ordering_bucket: bucket,
                    state_text: state_text(&rt_status, high_temp),
                    state_icon_name: state_icon_name(&rt_status).to_string(),
                    state_color: state_color(&rt_status, high_temp).to_string(),
                    show_support_reason,
                });
            }
        }

        rows.sort_by(|a, b| {
            let ak = bucket_sort_key(&a.ordering_bucket);
            let bk = bucket_sort_key(&b.ordering_bucket);
            ak.cmp(&bk)
                .then_with(|| a.display_name.cmp(&b.display_name))
        });

        OverviewStructureSnapshot { rows }
    }
}

impl OverviewTelemetryBatch {
    pub fn build(snapshot: &InventorySnapshot, runtime: &RuntimeState) -> Self {
        let mut rows: Vec<OverviewTelemetryRow> = Vec::new();

        for device in &snapshot.devices {
            for fan in &device.fans {
                let status = runtime.fan_statuses.get(&fan.id);
                let rt_status = status.cloned().unwrap_or(FanRuntimeStatus::Unmanaged);

                let (temp_mdeg, output_pct, high_temp, visual_state) = match &rt_status {
                    FanRuntimeStatus::Managed { control, .. } => (
                        control.aggregated_temp_millidegrees.unwrap_or(0),
                        control.logical_output_percent.unwrap_or(0.0),
                        control.alert_high_temp,
                        if control.alert_high_temp {
                            "managed_hot"
                        } else {
                            "managed"
                        },
                    ),
                    FanRuntimeStatus::Degraded { .. } => (0, 100.0, false, "degraded"),
                    FanRuntimeStatus::Fallback => (0, 100.0, false, "fallback"),
                    FanRuntimeStatus::Unmanaged => (0, 0.0, false, "unmanaged"),
                };

                let rpm = fan.current_rpm.unwrap_or(0) as i64;
                let show_rpm = fan.rpm_feedback
                    && matches!(
                        rt_status,
                        FanRuntimeStatus::Managed { .. }
                            | FanRuntimeStatus::Degraded { .. }
                            | FanRuntimeStatus::Fallback
                    );
                let show_output = matches!(
                    rt_status,
                    FanRuntimeStatus::Managed { .. }
                        | FanRuntimeStatus::Degraded { .. }
                        | FanRuntimeStatus::Fallback
                );

                rows.push(OverviewTelemetryRow {
                    fan_id: fan.id.clone(),
                    temperature_millidegrees: temp_mdeg,
                    temperature_text: if temp_mdeg > 0 {
                        format_temp(temp_mdeg)
                    } else {
                        "No live reading".into()
                    },
                    rpm,
                    rpm_text: if show_rpm {
                        format_rpm(rpm)
                    } else {
                        "No RPM feedback".into()
                    },
                    output_percent: output_pct,
                    output_text: if show_output {
                        format_output(output_pct)
                    } else {
                        "No control".into()
                    },
                    output_fill_ratio: (output_pct / 100.0).clamp(0.0, 1.0),
                    high_temp_alert: high_temp,
                    show_rpm,
                    show_output,
                    visual_state: visual_state.to_string(),
                });
            }
        }

        OverviewTelemetryBatch { rows }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DegradedReason;
    use crate::inventory::{ControlMode, FanChannel, HwmonDevice, SupportState, TemperatureSensor};
    use crate::lifecycle::ControlRuntimeSnapshot;
    use std::collections::HashMap;

    fn test_device() -> HwmonDevice {
        HwmonDevice {
            id: "hwmon-test-0000000000000001".into(),
            name: "testchip".into(),
            sysfs_path: "/sys/class/hwmon/hwmon0".into(),
            stable_identity: "/sys/devices/platform/testchip".into(),
            temperatures: vec![TemperatureSensor {
                id: "hwmon-test-0000000000000001-temp1".into(),
                channel: 1,
                label: Some("CPU".into()),
                friendly_name: None,
                input_millidegrees_celsius: Some(45000),
            }],
            fans: vec![FanChannel {
                id: "hwmon-test-0000000000000001-fan1".into(),
                channel: 1,
                label: Some("fan1".into()),
                friendly_name: Some("CPU Fan".into()),
                rpm_feedback: true,
                current_rpm: Some(1240),
                control_modes: vec![ControlMode::Pwm],
                support_state: SupportState::Available,
                support_reason: None,
            }],
        }
    }

    fn unmanaged_runtime() -> RuntimeState {
        let mut fan_statuses = HashMap::new();
        fan_statuses.insert(
            "hwmon-test-0000000000000001-fan1".into(),
            FanRuntimeStatus::Unmanaged,
        );
        RuntimeState {
            fan_statuses,
            owned_fans: vec![],
        }
    }

    fn managed_runtime() -> RuntimeState {
        let mut fan_statuses = HashMap::new();
        fan_statuses.insert(
            "hwmon-test-0000000000000001-fan1".into(),
            FanRuntimeStatus::Managed {
                control_mode: ControlMode::Pwm,
                control: ControlRuntimeSnapshot {
                    sensor_ids: vec!["hwmon-test-0000000000000001-temp1".into()],
                    aggregation: crate::control::AggregationFn::Average,
                    target_temp_millidegrees: 50000,
                    aggregated_temp_millidegrees: Some(55250),
                    logical_output_percent: Some(31.4),
                    mapped_pwm: None,
                    auto_tuning: false,
                    alert_high_temp: false,
                    last_error_millidegrees: None,
                },
            },
        );
        RuntimeState {
            fan_statuses,
            owned_fans: vec!["hwmon-test-0000000000000001-fan1".into()],
        }
    }

    #[test]
    fn structure_row_has_correct_static_fields() {
        let snapshot = InventorySnapshot {
            devices: vec![test_device()],
        };
        let runtime = unmanaged_runtime();
        let config = AppConfig::default();

        let structure = OverviewStructureSnapshot::build(&snapshot, &runtime, &config);

        assert_eq!(structure.rows.len(), 1);
        let row = &structure.rows[0];
        assert_eq!(row.fan_id, "hwmon-test-0000000000000001-fan1");
        assert_eq!(row.display_name, "CPU Fan");
        assert_eq!(row.ordering_bucket, "unmanaged");
        assert_eq!(row.state_text, "Unmanaged");
        assert!(row.has_tach);
    }

    #[test]
    fn structure_orders_fallback_first() {
        let mut snapshot = InventorySnapshot {
            devices: vec![test_device()],
        };
        let mut fan2 = snapshot.devices[0].fans[0].clone();
        fan2.id = "hwmon-test-0000000000000001-fan2".into();
        fan2.friendly_name = None;
        fan2.label = Some("fan2".into());
        snapshot.devices[0].fans.push(fan2);

        let mut fan_statuses = HashMap::new();
        fan_statuses.insert(
            "hwmon-test-0000000000000001-fan1".into(),
            FanRuntimeStatus::Managed {
                control_mode: ControlMode::Pwm,
                control: ControlRuntimeSnapshot {
                    sensor_ids: vec![],
                    aggregation: crate::control::AggregationFn::Average,
                    target_temp_millidegrees: 50000,
                    aggregated_temp_millidegrees: Some(55000),
                    logical_output_percent: Some(50.0),
                    mapped_pwm: None,
                    auto_tuning: false,
                    alert_high_temp: false,
                    last_error_millidegrees: None,
                },
            },
        );
        fan_statuses.insert(
            "hwmon-test-0000000000000001-fan2".into(),
            FanRuntimeStatus::Fallback,
        );
        let runtime = RuntimeState {
            fan_statuses,
            owned_fans: vec!["hwmon-test-0000000000000001-fan1".into()],
        };

        let structure =
            OverviewStructureSnapshot::build(&snapshot, &runtime, &AppConfig::default());

        assert_eq!(structure.rows[0].ordering_bucket, "fallback");
        assert_eq!(structure.rows[1].ordering_bucket, "managed");
    }

    #[test]
    fn telemetry_has_preformatted_strings() {
        let snapshot = InventorySnapshot {
            devices: vec![test_device()],
        };
        let runtime = managed_runtime();

        let telemetry = OverviewTelemetryBatch::build(&snapshot, &runtime);

        assert_eq!(telemetry.rows.len(), 1);
        let row = &telemetry.rows[0];
        assert_eq!(row.fan_id, "hwmon-test-0000000000000001-fan1");
        assert_eq!(row.temperature_millidegrees, 55250);
        assert_eq!(row.temperature_text, "55.2 °C");
        assert_eq!(row.rpm, 1240);
        assert_eq!(row.rpm_text, "1240 RPM");
        assert!((row.output_percent - 31.4).abs() < 0.01);
        assert_eq!(row.output_text, "31.4%");
        assert!((row.output_fill_ratio - 0.314).abs() < 0.001);
        assert!(row.show_rpm);
        assert!(row.show_output);
        assert_eq!(row.visual_state, "managed");
        assert!(!row.high_temp_alert);
    }

    #[test]
    fn telemetry_unmanaged_has_no_readings() {
        let snapshot = InventorySnapshot {
            devices: vec![test_device()],
        };
        let runtime = unmanaged_runtime();

        let telemetry = OverviewTelemetryBatch::build(&snapshot, &runtime);

        let row = &telemetry.rows[0];
        assert_eq!(row.temperature_millidegrees, 0);
        assert_eq!(row.temperature_text, "No live reading");
        assert!(!row.show_rpm);
        assert!(!row.show_output);
        assert_eq!(row.visual_state, "unmanaged");
    }

    #[test]
    fn high_temp_alert_produces_managed_hot_bucket() {
        let snapshot = InventorySnapshot {
            devices: vec![test_device()],
        };
        let mut runtime = managed_runtime();
        if let FanRuntimeStatus::Managed { control, .. } = runtime
            .fan_statuses
            .get_mut("hwmon-test-0000000000000001-fan1")
            .unwrap()
        {
            control.alert_high_temp = true;
        }

        let structure =
            OverviewStructureSnapshot::build(&snapshot, &runtime, &AppConfig::default());
        let telemetry = OverviewTelemetryBatch::build(&snapshot, &runtime);

        assert_eq!(structure.rows[0].ordering_bucket, "managed_hot");
        assert!(telemetry.rows[0].high_temp_alert);
        assert_eq!(telemetry.rows[0].visual_state, "managed_hot");
    }

    #[test]
    fn degraded_shows_full_output() {
        let snapshot = InventorySnapshot {
            devices: vec![test_device()],
        };
        let mut fan_statuses = HashMap::new();
        fan_statuses.insert(
            "hwmon-test-0000000000000001-fan1".into(),
            FanRuntimeStatus::Degraded {
                reasons: vec![DegradedReason::FanMissing {
                    fan_id: "hwmon-test-0000000000000001-fan1".into(),
                }],
            },
        );
        let runtime = RuntimeState {
            fan_statuses,
            owned_fans: vec![],
        };

        let telemetry = OverviewTelemetryBatch::build(&snapshot, &runtime);
        let row = &telemetry.rows[0];
        assert_eq!(row.visual_state, "degraded");
        assert!(row.show_output);
        assert_eq!(row.output_percent, 100.0);
    }

    #[test]
    fn serialization_round_trips() {
        let snapshot = InventorySnapshot {
            devices: vec![test_device()],
        };
        let runtime = managed_runtime();

        let structure =
            OverviewStructureSnapshot::build(&snapshot, &runtime, &AppConfig::default());
        let telemetry = OverviewTelemetryBatch::build(&snapshot, &runtime);

        let s_json = serde_json::to_string(&structure).unwrap();
        let t_json = serde_json::to_string(&telemetry).unwrap();

        let s_back: OverviewStructureSnapshot = serde_json::from_str(&s_json).unwrap();
        let t_back: OverviewTelemetryBatch = serde_json::from_str(&t_json).unwrap();

        assert_eq!(s_back.rows.len(), 1);
        assert_eq!(t_back.rows.len(), 1);
        assert_eq!(s_back.rows[0].fan_id, structure.rows[0].fan_id);
        assert_eq!(t_back.rows[0].fan_id, telemetry.rows[0].fan_id);
    }
}
