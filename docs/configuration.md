# Configuration Reference

The daemon's config file is its single source of truth. All fan enrollment, control profiles, and runtime state live here. The CLI and GUI are stateless clients that issue DBus method calls to the daemon, which reads and writes this file.

---

## File location

```
$XDG_STATE_DIR/kde-fan-control/config.toml
```

Fallbacks, searched in order:

1. `$XDG_STATE_DIR/kde-fan-control/config.toml`
2. `$XDG_DATA_HOME/kde-fan-control/config.toml`
3. `/var/lib/kde-fan-control/config.toml`

The daemon creates the directory and file on first run if they don't exist.

The daemon uses the [`dirs`](https://docs.rs/dirs) crate to resolve `XDG_STATE_DIR` and `XDG_DATA_HOME` per the XDG Base Directory Specification. On most Linux desktops, `XDG_STATE_DIR` defaults to `~/.local/state`.

---

## Schema version

```toml
version = 1
```

Current version: **1**

The `version` field tracks the config schema. Future incompatible changes will increment this number. The daemon rejects config files with a version higher than it supports — if you see a startup error about config version mismatch, the config was written by a newer daemon and needs manual migration.

---

## Top-level structure

```toml
version = 1

[friendly_names.sensors]
"hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1" = "CPU Temp"

[friendly_names.fans]
"hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1" = "CPU Fan"

[draft.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1]
managed = true
control_mode = "pwm"
temp_sources = ["hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1"]
target_temp_millidegrees = 65000
aggregation = "average"

[draft.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1.pid_gains]
kp = 1.0
ki = 1.0
kd = 0.5

[draft.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1.cadence]
sample_interval_ms = 250
control_interval_ms = 250
write_interval_ms = 250

[draft.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1.actuator_policy]
output_min_percent = 0.0
output_max_percent = 100.0
pwm_min = 0
pwm_max = 255
startup_kick_percent = 35.0
startup_kick_ms = 1500

[draft.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1.pid_limits]
integral_min = -500.0
integral_max = 500.0
derivative_min = -5.0
derivative_max = 5.0

[applied]
applied_at = "2026-04-11T12:00:00Z"

[applied.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1]
control_mode = "pwm"
temp_sources = ["hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1"]
target_temp_millidegrees = 65000
aggregation = "average"
deadband_millidegrees = 1000

[applied.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1.pid_gains]
kp = 1.0
ki = 1.0
kd = 0.5

[applied.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1.cadence]
sample_interval_ms = 250
control_interval_ms = 250
write_interval_ms = 250

[applied.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1.actuator_policy]
output_min_percent = 0.0
output_max_percent = 100.0
pwm_min = 0
pwm_max = 255
startup_kick_percent = 35.0
startup_kick_ms = 1500

[applied.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1.pid_limits]
integral_min = -500.0
integral_max = 500.0
derivative_min = -5.0
derivative_max = 5.0
```

---

## Field reference

### AppConfig (top level)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `version` | u32 | `1` | Schema version. Daemon rejects configs with a higher version. |
| `friendly_names` | [FriendlyNames](#friendlynames) | empty | User-assigned labels for sensors and fans. |
| `draft` | [DraftConfig](#draftconfig) | empty | Staged configuration — not live until explicitly applied. |
| `applied` | Option\<[AppliedConfig](#appliedconfig)\> | none | The live configuration the daemon uses for control loops and boot recovery. |
| `fallback_incident` | Option\<[FallbackIncident](#fallbackincident)\> | none | Record of the last fallback event (crash/shutdown). Cleared after successful boot reconciliation. |

### FriendlyNames

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `sensors` | HashMap\<String, String\> | empty | Maps stable sensor IDs to human-readable names. |
| `fans` | HashMap\<String, String\> | empty | Maps stable fan IDs to human-readable names. |

Example:

```toml
[friendly_names.sensors]
"hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1" = "CPU Temp"
"hwmon-nct6798-XXXXXXXXXXXXXXXX-temp2" = "System Temp"

[friendly_names.fans]
"hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1" = "CPU Fan"
"hwmon-nct6798-XXXXXXXXXXXXXXXX-fan2" = "Case Fan"
```

### DraftConfig

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `fans` | HashMap\<String, [DraftFanEntry](#draftfanentry)\> | empty | Per-fan draft enrollment entries, keyed by stable fan ID. |

### DraftFanEntry

A fan's staged configuration. Fields marked **optional** can be omitted from TOML — the daemon uses defaults when the draft is applied. All optional fields use `serde(default)`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `managed` | bool | `false` | Whether the daemon should control this fan when the draft is applied. Unmanaged entries are informational only and skip validation. |
| `control_mode` | Option\<String\> | none | `"pwm"` or `"voltage"`. Must match a mode the fan's hardware supports. Required for managed fans. |
| `temp_sources` | Vec\<String\> | empty | Stable IDs of temperature sensor(s) to use as PID input. At least one required for managed fans. |
| `target_temp_millidegrees` | Option\<i64\> | none | Target temperature in millidegrees Celsius. `65000` = 65 °C. Required for managed fans. |
| `aggregation` | Option\<String\> | `"average"` | How to combine multiple sensor readings: `"average"`, `"max"`, `"min"`, or `"median"`. |
| `pid_gains` | Option\<[PidGains](#pidgains)\> | defaults | Proportional/integral/derivative gains for the PID controller. |
| `cadence` | Option\<[ControlCadence](#controlcadence)\> | 250/250/250 | Sample, control, and write intervals in milliseconds. |
| `deadband_millidegrees` | Option\<i64\> | `1000` | Temperature deadband in millidegrees. The controller holds its previous output when the error is within this range. `1000` = 1 °C. |
| `actuator_policy` | Option\<[ActuatorPolicy](#actuatorpolicy)\> | defaults | PWM range, output clamping, and startup kick settings. |
| `pid_limits` | Option\<[PidLimits](#pidlimits)\> | defaults | Integral and derivative anti-windup clamp limits. |

### AppliedConfig

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `fans` | HashMap\<String, [AppliedFanEntry](#appliedfanentry)\> | — | Per-fan live entries. Only fans that passed validation appear here. |
| `applied_at` | Option\<String\> | none | ISO 8601 timestamp of when this config was promoted. |

### AppliedFanEntry

A fan that is actively managed by the daemon. All subtable fields use `serde(default)` for backward compatibility — a Phase 2 config that omits `pid_gains`, `cadence`, etc. will load with safe defaults.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `control_mode` | String | — | `"pwm"` or `"voltage"`. The active control mode. |
| `temp_sources` | Vec\<String\> | empty | Stable sensor IDs used as PID input. |
| `target_temp_millidegrees` | i64 | `65000` | Target temperature in millidegrees Celsius. Defaults to 65 °C (conservative — fans run moderately, not silent). |
| `aggregation` | String | `"average"` | Sensor aggregation function. |
| `pid_gains` | [PidGains](#pidgains) | defaults | PID controller gains. |
| `cadence` | [ControlCadence](#controlcadence) | 250/250/250 | Control loop timing. |
| `deadband_millidegrees` | i64 | `1000` | Temperature deadband. `1000` = 1 °C. |
| `actuator_policy` | [ActuatorPolicy](#actuatorpolicy) | defaults | PWM range and startup kick. |
| `pid_limits` | [PidLimits](#pidlimits) | defaults | Integral and derivative clamps. |

### PidGains

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `kp` | f64 | `1.0` | Proportional gain. Scales with the current temperature error. |
| `ki` | f64 | `1.0` | Integral gain. Accumulates error over time to eliminate steady-state offset. |
| `kd` | f64 | `0.5` | Derivative gain. Responds to the rate of temperature change (derivative-on-measurement, not error). |

The PID controller computes:

```
error_deg = (aggregated_temp_mdeg - target_temp_mdeg) / 1000.0
integral += error_deg × dt, clamped to [integral_min, integral_max]
derivative = clamp(-delta_measurement / dt, derivative_min, derivative_max)
output % = Kp × error_deg + Ki × integral + Kd × derivative
```

Output is clamped to [0, 100] before being written to the fan.

### ControlCadence

| Field | Type | Default | Description | Constraints |
|-------|------|---------|-------------|-------------|
| `sample_interval_ms` | u64 | `250` | How often the daemon reads temperature sensors. | ≥ 250, ≤ `control_interval_ms` |
| `control_interval_ms` | u64 | `250` | How often the PID calculation runs. | ≥ 250, ≤ `write_interval_ms` |
| `write_interval_ms` | u64 | `250` | How often the daemon writes PWM to sysfs. | ≥ 250 |

The ordering constraint is: `sample ≤ control ≤ write`. All intervals use `MissedTickBehavior::Skip` — if the system is overloaded, ticks are dropped rather than queued.

### ActuatorPolicy

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `output_min_percent` | f64 | `0.0` | Minimum logical output the controller can produce (0–100%). Useful to prevent a fan from stalling at very low duty cycles. |
| `output_max_percent` | f64 | `100.0` | Maximum logical output (0–100%). Useful to cap a noisy fan. |
| `pwm_min` | u16 | `0` | Minimum PWM value written to sysfs (0–255). Maps to `output_min_percent`. |
| `pwm_max` | u16 | `255` | Maximum PWM value written to sysfs (0–255). Maps to `output_max_percent`. |
| `startup_kick_percent` | f64 | `35.0` | Output percentage used for the startup kick pulse (0–100%). |
| `startup_kick_ms` | u64 | `1500` | Duration of the startup kick in milliseconds. |

The daemon maps logical output to PWM values linearly:

```
pwm_value = pwm_min + (output% / 100) × (pwm_max - pwm_min)
```

**Startup kick** — when the controller transitions from 0% to >0%, it writes `startup_kick_percent` for `startup_kick_ms` before switching to the calculated PID output. This prevents fan stall on low-PWM startup, where some fans need a brief pulse above their minimum stable speed.

Constraints: all percentages must be in `[0.0, 100.0]`, `output_min_percent ≤ output_max_percent`, and `pwm_min ≤ pwm_max`.

### PidLimits

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `integral_min` | f64 | `-500.0` | Lower bound for the integral accumulator. |
| `integral_max` | f64 | `500.0` | Upper bound for the integral accumulator. |
| `derivative_min` | f64 | `-5.0` | Lower bound for the derivative contribution. |
| `derivative_max` | f64 | `5.0` | Upper bound for the derivative contribution. |

These prevent windup after sustained periods when the fan cannot reach the target temperature (e.g., ambient temperature exceeds the target). Constraints: `integral_min ≤ integral_max` and `derivative_min ≤ derivative_max`.

### FallbackIncident

Written by the daemon when it triggers fallback (crash, shutdown, or runtime degradation). Persists across process exits so it can survive a daemon restart.

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | String (ISO 8601) | When the fallback was triggered. |
| `affected_fans` | Vec\<String\> | Stable fan IDs that were owned at the time of fallback. |
| `failed` | Vec\<[FallbackFailure](#fallbackfailure)\> | Fans where the PWM 255 write failed. |
| `detail` | Option\<String\> | Free-form explanation of why fallback was triggered. |

### FallbackFailure

| Field | Type | Description |
|-------|------|-------------|
| `fan_id` | String | Stable fan ID that the daemon attempted to write to. |
| `error` | String | Human-readable description of the write failure. |

---

## Stable IDs

Fan and sensor IDs are constructed from the hwmon device's kernel identity and channel number. The general form is:

```
hwmon-{chip_name}-{stable_identity_hash}-temp{N}
hwmon-{chip_name}-{stable_identity_hash}-fan{N}
```

These IDs are stable across reboots as long as the hardware topology doesn't change. The daemon discovers them by scanning `/sys/class/hwmon/hwmon*` and resolving udev metadata. You can see your system's IDs with:

```bash
kde-fan-control inventory
```

---

## Lifecycle: draft → apply

Changes to the running configuration follow a two-phase commit:

1. **Stage in draft** — Use the CLI (`enroll`, `unenroll`, `control set`) or GUI to add or modify fan entries in `[draft]`. Nothing is live yet.
2. **Validate** — `kde-fan-control validate` checks every `managed = true` draft entry against current hardware. Returns per-fan pass/fail with reasons. No state changes.
3. **Apply** — `kde-fan-control apply` promotes passing fans to `[applied]`, persists the config, claims fan ownership, and starts control loops.
4. **Partial apply** — Valid fans go live. Invalid fans remain in `[draft]` with rejection reasons. A single bad sensor doesn't block the rest.
5. **Previous fans preserved** — Fans in `[applied]` that are absent from `[draft]` are kept. Apply is additive — it only adds or updates, it doesn't remove.

Key invariants:

- **Only the daemon writes the config file.** CLI and GUI issue DBus method calls.
- **Apply is atomic from the fan's perspective.** A fan either enters the applied config with a complete valid entry, or it doesn't.
- **Boot reconciliation restores managed fans.** On daemon start, the applied config is validated against current hardware. Fans that still pass are automatically re-claimed.

---

## Backward compatibility

All fields introduced after Phase 1 (`pid_gains`, `cadence`, `deadband_millidegrees`, `actuator_policy`, `pid_limits`) have `serde(default)` attributes. A config file written by an earlier version of the daemon will load cleanly using safe defaults when read by a newer version.

For example, this Phase 2 config loads successfully in the current daemon:

```toml
version = 1
[friendly_names]
[draft]

[applied]
applied_at = "2026-04-10T12:00:00Z"

[applied.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1]
control_mode = "pwm"
temp_sources = ["hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1"]
```

The daemon fills in: `target_temp_millidegrees = 65000`, `aggregation = "average"`, `deadband_millidegrees = 1000`, and defaults for `pid_gains`, `cadence`, `actuator_policy`, and `pid_limits`.

---

## Control mode

| Mode | TOML value | Requirements |
|------|-----------|--------------|
| **PWM** | `"pwm"` | `pwmN` writable, `pwmN_enable` writable |
| **Voltage** | `"voltage"` | `pwmN` writable, `pwmN_enable` writable, `pwmN_mode` writable (exposed as `ControlMode::Voltage` in the fan's capabilities) |

Voltage mode is only available when the hwmon controller supports it — specifically, when `pwmN_mode` is writable. The daemon reports available modes per fan during hardware discovery. If you request a mode the fan doesn't support, validation rejects the entry.

---

## Aggregation functions

When a fan has multiple `temp_sources`, the daemon combines readings using the configured `aggregation` function before feeding the result to the PID controller:

| Value | Behavior |
|-------|----------|
| `"average"` | Arithmetic mean of all sensor readings. Good default — smooths out individual sensor noise. |
| `"max"` | Highest reading. Use when you want the fan to respond to the hottest sensor (e.g., multiple CPU core temps). |
| `"min"` | Lowest reading. Rarely useful, but available for niche setups. |
| `"median"` | Middle value (or average of the two middle values for even counts). Robust against outlier sensor readings. |

---

## Validation rules

The daemon enforces these rules during `validate` and `apply`:

| Check | Error |
|-------|-------|
| Fan ID exists in current hardware inventory | `FanNotFound` |
| Fan's support state is `Available` | `FanNotEnrollable` |
| `control_mode` is selected for `managed = true` fans | `MissingControlMode` |
| `control_mode` is in the fan's supported modes list | `UnsupportedControlMode` |
| `target_temp_millidegrees` is set for managed fans | `MissingTargetTemp` |
| `temp_sources` is non-empty for managed fans | `NoSensorForManagedFan` |
| All `temp_sources` IDs exist in current inventory | `TempSourceNotFound` |
| Cadence intervals ≥ 250 ms, and `sample ≤ control ≤ write` | `InvalidCadence` |
| Actuator percentages in `[0.0, 100.0]`, `output_min ≤ output_max`, `pwm_min ≤ pwm_max` | `InvalidActuatorPolicy` |
| `integral_min ≤ integral_max`, `derivative_min ≤ derivative_max` | `InvalidPidLimits` |

---

## Example configurations

### Single fan, PWM, all defaults

The minimum viable config for one managed fan:

```toml
version = 1

[draft.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1]
managed = true
control_mode = "pwm"
temp_sources = ["hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1"]
target_temp_millidegrees = 65000

[applied]
applied_at = "2026-04-11T12:00:00Z"

[applied.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1]
control_mode = "pwm"
temp_sources = ["hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1"]
target_temp_millidegrees = 65000
aggregation = "average"
deadband_millidegrees = 1000
```

All subtables (`pid_gains`, `cadence`, `actuator_policy`, `pid_limits`) are omitted — the daemon uses defaults.

### Multi-sensor aggregation with custom PID gains

A case fan driven by the maximum of two temperature sensors, with relaxed gains and a wider deadband to reduce oscillation:

```toml
version = 1

[friendly_names.sensors]
"hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1" = "CPU Core"
"hwmon-nct6798-XXXXXXXXXXXXXXXX-temp3" = "VRM"

[friendly_names.fans]
"hwmon-nct6798-XXXXXXXXXXXXXXXX-fan2" = "Case Intake"

[draft.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan2]
managed = true
control_mode = "pwm"
temp_sources = [
    "hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1",
    "hwmon-nct6798-XXXXXXXXXXXXXXXX-temp3",
]
target_temp_millidegrees = 70000
aggregation = "max"
deadband_millidegrees = 2000

[draft.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan2.pid_gains]
kp = 0.8
ki = 0.3
kd = 0.4

[applied]
applied_at = "2026-04-11T14:30:00Z"

[applied.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan2]
control_mode = "pwm"
temp_sources = [
    "hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1",
    "hwmon-nct6798-XXXXXXXXXXXXXXXX-temp3",
]
target_temp_millidegrees = 70000
aggregation = "max"
deadband_millidegrees = 2000

[applied.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan2.pid_gains]
kp = 0.8
ki = 0.3
kd = 0.4
```

- `aggregation = "max"` means the fan reacts to the hotter of the two sensors.
- `deadband_millidegrees = 2000` (2 °C) avoids rapid fan speed changes near the 70 °C target.
- Lower `ki` and `kd` compared to defaults reduce overshoot on a case fan with slower thermal response.

### Voltage control mode

Voltage control is only available when the hwmon controller exposes a writable `pwmN_mode` attribute. The daemon reports this per-fan during hardware discovery. If the fan's `control_modes` list includes `voltage`, you can use it:

```toml
version = 1

[draft.fans.hwmon-ite8613-XXXXXXXXXXXXXXXX-fan3]
managed = true
control_mode = "voltage"
temp_sources = ["hwmon-ite8613-XXXXXXXXXXXXXXXX-temp2"]
target_temp_millidegrees = 60000

[draft.fans.hwmon-ite8613-XXXXXXXXXXXXXXXX-fan3.actuator_policy]
output_min_percent = 20.0
output_max_percent = 90.0
pwm_min = 30
pwm_max = 230

[applied]
applied_at = "2026-04-11T16:00:00Z"

[applied.fans.hwmon-ite8613-XXXXXXXXXXXXXXXX-fan3]
control_mode = "voltage"
temp_sources = ["hwmon-ite8613-XXXXXXXXXXXXXXXX-temp2"]
target_temp_millidegrees = 60000
aggregation = "average"
deadband_millidegrees = 1500

[applied.fans.hwmon-ite8613-XXXXXXXXXXXXXXXX-fan3.actuator_policy]
output_min_percent = 20.0
output_max_percent = 90.0
pwm_min = 30
pwm_max = 230
startup_kick_percent = 35.0
startup_kick_ms = 1500
```

- `output_min_percent = 20.0` prevents the fan from dropping below 20%, avoiding stall on voltage-controlled fans that may not start at very low levels.
- `pwm_min = 30` / `pwm_max = 230` narrows the sysfs write range to stay within the controller's safe operating band.
- If you request `"voltage"` for a fan that only supports `"pwm"`, validation will reject the entry with `UnsupportedControlMode`.

### Custom cadence with slower write interval

For a fan on a shared hwmon controller where frequent sysfs writes cause contention:

```toml
[draft.fans.hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1.cadence]
sample_interval_ms = 500
control_interval_ms = 1000
write_interval_ms = 2000
```

The daemon samples temperature every 500 ms, runs the PID calculation every 1000 ms, and writes PWM every 2000 ms. This reduces sysfs bus traffic while still maintaining reasonable thermal response.

---

## Fallback incident examples

### Successful fallback (graceful shutdown)

```toml
[fallback_incident]
timestamp = "2026-04-11T18:30:00Z"
affected_fans = ["hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1"]
failed = []
detail = "SIGTERM received — graceful shutdown"
```

All owned fans were successfully written to PWM 255 before exit.

### Partial fallback (write failure)

```toml
[fallback_incident]
timestamp = "2026-04-11T18:35:00Z"
affected_fans = [
    "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1",
    "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan2",
]
failed = [
    { fan_id = "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan2", error = "permission denied" },
]
detail = "panic hook triggered fallback"
```

Fan 1 got PWM 255. Fan 2's write failed — it remains at whatever PWM value it had at the time of the crash. The daemon logs this so you can investigate after restart.

---

## Safety notes

- **Only the daemon writes to sysfs.** Never manually write `pwmN` or `pwmN_enable` while the daemon is running — the daemon will overwrite your values on its next write tick.
- **Fans not in the config are never touched.** The daemon only manages fans explicitly enrolled in `draft` and promoted to `applied`.
- **Fallback writes PWM 255 (full speed).** On daemon exit, crash, or sensor loss, all owned fans are driven to maximum speed. This is the safe default — it's better to have a loud fan than an overheating CPU.
- **Fallback incidents persist.** If the daemon crashes and restarts, it reads the `fallback_incident` from the config and reports it via `kde-fan-control events` and the GUI. The incident is cleared once boot reconciliation successfully restores all managed fans.
- **Config writes are atomic.** The daemon persists the full config on every state change (apply, enroll, unenroll, name change). There is no partial-write risk.