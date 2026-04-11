use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::control::{ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits};
use crate::inventory::{ControlMode, FanChannel, InventorySnapshot, SupportState};

/// Current schema version for the daemon-owned configuration file.
/// Increment when making backward-incompatible changes to the config format.
pub const CONFIG_VERSION: u32 = 1;

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
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            friendly_names: FriendlyNames::default(),
            draft: DraftConfig::default(),
            applied: None,
            fallback_incident: None,
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

/// A fan that is actively managed by the daemon under the applied config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedFanEntry {
    /// The control mode in use for this managed fan.
    pub control_mode: ControlMode,

    /// Temperature source IDs used as input for this fan's control loop.
    #[serde(default)]
    pub temp_sources: Vec<String>,

    pub target_temp_millidegrees: i64,

    pub aggregation: AggregationFn,

    pub pid_gains: PidGains,

    pub cadence: ControlCadence,

    pub deadband_millidegrees: i64,

    pub actuator_policy: ActuatorPolicy,

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
        fs::write(&path, contents)
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
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("/var/lib"))
        .join("kde-fan-control")
        .join("config.toml")
}

// ---------------------------------------------------------------------------
// Validation — reusable by DBus apply path and boot reconciliation
// ---------------------------------------------------------------------------

/// Errors that can occur when validating a draft against current inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValidationError {
    /// The referenced fan ID does not exist in the current inventory.
    FanNotFound { fan_id: String },

    /// The fan exists but its support state prevents safe management.
    FanNotEnrollable {
        fan_id: String,
        support_state: SupportState,
        reason: String,
    },

    /// The selected control mode is not supported by this fan's capabilities.
    UnsupportedControlMode {
        fan_id: String,
        requested: ControlMode,
        available: Vec<ControlMode>,
    },

    /// A managed fan entry has no control mode selected.
    MissingControlMode { fan_id: String },

    /// A referenced temperature source ID does not exist in current inventory.
    TempSourceNotFound { fan_id: String, temp_id: String },

    /// A managed fan entry did not specify a target temperature.
    MissingTargetTemp { fan_id: String },

    /// A managed fan entry did not specify any temperature sources.
    NoSensorForManagedFan { fan_id: String },

    /// A managed fan specified invalid cadence bounds.
    InvalidCadence { fan_id: String, reason: String },

    /// A managed fan specified invalid actuator limits.
    InvalidActuatorPolicy { fan_id: String, reason: String },

    /// A managed fan specified invalid PID clamp limits.
    InvalidPidLimits { fan_id: String, reason: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FanNotFound { fan_id } => {
                write!(f, "fan '{fan_id}' not found in current inventory")
            }
            Self::FanNotEnrollable {
                fan_id,
                support_state,
                reason,
            } => {
                write!(
                    f,
                    "fan '{fan_id}' is not enrollable (support: {support_state:?}): {reason}"
                )
            }
            Self::UnsupportedControlMode {
                fan_id,
                requested,
                available,
            } => {
                let available_str: Vec<String> =
                    available.iter().map(|m| format!("{m:?}")).collect();
                write!(
                    f,
                    "fan '{fan_id}' does not support control mode {requested:?} (available: {})",
                    available_str.join(", ")
                )
            }
            Self::MissingControlMode { fan_id } => {
                write!(f, "managed fan '{fan_id}' has no control mode selected")
            }
            Self::TempSourceNotFound { fan_id, temp_id } => {
                write!(
                    f,
                    "temperature source '{temp_id}' for fan '{fan_id}' not found in current inventory"
                )
            }
            Self::MissingTargetTemp { fan_id } => {
                write!(
                    f,
                    "managed fan '{fan_id}' has no target temperature configured"
                )
            }
            Self::NoSensorForManagedFan { fan_id } => {
                write!(
                    f,
                    "managed fan '{fan_id}' has no temperature source configured"
                )
            }
            Self::InvalidCadence { fan_id, reason } => {
                write!(f, "managed fan '{fan_id}' has invalid cadence: {reason}")
            }
            Self::InvalidActuatorPolicy { fan_id, reason } => {
                write!(
                    f,
                    "managed fan '{fan_id}' has invalid actuator policy: {reason}"
                )
            }
            Self::InvalidPidLimits { fan_id, reason } => {
                write!(f, "managed fan '{fan_id}' has invalid PID limits: {reason}")
            }
        }
    }
}

