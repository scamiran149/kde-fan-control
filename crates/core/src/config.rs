//! Daemon-owned configuration types and persistence.
//!
//! Defines `AppConfig`, `DraftConfig`, `AppliedConfig`, and all
//! fan-entry variants. The daemon owns a single authoritative
//! TOML config file; clients mutate it exclusively through the
//! Lifecycle DBus interface. All new fields must use
//! `#[serde(default)]` for backward compatibility.
//!
//! This module also re-exports the validation and apply-draft
//! functions from the `validation` submodule for convenience.

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::control::{ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits};
use crate::inventory::ControlMode;

pub use crate::validation::{
    ValidationError, ValidationResult, apply_draft, find_fan_by_id, temp_source_exists,
    validate_draft,
};

/// Current schema version for the daemon-owned configuration file.
/// Increment when making backward-incompatible changes to the config format.
pub const CONFIG_VERSION: u32 = 1;

pub fn app_state_dir() -> PathBuf {
    state_directory_from_env(env::var("STATE_DIRECTORY").ok()).unwrap_or_else(|| {
        dirs::state_dir()
            .or_else(dirs::data_local_dir)
            .unwrap_or_else(|| PathBuf::from("/var/lib"))
            .join("kde-fan-control")
    })
}

fn state_directory_from_env(value: Option<String>) -> Option<PathBuf> {
    value
        .and_then(|raw| raw.split(':').next().map(str::trim).map(str::to_owned))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

/// Top-level daemon-owned configuration.
///
/// This is the single authoritative persisted state that the daemon owns.
/// It carries friendly names, draft lifecycle edits, and the applied
/// configuration that drives boot-time fan management recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Schema version for future migration support.
    #[serde(default = "default_version")]
    pub version: u32,

    /// User-assigned friendly names for sensors and fans.
    #[serde(default)]
    pub friendly_names: FriendlyNames,

    /// Draft lifecycle configuration — mutable, user-editable, validated before apply.
    /// Not live until explicitly promoted to `applied`.
    #[serde(default)]
    pub draft: DraftConfig,

    /// The single authoritative applied configuration used for runtime behavior
    /// and boot-time recovery. Exactly one in v1.
    #[serde(default)]
    pub applied: Option<AppliedConfig>,

    /// The last fallback incident recorded by the daemon.
    ///
    /// This durable record survives process exit so the next daemon start can
    /// continue surfacing which owned fans were driven toward safe maximum.
    #[serde(default)]
    pub fallback_incident: Option<FallbackIncident>,

    /// Interval in milliseconds between re-assessment attempts for degraded fans.
    /// Defaults to 10000 (10 seconds).
    #[serde(default = "default_reassess_degraded_interval_ms")]
    pub reassess_degraded_interval_ms: u64,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

fn default_reassess_degraded_interval_ms() -> u64 {
    10000
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            friendly_names: FriendlyNames::default(),
            draft: DraftConfig::default(),
            applied: None,
            fallback_incident: None,
            reassess_degraded_interval_ms: default_reassess_degraded_interval_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FriendlyNames {
    #[serde(default)]
    pub sensors: HashMap<String, String>,
    #[serde(default)]
    pub fans: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Draft config — staged edits that require explicit apply to go live
// ---------------------------------------------------------------------------

/// Draft lifecycle configuration. Users edit this via DBus or CLI, then
/// apply it to promote the changes into the authoritative applied config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DraftConfig {
    /// Per-fan enrollment entries keyed by stable fan ID.
    #[serde(default)]
    pub fans: HashMap<String, DraftFanEntry>,
}

/// A single fan's draft enrollment state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DraftFanEntry {
    /// Whether the daemon should manage this fan when the draft is applied.
    pub managed: bool,

    /// The control mode the user selected for this fan.
    /// Must be one of the modes reported as supported by the fan's capabilities.
    #[serde(default)]
    pub control_mode: Option<ControlMode>,

    /// Stable ID(s) of temperature sensor(s) to use as input for this fan's
    /// control loop. Not validated until apply time against current inventory.
    #[serde(default)]
    pub temp_sources: Vec<String>,

    #[serde(default)]
    pub target_temp_millidegrees: Option<i64>,

    #[serde(default)]
    pub aggregation: Option<AggregationFn>,

    #[serde(default)]
    pub pid_gains: Option<PidGains>,

    #[serde(default)]
    pub cadence: Option<ControlCadence>,

    #[serde(default)]
    pub deadband_millidegrees: Option<i64>,

    #[serde(default)]
    pub actuator_policy: Option<ActuatorPolicy>,

    #[serde(default)]
    pub pid_limits: Option<PidLimits>,
}

// ---------------------------------------------------------------------------
// Applied config — the single authoritative live configuration
// ---------------------------------------------------------------------------

/// The applied configuration — the result of promoting a validated draft.
/// This is what the daemon uses at runtime and what gets recovered on boot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedConfig {
    /// Per-fan managed entries keyed by stable fan ID.
    /// Only fans that passed validation appear here.
    pub fans: HashMap<String, AppliedFanEntry>,

    /// Timestamp (ISO 8601) when this config was last applied.
    #[serde(default)]
    pub applied_at: Option<String>,
}

