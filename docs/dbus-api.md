# KDE Fan Control — DBus Interface Contract

> This document is the authoritative specification for the KDE Fan Control DBus API.
> A conforming client implementation in any language should require only this document.

---

## 1. Overview

### Bus Details

| Property | Value |
|---|---|
| Bus | System bus (default); session bus when daemon is run with `--session-bus` |
| Bus name | `org.kde.FanControl` |
| Name owner | Root (UID 0) only |
| DBus policy file | `packaging/dbus/org.kde.FanControl.conf` |

### Object Tree

| Object Path | Interface |
|---|---|
| `/org/kde/FanControl` | `org.kde.FanControl.Inventory` |
| `/org/kde/FanControl/Lifecycle` | `org.kde.FanControl.Lifecycle` |
| `/org/kde/FanControl/Control` | `org.kde.FanControl.Control` |

### DBus Policy Summary

- Root may own the bus name `org.kde.FanControl`.
- All users may send messages to the destination.
- Write methods restrict execution via polkit (see §6).

### Type Conventions

All methods that return structured data return a JSON string (`s` in DBus type signature). The caller must parse the JSON according to the schemas defined in this document.

- JSON `null` represents an absent or unset value.
- JSON `true`/`false` represents boolean values.
- JSON arrays are ordered.
- JSON objects have the exact field names documented below; clients must ignore unknown fields for forward compatibility.

---

## 2. Interface: org.kde.FanControl.Inventory

**Path**: `/org/kde/FanControl`

### 2.1 Methods

| Method | Signature | Auth | Description |
|---|---|---|---|
| `Snapshot` | `→ s` | none | Returns full hardware inventory as a JSON string (see §5.1) |
| `SetSensorName` | `(id: s, name: s) → ()` | polkit | Assign a friendly name to a temperature sensor |
| `SetFanName` | `(id: s, name: s) → ()` | polkit | Assign a friendly name to a fan channel |
| `RemoveSensorName` | `(id: s) → ()` | polkit | Remove a sensor's friendly name |
| `RemoveFanName` | `(id: s) → ()` | polkit | Remove a fan's friendly name |

### 2.2 Method Details

#### `Snapshot() → s`

Returns a JSON string conforming to the `InventorySnapshot` schema (§5.1). The snapshot is a point-in-time view of all discovered hwmon devices, their temperature sensors, and their fan channels. No authentication is required; this is a read-open method.

#### `SetSensorName(id: s, name: s) → ()`

- `id`: The stable sensor ID (e.g. `hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1`).
- `name`: Human-readable friendly name. Must not be empty. Maximum 128 characters.
- The name is applied immediately to the in-memory snapshot. No signal is emitted.
- Errors: `org.kde.FanControl.Error.NotFound` if `id` does not match any known sensor.

#### `SetFanName(id: s, name: s) → ()`

- `id`: The stable fan ID (e.g. `hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1`).
- `name`: Human-readable friendly name. Must not be empty. Maximum 128 characters.
- The name is applied immediately to the in-memory snapshot. No signal is emitted.
- Errors: `org.kde.FanControl.Error.NotFound` if `id` does not match any known fan.

#### `RemoveSensorName(id: s) → ()`

- `id`: The stable sensor ID whose friendly name should be cleared.
- After removal, the sensor's `friendly_name` field in the snapshot will be `null`.
- Errors: `org.kde.FanControl.Error.NotFound` if `id` does not match any known sensor.

#### `RemoveFanName(id: s) → ()`

- `id`: The stable fan ID whose friendly name should be cleared.
- After removal, the fan's `friendly_name` field in the snapshot will be `null`.
- Errors: `org.kde.FanControl.Error.NotFound` if `id` does not match any known fan.

### 2.3 Signals

None.

### 2.4 Errors

| Error Name | Condition |
|---|---|
| `org.kde.FanControl.Error.NotFound` | Referenced `id` does not exist in the current inventory |
| `org.kde.FanControl.Error.NotPrivileged` | Write method called without polkit authorization (or UID-0 when polkit unavailable) |
| `org.kde.FanControl.Error.InvalidArgument` | `name` is empty or exceeds 128 characters |

---

## 3. Interface: org.kde.FanControl.Lifecycle

**Path**: `/org/kde/FanControl/Lifecycle`

### 3.1 Methods

**Read methods (no auth required):**

| Method | Signature | Auth | Description |
|---|---|---|---|
| `GetDraftConfig` | `→ s` | none | Returns current draft configuration as JSON (§5.2) |
| `GetAppliedConfig` | `→ s` | none | Returns current applied (live) configuration as JSON (§5.3). Returns the string `"null"` if no config is applied. |
| `GetDegradedSummary` | `→ s` | none | Returns degraded state summary as JSON (§5.4) |
| `GetLifecycleEvents` | `→ s` | none | Returns up to 64 recent lifecycle events as a JSON array (§5.7) |
| `GetRuntimeState` | `→ s` | none | Returns full runtime state as JSON (§5.8) |
| `GetOverviewStructure` | `→ s` | none | Returns overview structure snapshot as JSON (§5.12) |
| `GetOverviewTelemetry` | `→ s` | none | Returns overview telemetry batch as JSON (§5.13) |

**Write methods (polkit auth required):**