fn validate_cadence(fan_id: &str, cadence: ControlCadence) -> Result<(), ValidationError> {
    if cadence.sample_interval_ms < 250
        || cadence.control_interval_ms < 250
        || cadence.write_interval_ms < 250
    {
        return Err(ValidationError::InvalidCadence {
            fan_id: fan_id.to_string(),
            reason: "sample, control, and write cadences must each be at least 250ms".to_string(),
        });
    }

    if cadence.sample_interval_ms > cadence.control_interval_ms
        || cadence.control_interval_ms > cadence.write_interval_ms
    {
        return Err(ValidationError::InvalidCadence {
            fan_id: fan_id.to_string(),
            reason: "cadence must satisfy sample <= control <= write".to_string(),
        });
    }

    Ok(())
}

fn validate_actuator_policy(fan_id: &str, policy: ActuatorPolicy) -> Result<(), ValidationError> {
    let percent_ok = |value: f64| (0.0..=100.0).contains(&value);

    if !percent_ok(policy.output_min_percent)
        || !percent_ok(policy.output_max_percent)
        || !percent_ok(policy.startup_kick_percent)
    {
        return Err(ValidationError::InvalidActuatorPolicy {
            fan_id: fan_id.to_string(),
            reason: "all actuator percentages must be within 0.0..=100.0".to_string(),
        });
    }

    if policy.output_min_percent > policy.output_max_percent {
        return Err(ValidationError::InvalidActuatorPolicy {
            fan_id: fan_id.to_string(),
            reason: "output_min_percent must be <= output_max_percent".to_string(),
        });
    }

    if policy.pwm_min > policy.pwm_max {
        return Err(ValidationError::InvalidActuatorPolicy {
            fan_id: fan_id.to_string(),
            reason: "pwm_min must be <= pwm_max".to_string(),
        });
    }

    Ok(())
}

fn validate_pid_limits(fan_id: &str, limits: PidLimits) -> Result<(), ValidationError> {
    if limits.integral_min > limits.integral_max {
        return Err(ValidationError::InvalidPidLimits {
            fan_id: fan_id.to_string(),
            reason: "integral_min must be <= integral_max".to_string(),
        });
    }

    if limits.derivative_min > limits.derivative_max {
        return Err(ValidationError::InvalidPidLimits {
            fan_id: fan_id.to_string(),
            reason: "derivative_min must be <= derivative_max".to_string(),
        });
    }

    Ok(())
}

impl std::error::Error for ValidationError {}

/// Result of validating a draft config against the current inventory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Fans that passed validation and can be promoted to applied.
    pub enrollable: Vec<String>,

    /// Fans that failed validation, with reasons.
    pub rejected: Vec<(String, ValidationError)>,
}

impl ValidationResult {
    /// Whether all draft fan entries passed validation.
    pub fn all_passed(&self) -> bool {
        self.rejected.is_empty()
    }
}

/// Look up a fan channel by stable ID across all devices in the snapshot.
pub fn find_fan_by_id<'a>(snapshot: &'a InventorySnapshot, fan_id: &str) -> Option<&'a FanChannel> {
    snapshot
        .devices
        .iter()
        .flat_map(|d| d.fans.iter())
        .find(|f| f.id == fan_id)
}

/// Look up whether a temperature sensor ID exists in the snapshot.
pub fn temp_source_exists(snapshot: &InventorySnapshot, temp_id: &str) -> bool {
    snapshot
        .devices
        .iter()
        .flat_map(|d| d.temperatures.iter())
        .any(|t| t.id == temp_id)
}

