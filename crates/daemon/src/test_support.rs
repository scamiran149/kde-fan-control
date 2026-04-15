//! Shared test fixtures for daemon integration tests.
//!
//! Provides pre-built `AppliedConfig`, `InventorySnapshot`, and
//! `ControlFixture` helpers that create temporary sysfs-like directory
//! trees for control-loop testing without real hardware.

use kde_fan_control_core::config::AppliedConfig;
use kde_fan_control_core::control::{
    ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits,
};
use kde_fan_control_core::inventory::{
    ControlMode, HwmonDevice, InventorySnapshot, TemperatureSensor,
};

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use kde_fan_control_core::config::AppliedFanEntry;

pub static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn applied_entry(temp_sources: Vec<String>) -> AppliedFanEntry {
    AppliedFanEntry {
        control_mode: ControlMode::Pwm,
        temp_sources,
        target_temp_millidegrees: 50_000,
        aggregation: AggregationFn::Average,
        pid_gains: PidGains {
            kp: 1.0,
            ki: 0.0,
            kd: 0.0,
        },
        cadence: ControlCadence {
            sample_interval_ms: 20,
            control_interval_ms: 20,
            write_interval_ms: 20,
        },
        deadband_millidegrees: 0,
        actuator_policy: ActuatorPolicy {
            output_min_percent: 0.0,
            output_max_percent: 100.0,
            pwm_min: 0,
            pwm_max: 255,
            startup_kick_percent: 35.0,
            startup_kick_ms: 1,
        },
        pid_limits: PidLimits::default(),
    }
}

pub fn applied_config_for(fan_id: &str, temp_id: &str) -> AppliedConfig {
    AppliedConfig {
        fans: std::collections::HashMap::from([(
            fan_id.to_string(),
            applied_entry(vec![temp_id.to_string()]),
        )]),
        applied_at: Some("2026-04-11T12:00:00Z".to_string()),
    }
}

pub fn test_snapshot(root: &Path) -> InventorySnapshot {
    InventorySnapshot {
        devices: vec![HwmonDevice {
            id: "hwmon-test-0000000000000001".to_string(),
            name: "testchip".to_string(),
            sysfs_path: root.display().to_string(),
            stable_identity: "/sys/devices/platform/testchip".to_string(),
            temperatures: vec![TemperatureSensor {
                id: "hwmon-test-0000000000000001-temp1".to_string(),
                channel: 1,
                label: Some("CPU".to_string()),
                friendly_name: None,
                input_millidegrees_celsius: Some(55_000),
            }],
            fans: vec![kde_fan_control_core::inventory::FanChannel {
                id: "hwmon-test-0000000000000001-fan1".to_string(),
                channel: 1,
                label: Some("CPU Fan".to_string()),
                friendly_name: None,
                rpm_feedback: true,
                current_rpm: Some(1200),
                control_modes: vec![ControlMode::Pwm],
                support_state: kde_fan_control_core::inventory::SupportState::Available,
                support_reason: None,
            }],
        }],
    }
}

pub struct ControlFixture {
    root: PathBuf,
}

impl ControlFixture {
    pub fn new() -> Self {
        let unique = format!(
            "kde-fan-control-daemon-control-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock should be after epoch")
                .as_nanos(),
            TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).expect("fixture root should be created");
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write_temp(&self, value: &str) {
        fs::write(self.root.join("temp1_input"), value).expect("temp input should be written");
    }

    pub fn write_pwm_seed(&self, value: &str) {
        fs::write(self.root.join("pwm1"), value).expect("pwm file should be written");
    }

    pub fn pwm_path(&self) -> PathBuf {
        self.root.join("pwm1")
    }
}

impl Drop for ControlFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