| Method | Signature | Auth | Description |
|---|---|---|---|
| `SetDraftFanEnrollment` | `(fan_id: s, managed: b, control_mode: s, temp_sources: as) → s` | root | Stage a fan enrollment change in the draft |
| `RemoveDraftFan` | `(fan_id: s) → ()` | root | Remove a fan from the draft config |
| `DiscardDraft` | `→ ()` | root | Clear the entire draft configuration |
| `ValidateDraft` | `→ s` | none | Validate draft against live inventory; returns `ValidationResult` JSON (§5.5) |
| `ApplyDraft` | `→ s` | polkit | Validate and promote draft to live; returns `ValidationResult` JSON (§5.5) |
| `RequestAuthorization` | `→ ()` | polkit | Proactively check/obtain polkit authorization; triggers auth dialog if needed |

### 3.2 Method Details

#### `GetDraftConfig() → s`

Returns a JSON string conforming to the `DraftConfig` schema (§5.2). If no draft exists, returns `{}` (an empty object with no `fans` key).

#### `GetAppliedConfig() → s`

Returns a JSON string conforming to the `AppliedConfig` schema (§5.3). If no configuration has been applied, returns the JSON literal string `"null"`.

#### `GetDegradedSummary() → s`

Returns a JSON string conforming to the `DegradedState` schema (§5.4). If there are no degraded entries, returns `{"entries": {}}`.

#### `GetLifecycleEvents() → s`

Returns a JSON array of up to 64 `LifecycleEvent` objects (§5.7), ordered most-recent-last. Events are retained in a circular buffer; older events are discarded.

#### `GetRuntimeState() → s`

Returns a JSON string conforming to the `RuntimeState` schema (§5.8).

#### `GetOverviewStructure() → s`

Returns a JSON string conforming to the `OverviewStructureSnapshot` schema (§5.12). Contains pre-computed display names, state badges, ordering buckets, and UI hints for each fan row. Intended for the GUI overview page's structural update path — row membership, order, and display properties that change rarely.

The daemon pre-formats strings (display names, state text, icon names, colors) so the GUI does not need to replicate the mapping logic. Rows are sorted by severity bucket (fallback → degraded → managed_hot → managed → unmanaged), then by display name within each bucket.

#### `GetOverviewTelemetry() → s`

Returns a JSON string conforming to the `OverviewTelemetryBatch` schema (§5.13). Contains live numeric values and pre-formatted display strings for each fan row. Intended for the GUI overview page's fast telemetry path — temperature, RPM, output percent, and alert flags that change every control loop tick.

Pre-formatted strings (`temperature_text`, `rpm_text`, `output_text`) avoid client-side string formatting and enable fixed-width monospace layout without width recalculation.

#### `SetDraftFanEnrollment(fan_id: s, managed: b, control_mode: s, temp_sources: as) → s`

- `fan_id`: Stable fan ID from inventory.
- `managed`: `true` to enroll the fan for daemon control; `false` to mark as unmanaged.
- `control_mode`: One of `"pwm"`, `"voltage"`, or `"none"`. Must match a mode in the fan's `control_modes` from inventory (when `managed` is `true`).
- `temp_sources`: Array of stable temperature sensor IDs to use as input sources for this fan's PID loop. At least one is required when `managed` is `true`.
- Returns the updated draft configuration as JSON (§5.2).
- If the fan already has a draft entry, it is overwritten. If not, a new entry is created with default values for all control-profile fields.
- Emits: `DraftChanged` signal.
- Errors: `org.kde.FanControl.Error.NotFound` if `fan_id` or any `temp_sources` entry does not exist in inventory.

#### `RemoveDraftFan(fan_id: s) → ()`

- Removes the fan's entry from the draft entirely.
- Emits: `DraftChanged` signal.
- No-op if the fan is not present in the draft.

#### `DiscardDraft() → ()`

- Clears the entire draft configuration. The draft becomes empty (`{}`).
- Emits: `DraftChanged` signal.

#### `ValidateDraft() → s`

- Validates the current draft against the live hardware inventory without modifying state.
- Returns `ValidationResult` JSON (§5.5).
- Validation checks include but are not limited to:
  - Every enrolled fan still exists in inventory.
  - Every enrolled fan's `control_mode` is in its `control_modes` list.
  - Every `temp_sources` entry still exists in inventory.
  - No duplicate temp source references.
- This method is a dry-run; it does not change draft or applied state.

#### `ApplyDraft() → s`

- Validates the draft (same checks as `ValidateDraft`), then promotes passing entries to the applied (live) configuration.
- Fans that pass validation become live: the daemon claims PWM ownership, starts control tasks, and persists the config.
- Fans that fail validation remain in the draft for correction.
- Returns `ValidationResult` JSON (§5.5) showing which fans were enrolled and which were rejected.
- Emits `AppliedConfigChanged` if any fans were promoted.
- Emits `DraftChanged` if any fans were rejected (they remain in draft).
- This is the **commit point**: config is persisted to disk, fan ownership is claimed, control tasks are started.
- Errors: `org.kde.FanControl.Error.DraftEmpty` if the draft has no fans.

### 3.3 Signals