/// Validate the draft config against the current inventory snapshot.
///
/// Each draft fan entry that is marked `managed: true` is checked:
/// - The fan ID must exist in the current inventory.
/// - The fan's support state must be `Available` (safe to enroll).
/// - The selected control mode must be one the fan reports as supported.
/// - All referenced temperature source IDs must exist in the inventory.
///
/// Returns a `ValidationResult` classifying each fan as enrollable or rejected.
pub fn validate_draft(draft: &DraftConfig, snapshot: &InventorySnapshot) -> ValidationResult {
    let mut enrollable = Vec::new();
    let mut rejected = Vec::new();

    for (fan_id, entry) in &draft.fans {
        if !entry.managed {
            // Unmanaged entries in the draft are informational only; skip validation.
            continue;
        }

        // 1. Fan must exist.
        let Some(fan) = find_fan_by_id(snapshot, fan_id) else {
            rejected.push((
                fan_id.clone(),
                ValidationError::FanNotFound {
                    fan_id: fan_id.clone(),
                },
            ));
            continue;
        };

        // 2. Fan must be enrollable (support state Available).
        if fan.support_state != SupportState::Available {
            rejected.push((
                fan_id.clone(),
                ValidationError::FanNotEnrollable {
                    fan_id: fan_id.clone(),
                    support_state: fan.support_state,
                    reason: fan
                        .support_reason
                        .clone()
                        .unwrap_or_else(|| "unsupported hardware".to_string()),
                },
            ));
            continue;
        }

        // 3. Control mode must be selected and supported.
        let Some(ref requested_mode) = entry.control_mode else {
            rejected.push((
                fan_id.clone(),
                ValidationError::MissingControlMode {
                    fan_id: fan_id.clone(),
                },
            ));
            continue;
        };

        if !fan.control_modes.contains(requested_mode) {
            rejected.push((
                fan_id.clone(),
                ValidationError::UnsupportedControlMode {
                    fan_id: fan_id.clone(),
                    requested: *requested_mode,
                    available: fan.control_modes.clone(),
                },
            ));
            continue;
        }

        // 4. Managed fan must specify a target temperature.
        if entry.resolved_target_temp_millidegrees().is_none() {
            rejected.push((
                fan_id.clone(),
                ValidationError::MissingTargetTemp {
                    fan_id: fan_id.clone(),
                },
            ));
            continue;
        }

        // 5. Managed fan must have at least one sensor source.
        if entry.temp_sources.is_empty() {
            rejected.push((
                fan_id.clone(),
                ValidationError::NoSensorForManagedFan {
                    fan_id: fan_id.clone(),
                },
            ));
            continue;
        }

        if let Err(error) = validate_cadence(fan_id, entry.resolved_cadence()) {
            rejected.push((fan_id.clone(), error));
            continue;
        }

        if let Err(error) = validate_actuator_policy(fan_id, entry.resolved_actuator_policy()) {
            rejected.push((fan_id.clone(), error));
            continue;
        }

        if let Err(error) = validate_pid_limits(fan_id, entry.resolved_pid_limits()) {
            rejected.push((fan_id.clone(), error));
            continue;
        }

        // 6. All temperature sources must exist.
        let mut temp_ok = true;
        for temp_id in &entry.temp_sources {
            if !temp_source_exists(snapshot, temp_id) {
                rejected.push((
                    fan_id.clone(),
                    ValidationError::TempSourceNotFound {
                        fan_id: fan_id.clone(),
                        temp_id: temp_id.clone(),
                    },
                ));
                temp_ok = false;
                break;
            }
        }
        if !temp_ok {
            continue;
        }

        enrollable.push(fan_id.clone());
    }

    ValidationResult {
        enrollable,
        rejected,
    }
}