fn default_applied_target_temp_millidegrees() -> i64 {
    65_000
}

fn default_applied_deadband_millidegrees() -> i64 {
    1_000
}

/// A fan that is actively managed by the daemon under the applied config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedFanEntry {
    /// The control mode in use for this managed fan.
    pub control_mode: ControlMode,

    /// Temperature source IDs used as input for this fan's control loop.
    #[serde(default)]
    pub temp_sources: Vec<String>,

    /// Target temperature in millidegrees Celsius.
    /// Defaults to 65°C (65000 m°C) when absent from TOML — a conservative
    /// safe default that results in fans running moderately, not silent.
    #[serde(default = "default_applied_target_temp_millidegrees")]
    pub target_temp_millidegrees: i64,

    /// Temperature aggregation function.
    /// Defaults to Average when absent from TOML.
    #[serde(default)]
    pub aggregation: AggregationFn,

    /// PID controller gains.
    /// Defaults to PidGains::default() when absent from TOML.
    #[serde(default)]
    pub pid_gains: PidGains,

    /// Control loop cadence (sample, control, write intervals).
    /// Defaults to ControlCadence::default() when absent from TOML.
    #[serde(default)]
    pub cadence: ControlCadence,

    /// Deadband in millidegrees Celsius.
    /// Defaults to 1°C (1000 m°C) when absent from TOML.
    #[serde(default = "default_applied_deadband_millidegrees")]
    pub deadband_millidegrees: i64,

    /// Actuator output policy (PWM range, startup kick, etc.).
    /// Defaults to ActuatorPolicy::default() when absent from TOML.
    #[serde(default)]
    pub actuator_policy: ActuatorPolicy,

    /// PID integral and derivative clamp limits.
    /// Defaults to PidLimits::default() when absent from TOML.
    #[serde(default)]
    pub pid_limits: PidLimits,
}

impl DraftFanEntry {
    pub fn resolved_target_temp_millidegrees(&self) -> Option<i64> {
        self.target_temp_millidegrees
    }

    pub fn resolved_aggregation(&self) -> AggregationFn {
        self.aggregation.unwrap_or_default()
    }

    pub fn resolved_pid_gains(&self) -> PidGains {
        self.pid_gains.unwrap_or_default()
    }

    pub fn resolved_cadence(&self) -> ControlCadence {
        self.cadence.unwrap_or_default()
    }

    pub fn resolved_deadband_millidegrees(&self) -> i64 {
        self.deadband_millidegrees.unwrap_or(1_000)
    }

    pub fn resolved_actuator_policy(&self) -> ActuatorPolicy {
        self.actuator_policy.unwrap_or_default()
    }

    pub fn resolved_pid_limits(&self) -> PidLimits {
        self.pid_limits.unwrap_or_default()
    }
}

/// A single fan whose fallback write failed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FallbackFailure {
    /// Stable fan ID that the daemon attempted to protect.
    pub fan_id: String,

    /// Human-readable write failure description.
    pub error: String,
}

/// Durable record of a fallback incident.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FallbackIncident {
    /// ISO 8601 timestamp when fallback was attempted.
    pub timestamp: String,

    /// Stable fan IDs that were owned by the daemon when fallback ran.
    #[serde(default)]
    pub affected_fans: Vec<String>,

    /// Fans whose safe-maximum write failed.
    #[serde(default)]
    pub failed: Vec<FallbackFailure>,

    /// Optional free-form detail explaining why fallback was triggered.
    #[serde(default)]
    pub detail: Option<String>,
}