| Signal | Signature | Description |
|---|---|---|
| `DraftChanged` | `()` | Emitted when the draft configuration changes (enrollment, removal, discard, auto-tune acceptance, control profile update) |
| `AppliedConfigChanged` | `()` | Emitted when the applied configuration changes (after successful `ApplyDraft`) |
| `DegradedStateChanged` | `()` | Emitted when the set of degraded entries changes |
| `LifecycleEventAppended` | `(event_kind: s, detail: s)` | Emitted when a new lifecycle event is recorded. `event_kind` is one of the `LifecycleEventKind` values; `detail` is a human-readable summary string |

### 3.4 Errors

| Error Name | Condition |
|---|---|
| `org.kde.FanControl.Error.NotPrivileged` | Write method called without polkit authorization (or UID-0 when polkit unavailable) |
| `org.kde.FanControl.Error.NotFound` | Referenced `fan_id` or `temp_id` does not exist |
| `org.kde.FanControl.Error.DraftEmpty` | `ApplyDraft` called with an empty draft |
| `org.kde.FanControl.Error.InvalidArgument` | `control_mode` is not valid for this fan, or `temp_sources` is empty when `managed` is true |

---

## 4. Interface: org.kde.FanControl.Control

**Path**: `/org/kde/FanControl/Control`

### 4.1 Methods

| Method | Signature | Auth | Description |
|---|---|---|---|
| `GetControlStatus` | `→ s` | none | Returns live control runtime status for all managed fans as JSON (§5.9) |
| `GetAutoTuneResult` | `(fan_id: s) → s` | none | Returns auto-tune result for a specific fan as JSON (§5.10) |
| `StartAutoTune` | `(fan_id: s) → ()` | polkit | Start bounded auto-tune observation for a fan |
| `AcceptAutoTune` | `(fan_id: s) → s` | polkit | Accept the latest completed auto-tune proposal into draft config |
| `SetDraftFanControlProfile` | `(fan_id: s, profile_json: s) → s` | polkit | Set control profile fields for a fan's draft entry |

### 4.2 Method Details

#### `GetControlStatus() → s`

- Returns a JSON object mapping fan IDs to `ControlRuntimeSnapshot` objects (§5.9).
- Only fans that are currently managed (status `"managed"`) appear.
- An empty object `{}` is returned if no fans are managed.

#### `GetAutoTuneResult(fan_id: s) → s`

- `fan_id`: The stable fan ID to query.
- Returns `AutoTuneResult` JSON (§5.10).
- Works for any fan, regardless of auto-tune state. If auto-tune has never been started for this fan, `status` is `"idle"`.
- Errors: `org.kde.FanControl.Error.NotFound` if `fan_id` does not exist.

#### `StartAutoTune(fan_id: s) → ()`

- Starts a bounded auto-tune observation window for the specified fan.
- Preconditions:
  - The fan must be in the applied (live) config with `managed: true`.
  - The fan must currently be owned by the daemon (i.e., its `status` is `"managed"`).
- During the observation window, the fan is driven to 100% PWM. The daemon records temperature response data across the observation window (default 30 seconds).
- Only one auto-tune may be active at a time across the entire daemon. If another auto-tune is already running, returns `org.kde.FanControl.Error.Busy`.
- Emits `AutoTuneCompleted(fan_id)` when the observation window completes.
- Errors: `org.kde.FanControl.Error.NotFound`, `org.kde.FanControl.Error.NotManaged`, `org.kde.FanControl.Error.Busy`.

#### `AcceptAutoTune(fan_id: s) → s`

- Writes the proposed gains from the latest completed auto-tune into the draft config entry for this fan.
- The auto-tune must have completed successfully (`status: "completed"`). If it has not completed, returns `org.kde.FanControl.Error.AutoTuneNotReady`.
- The fan must have a draft entry. If it does not, one is created from the current applied entry with only the PID gains overwritten.
- Returns the updated draft entry JSON.
- Changes are staged only — the caller must invoke `ApplyDraft` to make changes live.
- Emits `DraftChanged` signal.
- Errors: `org.kde.FanControl.Error.NotFound`, `org.kde.FanControl.Error.AutoTuneNotReady`.

#### `SetDraftFanControlProfile(fan_id: s, profile_json: s) → s`

- `fan_id`: The stable fan ID whose draft entry should be updated.
- `profile_json`: A JSON string containing a partial `DraftFanControlProfilePayload` object (§5.11). Only the fields present in the JSON are updated; omitted fields retain their current values.
- The fan must already have a draft entry (created by `SetDraftFanEnrollment`). If it does not, returns `org.kde.FanControl.Error.NotFound`.
- Returns the updated draft entry JSON.
- Emits `DraftChanged` signal.
- Errors: `org.kde.FanControl.Error.NotFound`, `org.kde.FanControl.Error.InvalidArgument` (malformed JSON or out-of-range values).

### 4.3 Signals

| Signal | Signature | Description |
|---|---|---|
| `ControlStatusChanged` | `()` | Emitted when live control status changes (after apply, during runtime PID updates are batched, not per-tick) |
| `AutoTuneCompleted` | `(fan_id: s)` | Emitted when an auto-tune observation window completes. The `fan_id` identifies which fan's auto-tune finished |

### 4.4 Errors