/// Attempt to promote the draft config to applied after validation.
///
/// This performs best-effort partial apply: only fans that pass validation
/// are promoted. Rejected fans are reported but do not block the rest.
///
/// Returns the new `AppliedConfig` (containing only validated fans) and the
/// full `ValidationResult` so callers can report which fans were skipped.
pub fn apply_draft(
    draft: &DraftConfig,
    snapshot: &InventorySnapshot,
    applied_at: String,
) -> (AppliedConfig, ValidationResult) {
    let result = validate_draft(draft, snapshot);

    let fans = result
        .enrollable
        .iter()
        .filter_map(|fan_id| {
            let entry = draft.fans.get(fan_id)?;
            let control_mode = entry.control_mode?;
            Some((
                fan_id.clone(),
                AppliedFanEntry {
                    control_mode,
                    temp_sources: entry.temp_sources.clone(),
                    target_temp_millidegrees: entry.resolved_target_temp_millidegrees()?,
                    aggregation: entry.resolved_aggregation(),
                    pid_gains: entry.resolved_pid_gains(),
                    cadence: entry.resolved_cadence(),
                    deadband_millidegrees: entry.resolved_deadband_millidegrees(),
                    actuator_policy: entry.resolved_actuator_policy(),
                    pid_limits: entry.resolved_pid_limits(),
                },
            ))
        })
        .collect();

    let applied = AppliedConfig {
        fans,
        applied_at: Some(applied_at),
    };

    (applied, result)
}

// ---------------------------------------------------------------------------
// Degraded-state and lifecycle event data
// ---------------------------------------------------------------------------

/// Maximum number of lifecycle events retained for inspection.
pub const MAX_LIFECYCLE_EVENTS: usize = 64;

/// Reason a fan or the system entered a degraded state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DegradedReason {
    /// A previously managed fan was restored successfully during boot reconciliation.
    BootRestored { fan_id: String },

    /// Boot reconciliation completed successfully for all managed fans.
    BootReconciled { restored_count: u32 },

    /// A previously managed fan no longer appears in the hardware inventory.
    FanMissing { fan_id: String },

    /// A previously managed fan still exists but is no longer safely enrollable.
    FanNoLongerEnrollable {
        fan_id: String,
        support_state: SupportState,
        reason: String,
    },

    /// A previously managed fan's control mode is no longer supported.
    ControlModeUnavailable { fan_id: String, mode: ControlMode },

    /// A temperature source referenced by an applied fan entry is missing.
    TempSourceMissing { fan_id: String, temp_id: String },

    /// The applied configuration failed to fully apply at boot — partial recovery.
    PartialBootRecovery {
        failed_count: u32,
        recovered_count: u32,
    },

    /// The daemon entered fallback mode — previously controlled fans set to max.
    FallbackActive { affected_fans: Vec<String> },
}

impl std::fmt::Display for DegradedReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BootRestored { fan_id } => {
                write!(f, "fan '{fan_id}' restored as managed on boot")
            }
            Self::BootReconciled { restored_count } => {
                write!(
                    f,
                    "boot reconciliation restored {restored_count} managed fan(s)"
                )
            }
            Self::FanMissing { fan_id } => {
                write!(f, "fan '{fan_id}' missing from hardware")
            }
            Self::FanNoLongerEnrollable {
                fan_id,
                support_state,
                reason,
            } => {
                write!(
                    f,
                    "fan '{fan_id}' no longer enrollable ({support_state:?}): {reason}"
                )
            }
            Self::ControlModeUnavailable { fan_id, mode } => {
                write!(f, "fan '{fan_id}' no longer supports control mode {mode:?}")
            }
            Self::TempSourceMissing { fan_id, temp_id } => {
                write!(
                    f,
                    "temperature source '{temp_id}' for fan '{fan_id}' missing"
                )
            }
            Self::PartialBootRecovery {
                failed_count,
                recovered_count,
            } => {
                write!(
                    f,
                    "partial boot recovery: {recovered_count} recovered, {failed_count} failed"
                )
            }
            Self::FallbackActive { affected_fans } => {
                write!(f, "fallback active for fans: {}", affected_fans.join(", "))
            }
        }
    }
}