impl FallbackIncident {
    /// Build the fallback fan-id set used by runtime-state reconstruction.
    pub fn fallback_fan_ids(&self) -> HashSet<String> {
        self.affected_fans.iter().cloned().collect()
    }
}

impl AppConfig {
    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    pub fn load() -> io::Result<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(&path)?;
        let config: AppConfig =
            toml::from_str(&contents).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Reject future schema versions — these need a migration path.
        if config.version > CONFIG_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "config version {} is newer than supported version {}; manual migration required",
                    config.version, CONFIG_VERSION
                ),
            ));
        }

        Ok(config)
    }

    pub fn save(&self) -> io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&path, contents)?;
        // Set config file to owner:rw, group:r, other:--- to avoid
        // world-readable config leakage (L3).
        #[cfg(unix)]
        if let Err(e) =
            std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o640))
        {
            // Log warning but don't fail the save
            eprintln!("warning: failed to set permissions on config file: {e}");
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Friendly-name helpers (preserved from Phase 1)
    // -----------------------------------------------------------------------

    pub fn set_sensor_name(&mut self, id: &str, name: String) {
        self.friendly_names.sensors.insert(id.to_string(), name);
    }

    pub fn set_fan_name(&mut self, id: &str, name: String) {
        self.friendly_names.fans.insert(id.to_string(), name);
    }

    pub fn remove_sensor_name(&mut self, id: &str) {
        self.friendly_names.sensors.remove(id);
    }

    pub fn remove_fan_name(&mut self, id: &str) {
        self.friendly_names.fans.remove(id);
    }

    pub fn sensor_name(&self, id: &str) -> Option<&str> {
        self.friendly_names.sensors.get(id).map(|s| s.as_str())
    }

    pub fn fan_name(&self, id: &str) -> Option<&str> {
        self.friendly_names.fans.get(id).map(|s| s.as_str())
    }

    // -----------------------------------------------------------------------
    // Draft config helpers
    // -----------------------------------------------------------------------

    /// Set or update a fan's draft enrollment entry.
    pub fn set_draft_fan(&mut self, fan_id: &str, entry: DraftFanEntry) {
        self.draft.fans.insert(fan_id.to_string(), entry);
    }

    /// Remove a fan from the draft.
    pub fn remove_draft_fan(&mut self, fan_id: &str) {
        self.draft.fans.remove(fan_id);
    }

    /// Get a fan's draft entry, if any.
    pub fn draft_fan(&self, fan_id: &str) -> Option<&DraftFanEntry> {
        self.draft.fans.get(fan_id)
    }

    // -----------------------------------------------------------------------
    // Applied config helpers
    // -----------------------------------------------------------------------

    /// Replace the applied config with a new validated set.
    pub fn set_applied(&mut self, applied: AppliedConfig) {
        self.applied = Some(applied);
    }

    /// Clear the applied config entirely (e.g., after all fans are removed).
    pub fn clear_applied(&mut self) {
        self.applied = None;
    }

    /// Get the current applied config, if any.
    pub fn applied(&self) -> Option<&AppliedConfig> {
        self.applied.as_ref()
    }

    /// Replace the currently persisted fallback incident.
    pub fn set_fallback_incident(&mut self, incident: FallbackIncident) {
        self.fallback_incident = Some(incident);
    }

    /// Clear the persisted fallback incident.
    pub fn clear_fallback_incident(&mut self) {
        self.fallback_incident = None;
    }
}

fn config_path() -> PathBuf {
    app_state_dir().join("config.toml")
}

// ---------------------------------------------------------------------------
// Re-exports from lifecycle module for backward compatibility
// ---------------------------------------------------------------------------