| Error Name | Condition |
|---|---|
| `org.kde.FanControl.Error.NotPrivileged` | Write method called without polkit authorization (or UID-0 when polkit unavailable) |
| `org.kde.FanControl.Error.NotFound` | Referenced `fan_id` does not exist in draft or inventory |
| `org.kde.FanControl.Error.NotManaged` | Fan is not currently managed by the daemon |
| `org.kde.FanControl.Error.Busy` | Another auto-tune is already running |
| `org.kde.FanControl.Error.AutoTuneNotReady` | Auto-tune has not completed for this fan |
| `org.kde.FanControl.Error.InvalidArgument` | Malformed `profile_json` or out-of-range values |

---

## 5. Type Reference

### 5.1 InventorySnapshot

Root object returned by `Snapshot()`.

```json
{
  "devices": [InventoryDevice]
}
```

#### InventoryDevice

| Field | Type | Description |
|---|---|---|
| `id` | string | Stable device ID: `hwmon-{sanitized_name}-{fnv1a_hex16}`. The hash is FNV-1a of the canonical sysfs device path. Survives hwmon number changes across reboots. |
| `name` | string | Chip name as exposed by the hwmon `name` attribute (e.g. `"nct6798"`) |
| `sysfs_path` | string | `/sys/class/hwmon/hwmonN` path (unstable across reboots; use `stable_identity` for persistence) |
| `stable_identity` | string | Canonical device path from the kernel device tree (e.g. `/sys/devices/platform/nct6798.656`). Used as the input to FNV-1a hash for the `id` field. |
| `temperatures` | array of InventoryTemperature | Temperature sensor channels on this device |
| `fans` | array of InventoryFan | Fan channels on this device |

#### InventoryTemperature

| Field | Type | Description |
|---|---|---|
| `id` | string | Stable sensor ID: `{device_id}-temp{N}` where N is the hwmon channel index |
| `channel` | integer (≥1) | hwmon channel number (corresponds to `temp{N}_input`) |
| `label` | string or null | Label from `temp{N}_label` sysfs attribute, or null if the label file does not exist |
| `friendly_name` | string or null | User-assigned friendly name via `SetSensorName`, or null if unset |
| `input_millidegrees_celsius` | integer | Current temperature in millidegrees Celsius as read from `temp{N}_input`. Kernel hwmon fixed-point: 45000 = 45.0°C. Value is -1 if the sensor is temporarily unreadable. |

#### InventoryFan

| Field | Type | Description |
|---|---|---|
| `id` | string | Stable fan ID: `{device_id}-fan{N}` where N is the hwmon channel index |
| `channel` | integer (≥1) | hwmon channel number (corresponds to `fan{N}_input`, `pwm{N}`) |
| `label` | string or null | Label from `fan{N}_label` sysfs attribute, or null if the label file does not exist |
| `friendly_name` | string or null | User-assigned friendly name via `SetFanName`, or null if unset |
| `rpm_feedback` | boolean | `true` if `fan{N}_input` sysfs node exists (tachometer reading available) |
| `current_rpm` | integer or null | Current RPM reading from `fan{N}_input`. Only present when `rpm_feedback` is `true`. Value is 0 if the tach read fails. |
| `control_modes` | array of string | Available control modes: `["pwm"]` if only `pwm{N}` is writable; `["pwm", "voltage"]` if additionally `pwm{N}_mode` is writable. Empty array if `support_state` is `"unavailable"`. |
| `support_state` | string | One of: `"available"` (writable `pwm{N}` node exists), `"partial"` (read-only PWM or tach-only), `"unavailable"` (no control node) |
| `support_reason` | string or null | Human-readable explanation when `support_state` is `"partial"` or `"unavailable"`. Always null when `"available"`. |

### 5.2 DraftConfig

Root object returned by `GetDraftConfig()`, `SetDraftFanEnrollment()`, `AcceptAutoTune()`, and `SetDraftFanControlProfile()`.

```json
{
  "fans": {
    "<fan_id>": DraftFanEntry
  }
}
```

#### DraftFanEntry

| Field | Type | Description |
|---|---|---|
| `managed` | boolean | Whether the daemon controls this fan. Always `true` for entries created by `SetDraftFanEnrollment`. |
| `control_mode` | string | One of `"pwm"`, `"voltage"`, `"none"` |
| `temp_sources` | array of string | Stable temperature sensor IDs used as PID input |
| `target_temp_millidegrees` | integer | Target temperature in millidegrees Celsius (e.g. 65000 = 65.0°C). Default: 65000. |
| `aggregation` | string | How multiple temp sources are combined: `"average"`, `"maximum"`, `"minimum"`. Default: `"average"`. |
| `pid_gains` | PidGains | PID controller gains |
| `cadence` | Cadence | Control loop timing |
| `deadband_millidegrees` | integer | Temperature deadband in millidegrees Celsius. PID output is not updated when the error is within this band. Default: 1000. |
| `actuator_policy` | ActuatorPolicy | Output mapping and startup behavior |
| `pid_limits` | PidLimits | Integrator and derivative clamping |

#### PidGains

| Field | Type | Description |
|---|---|---|
| `kp` | float | Proportional gain. Default: 1.0. |
| `ki` | float | Integral gain. Default: 1.0. |
| `kd` | float | Derivative gain. Default: 0.5. |

#### Cadence