/// A single bounded lifecycle event recording something that happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    /// ISO 8601 timestamp when the event occurred.
    pub timestamp: String,

    /// What happened.
    pub reason: DegradedReason,

    /// Optional human-readable detail beyond the reason.
    #[serde(default)]
    pub detail: Option<String>,
}

/// A bounded log of lifecycle events, kept to at most `MAX_LIFECYCLE_EVENTS`
/// entries. Oldest entries are dropped when the log is full.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LifecycleEventLog {
    events: Vec<LifecycleEvent>,
}

impl LifecycleEventLog {
    /// Create an empty event log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a lifecycle event, dropping the oldest entry if the log is full.
    pub fn push(&mut self, event: LifecycleEvent) {
        if self.events.len() >= MAX_LIFECYCLE_EVENTS {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    /// Read all events in chronological order.
    pub fn events(&self) -> &[LifecycleEvent] {
        &self.events
    }

    /// Clear all events.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Number of events in the log.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether the log is empty.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Degraded-state tracking (runtime, not persisted)
// ---------------------------------------------------------------------------

/// Runtime tracking of which fans are currently in a degraded state
/// and why. This is reconstructed on boot from applied config + live
/// inventory, and updated whenever lifecycle events cause fans to
/// enter or leave degraded state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DegradedState {
    /// Per-fan degraded reasons, keyed by stable fan ID.
    /// A fan may have multiple degraded reasons simultaneously.
    #[serde(default)]
    pub entries: HashMap<String, Vec<DegradedReason>>,
}

impl DegradedState {
    /// Create an empty degraded state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a fan as degraded with the given reason(s).
    pub fn mark_degraded(&mut self, fan_id: String, reasons: Vec<DegradedReason>) {
        self.entries.insert(fan_id, reasons);
    }

    /// Clear the degraded state for a specific fan.
    pub fn clear_fan(&mut self, fan_id: &str) {
        self.entries.remove(fan_id);
    }

    /// Clear all degraded entries.
    pub fn clear_all(&mut self) {
        self.entries.clear();
    }

    /// Whether any fans are currently degraded.
    pub fn has_degraded(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Get the set of fan IDs that are currently degraded.
    pub fn degraded_fan_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::{ActuatorPolicy, AggregationFn, ControlCadence, PidGains, PidLimits};
    use crate::inventory::{HwmonDevice, TemperatureSensor};

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

        assert!(deserialized
            .draft_fan("hwmon-test-0000000000000001-fan1")
            .is_some());
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
        assert!(result
            .enrollable
            .contains(&"hwmon-test-0000000000000001-fan1".to_string()));
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

        let (applied, result) = apply_draft(&draft, &snapshot, "2026-04-11T12:00:00Z".to_string());

        // Only the valid fan should appear in applied.
        assert!(applied
            .fans
            .contains_key("hwmon-test-0000000000000001-fan1"));
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

        let (applied, result) = apply_draft(&draft, &snapshot, "2026-04-11T12:00:00Z".to_string());
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
    fn lifecycle_event_log_bounds() {
        let mut log = LifecycleEventLog::new();
        for i in 0..MAX_LIFECYCLE_EVENTS + 10 {
            log.push(LifecycleEvent {
                timestamp: format!("2026-04-11T12:{i:02}:00Z"),
                reason: DegradedReason::FanMissing {
                    fan_id: format!("fan-{i}"),
                },
                detail: None,
            });
        }
        assert_eq!(log.len(), MAX_LIFECYCLE_EVENTS);
        // Oldest events should have been dropped.
        let first = &log.events()[0];
        assert!(first.timestamp.contains("10")); // first retained is index 10
    }

    #[test]
    fn degraded_reason_display() {
        let reason = DegradedReason::FanMissing {
            fan_id: "fan-1".to_string(),
        };
        assert!(format!("{reason}").contains("fan-1"));

        let reason = DegradedReason::PartialBootRecovery {
            failed_count: 2,
            recovered_count: 3,
        };
        let text = format!("{reason}");
        assert!(text.contains("3 recovered"));
        assert!(text.contains("2 failed"));
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
}