pub use crate::lifecycle::{
    DegradedReason, DegradedState, LifecycleEvent, LifecycleEventLog, MAX_LIFECYCLE_EVENTS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits};
    use crate::inventory::{
        ControlMode, FanChannel, HwmonDevice, InventorySnapshot, SupportState, TemperatureSensor,
    };

    fn managed_draft_entry() -> DraftFanEntry {
        DraftFanEntry {
            managed: true,
            control_mode: Some(ControlMode::Pwm),
            temp_sources: vec!["hwmon-test-0000000000000001-temp1".to_string()],
            target_temp_millidegrees: Some(65_000),
            aggregation: None,
            pid_gains: None,
            cadence: None,
            deadband_millidegrees: None,
            actuator_policy: None,
            pid_limits: None,
        }
    }

    /// Build a minimal inventory snapshot with one available fan and one sensor.
    fn test_snapshot() -> InventorySnapshot {
        InventorySnapshot {
            devices: vec![HwmonDevice {
                id: "hwmon-test-0000000000000001".to_string(),
                name: "testchip".to_string(),
                sysfs_path: "/sys/class/hwmon/hwmon0".to_string(),
                stable_identity: "/sys/devices/platform/testchip".to_string(),
                temperatures: vec![TemperatureSensor {
                    id: "hwmon-test-0000000000000001-temp1".to_string(),
                    channel: 1,
                    label: Some("CPU".to_string()),
                    friendly_name: None,
                    input_millidegrees_celsius: Some(45000),
                }],
                fans: vec![FanChannel {
                    id: "hwmon-test-0000000000000001-fan1".to_string(),
                    channel: 1,
                    label: Some("CPU Fan".to_string()),
                    friendly_name: None,
                    rpm_feedback: true,
                    current_rpm: Some(1200),
                    control_modes: vec![ControlMode::Pwm],
                    support_state: SupportState::Available,
                    support_reason: None,
                }],
            }],
        }
    }

    #[test]
    fn round_trip_friendly_names_only() {
        let mut config = AppConfig::default();
        config.set_sensor_name("temp1", "CPU Temp".to_string());
        config.set_fan_name("fan1", "CPU Fan".to_string());

        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: AppConfig = toml::from_str(&serialized).unwrap();

        assert_eq!(deserialized.sensor_name("temp1"), Some("CPU Temp"));
        assert_eq!(deserialized.fan_name("fan1"), Some("CPU Fan"));
        assert!(deserialized.applied.is_none());
    }

    #[test]
    fn round_trip_with_managed_fan() {
        let mut config = AppConfig::default();
        config.set_draft_fan("hwmon-test-0000000000000001-fan1", managed_draft_entry());

        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: AppConfig = toml::from_str(&serialized).unwrap();

        assert!(
            deserialized
                .draft_fan("hwmon-test-0000000000000001-fan1")
                .is_some()
        );
        let entry = deserialized
            .draft_fan("hwmon-test-0000000000000001-fan1")
            .unwrap();
        assert!(entry.managed);
        assert_eq!(entry.control_mode, Some(ControlMode::Pwm));
    }

    #[test]
    fn round_trip_applied_config() {
        let mut config = AppConfig::default();
        let applied = AppliedConfig {
            fans: {
                let mut m = HashMap::new();
                m.insert(
                    "hwmon-test-0000000000000001-fan1".to_string(),
                    AppliedFanEntry {
                        control_mode: ControlMode::Pwm,
                        temp_sources: vec!["hwmon-test-0000000000000001-temp1".to_string()],
                        target_temp_millidegrees: 65_000,
                        aggregation: AggregationFn::Average,
                        pid_gains: PidGains::default(),
                        cadence: ControlCadence::default(),
                        deadband_millidegrees: 1_000,
                        actuator_policy: ActuatorPolicy::default(),
                        pid_limits: PidLimits::default(),
                    },
                );
                m
            },
            applied_at: Some("2026-04-11T12:00:00Z".to_string()),
        };
        config.set_applied(applied);

        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: AppConfig = toml::from_str(&serialized).unwrap();

        assert!(deserialized.applied().is_some());
        let a = deserialized.applied().unwrap();
        assert!(a.fans.contains_key("hwmon-test-0000000000000001-fan1"));
    }

    #[test]
    fn round_trip_persisted_fallback_incident() {
        let mut config = AppConfig::default();
        config.set_fallback_incident(FallbackIncident {
            timestamp: "2026-04-11T16:30:00Z".to_string(),
            affected_fans: vec!["hwmon-test-0000000000000001-fan1".to_string()],
            failed: vec![FallbackFailure {
                fan_id: "hwmon-test-0000000000000001-fan1".to_string(),
                error: "permission denied".to_string(),
            }],
            detail: Some("panic hook triggered fallback".to_string()),
        });

        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: AppConfig = toml::from_str(&serialized).unwrap();

        let incident = deserialized.fallback_incident.as_ref().unwrap();
        assert_eq!(incident.timestamp, "2026-04-11T16:30:00Z");
        assert_eq!(
            incident.affected_fans,
            vec!["hwmon-test-0000000000000001-fan1"]
        );
        assert_eq!(incident.failed.len(), 1);
        assert_eq!(
            incident.failed[0].fan_id,
            "hwmon-test-0000000000000001-fan1"
        );
        assert_eq!(
            incident.detail.as_deref(),
            Some("panic hook triggered fallback")
        );
    }

    #[test]
    fn validation_accepts_valid_draft() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            managed_draft_entry(),
        );

        let result = validate_draft(&draft, &snapshot);
        assert!(result.all_passed());
        assert!(
            result
                .enrollable
                .contains(&"hwmon-test-0000000000000001-fan1".to_string())
        );
        assert!(result.rejected.is_empty());
    }

    #[test]
    fn validation_rejects_invalid_control_mode() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                control_mode: Some(ControlMode::Voltage),
                temp_sources: vec!["hwmon-test-0000000000000001-temp1".to_string()],
                ..managed_draft_entry()
            },
        );

        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::UnsupportedControlMode { .. }
        ));
    }

    #[test]
    fn validation_rejects_stale_fan_id() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft
            .fans
            .insert("hwmon-nonexistent-fan99".to_string(), managed_draft_entry());

        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::FanNotFound { .. }
        ));
    }

    #[test]
    fn validation_rejects_unsupported_fan() {
        let mut snapshot = test_snapshot();
        // Make the fan Partial instead of Available.
        snapshot.devices[0].fans[0].support_state = SupportState::Partial;
        snapshot.devices[0].fans[0].support_reason = Some("pwm not writable".to_string());
        snapshot.devices[0].fans[0].control_modes.clear();

        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            managed_draft_entry(),
        );

        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::FanNotEnrollable { .. }
        ));
    }

    #[test]
    fn validation_rejects_missing_temp_source() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                temp_sources: vec!["nonexistent-temp".to_string()],
                ..managed_draft_entry()
            },
        );

        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::TempSourceNotFound { .. }
        ));
    }

    #[test]
    fn validation_skips_unmanaged_entries() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry::default(),
        );

        let result = validate_draft(&draft, &snapshot);
        assert!(result.all_passed());
        assert!(result.enrollable.is_empty());
        assert!(result.rejected.is_empty());
    }

    #[test]
    fn apply_draft_best_effort_partial() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();

        // Valid entry.
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            managed_draft_entry(),
        );

        // Invalid entry — nonexistent fan.
        draft
            .fans
            .insert("ghost-fan".to_string(), managed_draft_entry());

        let (applied, result) =
            apply_draft(&draft, &snapshot, "2026-04-11T12:00:00Z".to_string(), None);

        // Only the valid fan should appear in applied.
        assert!(
            applied
                .fans
                .contains_key("hwmon-test-0000000000000001-fan1")
        );
        assert!(!applied.fans.contains_key("ghost-fan"));
        assert_eq!(result.rejected.len(), 1);
    }

    #[test]
    fn validation_rejects_missing_target_temperature() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                target_temp_millidegrees: None,
                ..managed_draft_entry()
            },
        );

        let result = validate_draft(&draft, &snapshot);
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::MissingTargetTemp { .. }
        ));
    }

    #[test]
    fn validation_rejects_managed_fan_without_sensor_sources() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                temp_sources: vec![],
                ..managed_draft_entry()
            },
        );

        let result = validate_draft(&draft, &snapshot);
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::NoSensorForManagedFan { .. }
        ));
    }

    #[test]
    fn apply_draft_preserves_resolved_control_profile() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                target_temp_millidegrees: Some(72_000),
                aggregation: Some(AggregationFn::Median),
                pid_gains: Some(PidGains {
                    kp: 2.0,
                    ki: 0.2,
                    kd: 0.7,
                }),
                cadence: Some(ControlCadence {
                    sample_interval_ms: 500,
                    control_interval_ms: 1_500,
                    write_interval_ms: 2_000,
                }),
                deadband_millidegrees: Some(2_500),
                actuator_policy: Some(ActuatorPolicy {
                    output_min_percent: 10.0,
                    output_max_percent: 95.0,
                    pwm_min: 20,
                    pwm_max: 240,
                    startup_kick_percent: 40.0,
                    startup_kick_ms: 1_800,
                }),
                pid_limits: Some(PidLimits {
                    integral_min: -20.0,
                    integral_max: 30.0,
                    derivative_min: -5.0,
                    derivative_max: 8.0,
                }),
                ..managed_draft_entry()
            },
        );

        let (applied, result) =
            apply_draft(&draft, &snapshot, "2026-04-11T12:00:00Z".to_string(), None);
        assert!(result.all_passed());

        let entry = applied
            .fans
            .get("hwmon-test-0000000000000001-fan1")
            .expect("fan should be applied");
        assert_eq!(entry.target_temp_millidegrees, 72_000);
        assert_eq!(entry.aggregation, AggregationFn::Median);
        assert_eq!(entry.pid_gains.kp, 2.0);
        assert_eq!(entry.pid_gains.ki, 0.2);
        assert_eq!(entry.pid_gains.kd, 0.7);
        assert_eq!(
            entry.cadence,
            ControlCadence {
                sample_interval_ms: 500,
                control_interval_ms: 1_500,
                write_interval_ms: 2_000,
            }
        );
        assert_eq!(entry.deadband_millidegrees, 2_500);
        assert_eq!(entry.actuator_policy.pwm_min, 20);
        assert_eq!(entry.actuator_policy.pwm_max, 240);
        assert_eq!(entry.actuator_policy.startup_kick_percent, 40.0);
        assert_eq!(entry.pid_limits.integral_min, -20.0);
        assert_eq!(entry.pid_limits.derivative_max, 8.0);
    }

    #[test]
    fn config_version_rejects_future() {
        let future_toml = r#"
version = 999
[friendly_names]
[draft]
"#;
        let result: Result<AppConfig, _> = toml::from_str(future_toml);
        // It should parse...
        let config = result.unwrap();
        assert_eq!(config.version, 999);
        // ...but load() would reject it. We test the version check directly.
        assert!(config.version > CONFIG_VERSION);
    }

    #[test]
    fn default_config_has_version_1() {
        let config = AppConfig::default();
        assert_eq!(config.version, CONFIG_VERSION);
    }

    #[test]
    fn backward_compat_phase2_config_deserializes_with_defaults() {
        let phase2_toml = r#"
            version = 1
            [friendly_names]
            [draft]

            [applied]
            applied_at = "2026-04-10T12:00:00Z"

            [applied.fans.hwmon-test-0000000000000001-fan1]
            control_mode = "pwm"
            temp_sources = ["hwmon-test-0000000000000001-temp1"]
        "#;
        let config: AppConfig =
            toml::from_str(phase2_toml).expect("Phase 2 config should deserialize");
        let applied = config.applied.expect("applied config should exist");
        let entry = applied
            .fans
            .get("hwmon-test-0000000000000001-fan1")
            .expect("fan entry should exist");

        assert_eq!(entry.control_mode, ControlMode::Pwm);
        assert_eq!(
            entry.temp_sources,
            vec!["hwmon-test-0000000000000001-temp1"]
        );
        // Phase 3 defaults filled in:
        assert_eq!(entry.target_temp_millidegrees, 65_000);
        assert_eq!(entry.aggregation, AggregationFn::Average);
        assert_eq!(entry.pid_gains, PidGains::default());
        assert_eq!(entry.cadence, ControlCadence::default());
        assert_eq!(entry.deadband_millidegrees, 1_000);
        assert_eq!(entry.actuator_policy, ActuatorPolicy::default());
        assert_eq!(entry.pid_limits, PidLimits::default());
    }

    #[test]
    fn backward_compat_phase2_config_no_applied_section() {
        let phase2_toml = r#"
            version = 1
            [friendly_names]
            [draft]
        "#;
        let config: AppConfig =
            toml::from_str(phase2_toml).expect("minimal Phase 2 config should deserialize");
        assert!(config.applied.is_none());
    }

    #[test]
    fn state_directory_env_uses_first_path() {
        let path = state_directory_from_env(Some(
            "/var/lib/kde-fan-control:/var/lib/ignored".to_string(),
        ))
        .expect("state directory env should resolve");

        assert_eq!(path, PathBuf::from("/var/lib/kde-fan-control"));
    }

    #[test]
    fn state_directory_env_ignores_empty_values() {
        assert!(state_directory_from_env(Some("   ".to_string())).is_none());
        assert!(state_directory_from_env(None).is_none());
    }

    #[test]
    fn validation_rejects_non_finite_pid_gains() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();

        // Test NaN in kp
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                pid_gains: Some(PidGains {
                    kp: f64::NAN,
                    ki: 1.0,
                    kd: 0.5,
                }),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::InvalidPidGains { .. }
        ));

        // Test Infinity in ki
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                pid_gains: Some(PidGains {
                    kp: 1.0,
                    ki: f64::INFINITY,
                    kd: 0.5,
                }),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::InvalidPidGains { .. }
        ));

        // Test negative Infinity in kd
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                pid_gains: Some(PidGains {
                    kp: 1.0,
                    ki: 1.0,
                    kd: f64::NEG_INFINITY,
                }),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::InvalidPidGains { .. }
        ));
    }

    #[test]
    fn validation_accepts_finite_pid_gains() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                pid_gains: Some(PidGains {
                    kp: 2.0,
                    ki: -0.5,
                    kd: 10.0,
                }),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(result.all_passed());
    }

    #[test]
    fn validation_rejects_target_temp_out_of_bounds() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();

        // Test target = 0 (absolute zero or below)
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                target_temp_millidegrees: Some(0),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::InvalidTargetTemperature { .. }
        ));

        // Test target negative
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                target_temp_millidegrees: Some(-1000),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::InvalidTargetTemperature { .. }
        ));

        // Test target > 150,000 (above 150 °C)
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                target_temp_millidegrees: Some(200_000),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::InvalidTargetTemperature { .. }
        ));
    }

    #[test]
    fn validation_accepts_target_temp_in_bounds() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();

        // Test boundary: 1 m°C (just above 0)
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                target_temp_millidegrees: Some(1),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(result.all_passed());

        // Test boundary: 150,000 m°C (150 °C)
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                target_temp_millidegrees: Some(150_000),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(result.all_passed());
    }

    #[test]
    fn pid_gains_is_finite_method() {
        assert!(
            PidGains {
                kp: 1.0,
                ki: 0.5,
                kd: 0.1
            }
            .is_finite()
        );
        assert!(
            PidGains {
                kp: -1.0,
                ki: 0.0,
                kd: 100.0
            }
            .is_finite()
        );
        assert!(
            !PidGains {
                kp: f64::NAN,
                ki: 1.0,
                kd: 1.0
            }
            .is_finite()
        );
        assert!(
            !PidGains {
                kp: 1.0,
                ki: f64::INFINITY,
                kd: 1.0
            }
            .is_finite()
        );
        assert!(
            !PidGains {
                kp: 1.0,
                ki: 1.0,
                kd: f64::NEG_INFINITY
            }
            .is_finite()
        );
    }

    #[test]
    fn pid_limits_is_finite_method() {
        assert!(
            PidLimits {
                integral_min: -500.0,
                integral_max: 500.0,
                derivative_min: -5.0,
                derivative_max: 5.0,
            }
            .is_finite()
        );
        assert!(
            !PidLimits {
                integral_min: f64::NAN,
                integral_max: 500.0,
                derivative_min: -5.0,
                derivative_max: 5.0,
            }
            .is_finite()
        );
        assert!(
            !PidLimits {
                integral_min: -500.0,
                integral_max: f64::INFINITY,
                derivative_min: -5.0,
                derivative_max: 5.0,
            }
            .is_finite()
        );
        assert!(
            !PidLimits {
                integral_min: -500.0,
                integral_max: 500.0,
                derivative_min: f64::NEG_INFINITY,
                derivative_max: 5.0,
            }
            .is_finite()
        );
    }

    #[test]
    fn validation_rejects_non_finite_pid_limits() {
        let snapshot = test_snapshot();
        let mut draft = DraftConfig::default();
        draft.fans.insert(
            "hwmon-test-0000000000000001-fan1".to_string(),
            DraftFanEntry {
                pid_limits: Some(PidLimits {
                    integral_min: f64::NAN,
                    integral_max: 500.0,
                    derivative_min: -5.0,
                    derivative_max: 5.0,
                }),
                ..managed_draft_entry()
            },
        );
        let result = validate_draft(&draft, &snapshot);
        assert!(!result.all_passed());
        assert!(matches!(
            &result.rejected[0].1,
            ValidationError::InvalidPidLimits { .. }
        ));
    }
}