| Field | Type | Description |
|---|---|---|
| `sample_interval_ms` | integer | Temperature sampling period in milliseconds. Default: 250. |
| `control_interval_ms` | integer | PID calculation period in milliseconds. Default: 250. |
| `write_interval_ms` | integer | PWM write period in milliseconds. Default: 250. |

#### ActuatorPolicy

| Field | Type | Description |
|---|---|---|
| `output_min_percent` | float | Minimum logical output as a percentage [0.0, 100.0]. Default: 0.0. |
| `output_max_percent` | float | Maximum logical output as a percentage [0.0, 100.0]. Default: 100.0. |
| `pwm_min` | integer | Minimum raw PWM value [0, 255]. Default: 0. |
| `pwm_max` | integer | Maximum raw PWM value [0, 255]. Default: 255. |
| `startup_kick_percent` | float | Output percentage applied during startup kick phase. Default: 35.0. |
| `startup_kick_ms` | integer | Duration of the startup kick in milliseconds. Default: 1500. |

#### PidLimits

| Field | Type | Description |
|---|---|---|
| `integral_min` | float | Minimum integrator accumulator value. Default: -500.0. |
| `integral_max` | float | Maximum integrator accumulator value. Default: 500.0. |
| `derivative_min` | float | Minimum derivative term clamp value. Default: -5.0. |
| `derivative_max` | float | Maximum derivative term clamp value. Default: 5.0. |

### 5.3 AppliedConfig

Root object returned by `GetAppliedConfig()`. Shares the same fan-entry shape as `DraftConfig`, but with these differences:

- All fields on every fan entry are required (no optional fields). If a field had a default in the draft, that default is materialized.
- An additional `applied_at` field is present on each fan entry:

| Field | Type | Description |
|---|---|---|
| `applied_at` | integer | Unix timestamp (seconds since epoch) when this fan entry was last promoted to applied |

Returns the JSON literal `"null"` if no configuration has been applied.

### 5.4 DegradedState

Root object returned by `GetDegradedSummary()`.

```json
{
  "entries": {
    "<fan_id>": [DegradedReason]
  }
}
```

Empty degraded state: `{"entries": {}}`.

#### DegradedReason

A tagged union discriminated by the `"kind"` field.

| kind | Additional fields | Description |
|---|---|---|
| `boot_restored` | none | All fans restored successfully from persisted config at boot |
| `boot_reconciled` | none | Fan configuration reconciled at boot with minor adjustments |
| `fan_missing` | `fan_id: string` | Fan no longer present in hardware inventory |
| `fan_no_longer_enrollable` | `fan_id: string` | Fan exists but is unsafe or unable to manage |
| `control_mode_unavailable` | `fan_id: string` | Configured control mode no longer supported by hardware |
| `temp_source_missing` | `fan_id: string`, `temp_id: string` | Referenced temperature sensor is gone |
| `partial_boot_recovery` | none | Some fans restored, some failed at boot |
| `fallback_active` | none | Daemon entered fallback mode; all owned fans driven to safe maximum PWM |

Example:

```json
{
  "kind": "temp_source_missing",
  "fan_id": "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1",
  "temp_id": "hwmon-XXX-temp3"
}
```

### 5.5 ValidationResult

Root object returned by `ValidateDraft()` and `ApplyDraft()`.

```json
{
  "enrollable": [string],
  "rejected": [[string, DegradedReason]]
}
```

| Field | Type | Description |
|---|---|---|
| `enrollable` | array of string | Fan IDs that passed validation. After `ApplyDraft`, these are now live. |
| `rejected` | array of [string, DegradedReason] | Pairs of (fan_id, reason) for fans that failed validation. After `ApplyDraft`, these remain in the draft. |

### 5.6 DraftFanEntry (full defaults)

When `SetDraftFanEnrollment` creates a new draft entry, defaults are:

| Field | Default |
|---|---|
| `target_temp_millidegrees` | 65000 |
| `aggregation` | `"average"` |
| `pid_gains` | `{"kp": 1.0, "ki": 1.0, "kd": 0.5}` |
| `cadence` | `{"sample_interval_ms": 250, "control_interval_ms": 250, "write_interval_ms": 250}` |
| `deadband_millidegrees` | 1000 |
| `actuator_policy` | `{"output_min_percent": 0.0, "output_max_percent": 100.0, "pwm_min": 0, "pwm_max": 255, "startup_kick_percent": 35.0, "startup_kick_ms": 1500}` |
| `pid_limits` | `{"integral_min": -500.0, "integral_max": 500.0, "derivative_min": -5.0, "derivative_max": 5.0}` |

### 5.7 LifecycleEvent

```json
{
  "timestamp": 1712345678,
  "kind": "applied",
  "detail": "3 fans enrolled, 1 rejected"
}
```

| Field | Type | Description |
|---|---|---|
| `timestamp` | integer | Unix timestamp (seconds since epoch) |
| `kind` | string | Event kind (see below) |
| `detail` | string | Human-readable summary |

**LifecycleEventKind values:**

`boot`, `boot_restored`, `boot_reconciled`, `partial_boot_recovery`, `applied`, `discarded`, `degraded`, `recovered`, `fallback_entered`, `fallback_exited`

### 5.8 RuntimeState

Root object returned by `GetRuntimeState()`.

```json
{
  "owned_fans": ["hwmon-...-fan1"],
  "fan_statuses": {
    "<fan_id>": FanRuntimeStatus
  }
}
```

| Field | Type | Description |
|---|---|---|
| `owned_fans` | array of string | Fan IDs currently under daemon PWM ownership |
| `fan_statuses` | object | Map of fan_id → FanRuntimeStatus for all fans in the applied config |

#### FanRuntimeStatus

| Field | Type | Description |
|---|---|---|
| `status` | string | One of: `"unmanaged"`, `"managed"`, `"degraded"`, `"fallback"` |
| `control_mode` | string or null | Current control mode. Present when status is `"managed"` or `"degraded"`. Null when `"unmanaged"` or `"fallback"`. |
| `control` | ControlRuntimeSnapshot or null | Live control data. Present only when status is `"managed"`. |
| `reasons` | array of DegradedReason or null | Degraded reasons. Present only when status is `"degraded"`. |

### 5.9 ControlRuntimeSnapshot

Per-fan live control data, returned by `GetControlStatus()` and embedded in `RuntimeState.fan_statuses` when status is `"managed"`.

```json
{
  "sensor_ids": ["hwmon-...-temp1"],
  "aggregation": "average",
  "target_temp_millidegrees": 65000,
  "aggregated_temp_millidegrees": 55000,
  "logical_output_percent": 42.5,
  "mapped_pwm": 108,
  "auto_tuning": false,
  "alert_high_temp": false,
  "last_error_millidegrees": -10000
}
```

| Field | Type | Description |
|---|---|---|
| `sensor_ids` | array of string | Temperature sensor IDs being sampled |
| `aggregation` | string | Aggregation method applied to sensor readings: `"average"`, `"maximum"`, `"minimum"` |
| `target_temp_millidegrees` | integer | Configured target in millidegrees Celsius |
| `aggregated_temp_millidegrees` | integer | Current aggregated temperature reading in millidegrees Celsius |
| `logical_output_percent` | float | PID controller output as a percentage [0.0, 100.0] |
| `mapped_pwm` | integer | Raw PWM value written to the hwmon node [0, 255], after actuator mapping |
| `auto_tuning` | boolean | `true` if auto-tune is currently running for this fan |
| `alert_high_temp` | boolean | `true` if the aggregated temperature exceeds a safety threshold and the controller has been overridden to 100% |
| `last_error_millidegrees` | integer | Most recent PID error term (target − aggregated) in millidegrees Celsius. Negative means below target. |

### 5.10 AutoTuneResult

Returned by `GetAutoTuneResult(fan_id)`.

```json
{
  "status": "idle",
  "observation_window_ms": 30000,
  "proposal": null,
  "error": null
}
```

| Field | Type | Description |
|---|---|---|
| `status` | string | One of: `"idle"`, `"running"`, `"completed"`, `"failed"` |
| `observation_window_ms` | integer | Configured observation window duration in milliseconds. Present in all states. Default: 30000. |
| `proposal` | AutoTuneProposal or null | Present only when `status` is `"completed"`. Null otherwise. |
| `error` | string or null | Human-readable error message. Present only when `status` is `"failed"`. Null otherwise. |

#### AutoTuneProposal

```json
{
  "proposed_gains": { "kp": 0.072, "ki": 0.006, "kd": 0.9 },
  "observation_window_ms": 30000,
  "lag_time_ms": 5000,
  "max_rate_c_per_sec": 2.0
}
```

| Field | Type | Description |
|---|---|---|
| `proposed_gains` | PidGains | Computed PID gains suitable for this fan/thermal system |
| `observation_window_ms` | integer | Duration of the observation window actually used |
| `lag_time_ms` | integer | Estimated thermal lag between fan output change and temperature response |
| `max_rate_c_per_sec` | float | Maximum observed temperature rate of change in °C/s during the 100% drive phase |

### 5.11 DraftFanControlProfilePayload

Partial update object accepted by `SetDraftFanControlProfile`. All fields are optional; only provided fields are written.

```json
{
  "target_temp_millidegrees": 65000,
  "aggregation": "average",
  "pid_gains": { "kp": 1.0, "ki": 1.0, "kd": 0.5 },
  "cadence": { "sample_interval_ms": 250, "control_interval_ms": 250, "write_interval_ms": 250 },
  "deadband_millidegrees": 1000,
  "actuator_policy": {
    "output_min_percent": 0.0,
    "output_max_percent": 100.0,
    "pwm_min": 0,
    "pwm_max": 255,
    "startup_kick_percent": 35.0,
    "startup_kick_ms": 1500
  },
  "pid_limits": {
    "integral_min": -500.0,
    "integral_max": 500.0,
    "derivative_min": -5.0,
    "derivative_max": 5.0
  }
}
```

When a nested object (e.g. `pid_gains`) is provided, it replaces all fields within that sub-object. To update only `kp` while preserving `ki` and `kd`, the caller must read the current draft first, merge locally, then write the complete `pid_gains` object.

### 5.12 OverviewStructureSnapshot

Root object returned by `GetOverviewStructure()`.

```json
{
  "rows": [OverviewStructureRow]
}
```

#### OverviewStructureRow

| Field | Type | Description |
|---|---|---|
| `fan_id` | string | Stable fan ID from inventory |
| `display_name` | string | Human-readable name: friendly_name > label > fan_id |
| `friendly_name` | string or null | User-assigned friendly name via `SetFanName`, or null if unset |
| `hardware_label` | string or null | Label from `fan{N}_label` sysfs attribute, or null if absent |
| `support_state` | string | One of: `"available"`, `"partial"`, `"unavailable"` |
| `control_mode` | string or null | Current control mode (e.g. `"pwm"`). Present when managed. Null otherwise. |
| `has_tach` | boolean | `true` if tachometer reading is available |
| `support_reason` | string or null | Human-readable explanation when `support_state` is not `"available"` |
| `ordering_bucket` | string | Severity bucket for row ordering: `"fallback"`, `"degraded"`, `"managed_hot"`, `"managed"`, `"unmanaged"` |
| `state_text` | string | Pre-formatted badge text: `"Managed"`, `"Unmanaged"`, `"Degraded"`, `"Fallback"` |
| `state_icon_name` | string | Pre-formatted freedesktop icon name: `"emblem-ok-symbolic"`, `"dialog-information-symbolic"`, `"data-warning-symbolic"`, `"dialog-error-symbolic"` |
| `state_color` | string | Pre-formatted hex color for badge background: `"#43a047"` (managed), `"#9e9e9e"` (unmanaged), `"#ff9800"` (degraded), `"#e53935"` (fallback/managed_hot) |
| `show_support_reason` | boolean | `true` when the support reason row should be displayed (unmanaged or degraded fans with a support_reason) |

Rows are sorted by `ordering_bucket` severity (fallback=0, degraded=1, managed_hot=2, managed=3, unmanaged=4), then by `display_name` alphabetically within each bucket.

### 5.13 OverviewTelemetryBatch

Root object returned by `GetOverviewTelemetry()`.

```json
{
  "rows": [OverviewTelemetryRow]
}
```

#### OverviewTelemetryRow

| Field | Type | Description |
|---|---|---|
| `fan_id` | string | Stable fan ID matching an `OverviewStructureRow.fan_id` |
| `temperature_millidegrees` | integer | Current aggregated temperature in millidegrees Celsius. 0 if no live reading. |
| `temperature_text` | string | Pre-formatted temperature display string: `"55.2 °C"` or `"No live reading"` |
| `rpm` | integer | Current RPM reading. 0 if no tach or unreadable. |
| `rpm_text` | string | Pre-formatted RPM display string: `"1240 RPM"`, `"0 RPM"`, or `"No RPM feedback"` |
| `output_percent` | float | PID output as a percentage [0.0, 100.0]. 100.0 for degraded/fallback fans. |
| `output_text` | string | Pre-formatted output display string: `"42.5%"` or `"No control"` |
| `output_fill_ratio` | float | Normalized fill ratio for UI bar: `output_percent / 100.0`, clamped to [0.0, 1.0] |
| `high_temp_alert` | boolean | `true` if aggregated temperature exceeds the safety threshold |
| `show_rpm` | boolean | `true` when the RPM field should be displayed (managed/degraded/fallback fans with tach) |
| `show_output` | boolean | `true` when the output bar should be displayed (managed/degraded/fallback fans) |
| `visual_state` | string | State for UI badge coloring: `"managed"`, `"managed_hot"`, `"degraded"`, `"fallback"`, `"unmanaged"` |

The `visual_state` field distinguishes `"managed_hot"` (managed fan with high-temp alert) from `"managed"` so the GUI can render a different badge color without recalculating the condition.

---

## 6. Authorization Model

### Current

All write methods require polkit authorization using the action ID
`org.kde.fancontrol.write-config` with the implicit authorization mode
`auth_admin_keep`. The daemon calls `org.freedesktop.PolicyKit1.Authority.CheckAuthorization()`
with `AllowUserInteraction=1`, which triggers a graphical authentication dialog
in the caller's desktop session.

If the polkit authority is unavailable (e.g. no polkit daemon running in a
headless/SSH session), the daemon falls back to a UID-0 check: only root
callers are authorized.

The `RequestAuthorization` method on the Lifecycle interface allows the GUI
to proactively trigger the polkit authentication dialog before performing any
write operation. This enables the lock/unlock UX pattern.

### Polkit Policy

Installed at `/usr/share/polkit-1/actions/org.kde.fancontrol.policy`:

| Action ID | `allow_any` | `allow_inactive` | `allow_active` |
|---|---|---|---|
| `org.kde.fancontrol.write-config` | `auth_admin` | `auth_admin` | `auth_admin_keep` |

### Auth Summary Per Method

| Interface | Method | Auth |
|---|---|---|
| Inventory | `Snapshot` | none |
| Inventory | `SetSensorName` | polkit |
| Inventory | `SetFanName` | polkit |
| Inventory | `RemoveSensorName` | polkit |
| Inventory | `RemoveFanName` | polkit |
| Lifecycle | `GetDraftConfig` | none |
| Lifecycle | `GetAppliedConfig` | none |
| Lifecycle | `GetDegradedSummary` | none |
| Lifecycle | `GetLifecycleEvents` | none |
| Lifecycle | `GetRuntimeState` | none |
| Lifecycle | `GetOverviewStructure` | none |
| Lifecycle | `GetOverviewTelemetry` | none |
| Lifecycle | `SetDraftFanEnrollment` | polkit |
| Lifecycle | `RemoveDraftFan` | polkit |
| Lifecycle | `DiscardDraft` | polkit |
| Lifecycle | `ValidateDraft` | none |
| Lifecycle | `ApplyDraft` | polkit |
| Lifecycle | `RequestAuthorization` | polkit |
| Control | `GetControlStatus` | none |
| Control | `GetAutoTuneResult` | none |
| Control | `StartAutoTune` | polkit |
| Control | `AcceptAutoTune` | polkit |
| Control | `SetDraftFanControlProfile` | polkit |

---

## 7. Signal Reference

| Interface | Signal | Signature | Emitted When |
|---|---|---|---|
| Lifecycle | `DraftChanged` | `()` | Draft configuration is modified by any write method (`SetDraftFanEnrollment`, `RemoveDraftFan`, `DiscardDraft`, `AcceptAutoTune`, `SetDraftFanControlProfile`) |
| Lifecycle | `AppliedConfigChanged` | `()` | One or more fans are promoted from draft to applied by `ApplyDraft` |
| Lifecycle | `DegradedStateChanged` | `()` | The set of degraded entries changes (fan goes missing, sensor disappears, recovery, etc.) |
| Lifecycle | `LifecycleEventAppended` | `(event_kind: s, detail: s)` | A new lifecycle event is recorded. `event_kind` is a `LifecycleEventKind` string (§5.7). `detail` is a human-readable summary. |
| Control | `ControlStatusChanged` | `()` | Live control status changes (e.g. after `ApplyDraft`, fan ownership transitions, alert state changes). Not emitted per-PID-tick. |
| Control | `AutoTuneCompleted` | `(fan_id: s)` | An auto-tune observation window completes for the identified fan (success or failure) |

---

## 8. Example Command-Line Calls

### 8.1 Read inventory snapshot

```sh
qdbus org.kde.FanControl /org/kde/FanControl org.kde.FanControl.Inventory.Snapshot
```

```sh
gdbus call --system --dest org.kde.FanControl \
  --object-path /org/kde/FanControl \
  --method org.kde.FanControl.Inventory.Snapshot
```

### 8.2 Enroll a fan in the draft

```sh
sudo qdbus org.kde.FanControl /org/kde/FanControl/Lifecycle \
  org.kde.FanControl.Lifecycle.SetDraftFanEnrollment \
  "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1" \
  true \
  "pwm" \
  "hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1"
```

```sh
sudo gdbus call --system --dest org.kde.FanControl \
  --object-path /org/kde/FanControl/Lifecycle \
  --method org.kde.FanControl.Lifecycle.SetDraftFanEnrollment \
  "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1" \
  true "pwm" \
  "['hwmon-nct6798-XXXXXXXXXXXXXXXX-temp1']"
```

### 8.3 Apply the draft

```sh
sudo qdbus org.kde.FanControl /org/kde/FanControl/Lifecycle \
  org.kde.FanControl.Lifecycle.ApplyDraft
```

```sh
sudo gdbus call --system --dest org.kde.FanControl \
  --object-path /org/kde/FanControl/Lifecycle \
  --method org.kde.FanControl.Lifecycle.ApplyDraft
```

### 8.4 Check runtime state

```sh
qdbus org.kde.FanControl /org/kde/FanControl/Lifecycle \
  org.kde.FanControl.Lifecycle.GetRuntimeState
```

```sh
gdbus call --system --dest org.kde.FanControl \
  --object-path /org/kde/FanControl/Lifecycle \
  --method org.kde.FanControl.Lifecycle.GetRuntimeState
```

### 8.5 Start and check auto-tune

```sh
# Start auto-tune (requires root)
sudo qdbus org.kde.FanControl /org/kde/FanControl/Control \
  org.kde.FanControl.Control.StartAutoTune \
  "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1"

# Check auto-tune result (read-open)
qdbus org.kde.FanControl /org/kde/FanControl/Control \
  org.kde.FanControl.Control.GetAutoTuneResult \
  "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1"
```

```sh
# Start
sudo gdbus call --system --dest org.kde.FanControl \
  --object-path /org/kde/FanControl/Control \
  --method org.kde.FanControl.Control.StartAutoTune \
  "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1"

# Check
gdbus call --system --dest org.kde.FanControl \
  --object-path /org/kde/FanControl/Control \
  --method org.kde.FanControl.Control.GetAutoTuneResult \
  "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1"
```

### 8.6 Set a control profile field

```sh
sudo qdbus org.kde.FanControl /org/kde/FanControl/Control \
  org.kde.FanControl.Control.SetDraftFanControlProfile \
  "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1" \
  '{"target_temp_millidegrees": 70000, "pid_gains": {"kp": 1.5, "ki": 0.8, "kd": 0.3}}'
```

```sh
sudo gdbus call --system --dest org.kde.FanControl \
  --object-path /org/kde/FanControl/Control \
  --method org.kde.FanControl.Control.SetDraftFanControlProfile \
  "hwmon-nct6798-XXXXXXXXXXXXXXXX-fan1" \
  '{"target_temp_millidegrees": 70000, "pid_gains": {"kp": 1.5, "ki": 0.8, "kd": 0.3}}'
```