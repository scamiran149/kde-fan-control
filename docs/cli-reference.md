# CLI Reference

Full reference for the `kde-fan-control` command-line interface.

## Global Behavior

- **Binary location**: `target/release/kde-fan-control` (release) or `target/debug/kde-fan-control` (debug)
- **Bus connection**: All commands that communicate with the daemon connect to the **system bus** first. If that fails, they fall back to the **session bus**.
- **Root requirement**: Write commands that change daemon state require root. If you get "Access denied", re-run with `sudo`.
- **Output formats**: Every command that accepts `--format` supports:
  - `text` — human-readable tables and labeled values (default)
  - `json` — machine-parseable JSON matching DBus response schemas
- **Exit codes**: `0` on success, non-zero on error
- **Access denied message**: `Access denied: lifecycle changes require root privileges. Run with sudo or as root.`
- **Parser**: Built with [clap](https://crates.io/crates/clap) 4.6.0 using subcommands

---

## Commands

### `inventory`

```
kde-fan-control inventory [--format text|json] [--root PATH] [--direct]
```

Lists all discovered hwmon devices, temperature sensors, and fan channels.

| Flag | Description |
|---|---|
| `--format text\|json` | Output format (default: `text`) |
| `--root PATH` | Override sysfs root directory (for testing with fake hardware trees) |
| `--direct` | Bypass the daemon; scan sysfs directly. Useful when the daemon isn't running. |

**Output includes:**

- Device name, sysfs path, stable identity
- Sensors: ID, label, current temperature
- Fans: ID, label, RPM, available control modes, support state

**Examples:**

```
$ kde-fan-control inventory
$ kde-fan-control inventory --direct
$ kde-fan-control inventory --format json
$ kde-fan-control inventory --direct --root /tmp/fake-hwmon
```

---

### `rename`

```
kde-fan-control rename ID NAME [--fan]
```

Assign a friendly name to a sensor or fan.

| Argument / Flag | Description |
|---|---|
| `ID` | Stable identifier of the sensor or fan |
| `NAME` | Friendly name to assign (quote it if it contains spaces) |
| `--fan` | Name a fan instead of a sensor (default: sensor) |

Requires root.

**Examples:**

```
$ sudo kde-fan-control rename hwmon-nct6798-xxx-temp1 "CPU Temp"
$ sudo kde-fan-control rename hwmon-nct6798-xxx-fan1 "CPU Fan" --fan
```

---

### `unname`

```
kde-fan-control unname ID [--fan]
```

Remove a friendly name from a sensor or fan.

| Argument / Flag | Description |
|---|---|
| `ID` | Stable identifier of the sensor or fan |
| `--fan` | Target a fan instead of a sensor (default: sensor) |

Requires root.

**Example:**

```
$ sudo kde-fan-control unname hwmon-nct6798-xxx-temp1
```

---

### `draft`

```
kde-fan-control draft [--format text|json]
```

Shows the current draft (staged) configuration. The output is clearly labeled as **DRAFT — NOT yet applied**.

Each fan entry includes: managed status, control mode, temp sources.

**Example:**

```
$ kde-fan-control draft
$ kde-fan-control draft --format json
```

---

### `applied`

```
kde-fan-control applied [--format text|json]
```

Shows the current applied (live) configuration. Includes the timestamp of the last `apply`.

Each managed fan entry includes: control mode, temp sources.

**Example:**

```
$ kde-fan-control applied
```

---

### `degraded`

```
kde-fan-control degraded [--format text|json]
```

Shows fans currently in degraded state with reasons.

**Degradation reasons:**

| Reason | Meaning |
|---|---|
| `fan_missing` | The fan device is no longer present on the system |
| `fan_no_longer_enrollable` | The fan no longer meets enrollment criteria |
| `control_mode_unavailable` | The requested control mode (e.g. PWM) is no longer available |
| `temp_source_missing` | One or more configured temperature sources are gone |
| `fallback_active` | The fan is running in fallback (high-speed safe) mode |

If all enrolled fans are healthy, the output is:

```
No degraded fans
```

**Example:**

```
$ kde-fan-control degraded
$ kde-fan-control degraded --format json
```

---

### `events`

```
kde-fan-control events [--format text|json]
```

Shows up to 64 recent lifecycle events with timestamps.

**Event types include:**

- Boot restores
- Degradation reasons
- Fallback incidents
- Partial recovery

**Example:**

```
$ kde-fan-control events
```

---

### `enroll`

```
kde-fan-control enroll FAN_ID [--managed] [--control-mode MODE] [--temp-sources ID1,ID2,...]
```

Stages a fan enrollment change in the draft. Changes are **not live** until `apply`.

| Argument / Flag | Description |
|---|---|
| `FAN_ID` | Stable identifier of the fan to enroll |
| `--managed` | Whether the daemon should manage this fan (default: `true`) |
| `--control-mode` | Control mode: `pwm`, `voltage`, or `none` (default: `none`) |
| `--temp-sources` | Comma-separated sensor IDs for this fan's control loop |

Returns the updated draft for confirmation. Requires root.

**Examples:**

```
$ sudo kde-fan-control enroll hwmon-nct6798-xxx-fan1
$ sudo kde-fan-control enroll hwmon-nct6798-xxx-fan1 --control-mode pwm --temp-sources hwmon-nct6798-xxx-temp1,hwmon-nct6798-xxx-temp2
$ sudo kde-fan-control enroll hwmon-nct6798-xxx-fan2 --managed false
```

---

### `unenroll`

```
kde-fan-control unenroll FAN_ID
```

Removes a fan from the draft configuration. Not live until `apply`.

| Argument | Description |
|---|---|
| `FAN_ID` | Stable identifier of the fan to remove |

Requires root.

**Example:**

```
$ sudo kde-fan-control unenroll hwmon-nct6798-xxx-fan2
```

---

### `discard`

```
kde-fan-control discard
```

Discards the entire draft configuration. No effect on the live/applied config.

Requires root.

**Example:**

```
$ sudo kde-fan-control discard
```

---

### `validate`

```
kde-fan-control validate
```

Validates the current draft against live inventory without making any changes. Reports which fans would be promoted (enrollable) and which would be rejected, with reasons.

- Does **not** modify any state
- Does **not** require root (read operation)

**Example:**

```
$ kde-fan-control validate
```

---

### `apply`

```
kde-fan-control apply
```

Validates and promotes the draft to live configuration. This is the **commit point** — changes are persisted.

- Passing fans become managed immediately; control tasks start.
- Rejected fans remain in the draft with reasons.
- After apply, affected fans are claimed by the daemon and control loops begin.

Requires root.

**Example:**

```
$ sudo kde-fan-control apply
```

---

### `state`

```
kde-fan-control state [--format text|json] [--detail]
```

Shows runtime state of all fans: `managed`, `degraded`, `fallback`, or `unmanaged`.

**Managed fans show:** current temperature, target temperature, output %, PWM value, auto-tune status.

| Flag | Description |
|---|---|
| `--format text\|json` | Output format (default: `text`) |
| `--detail` | Also show PID gains, cadence settings, sensor IDs, and aggregation function |

The command merges data from multiple DBus methods (runtime state + control status + auto-tune results).

Does **not** require root (read operation).

**Examples:**

```
$ kde-fan-control state
$ kde-fan-control state --detail
$ kde-fan-control state --format json
```

---

### `control set`

```
kde-fan-control control set FAN_ID --target-temp TEMP --aggregation MODE --kp KP --ki KI --kd KD --sample-ms MS --control-ms MS --write-ms MS [--deadband-mc MC]
```

Stages PID control profile changes for a managed fan. All values go into the **draft** — not live until `apply`.

| Argument / Flag | Description |
|---|---|
| `FAN_ID` | Stable identifier of the managed fan |
| `--target-temp` | Target temperature in Celsius (auto-converted to millidegrees internally) |
| `--aggregation` | Temperature aggregation mode: `average`, `max`, `min`, `median` |
| `--kp` | Proportional gain |
| `--ki` | Integral gain |
| `--kd` | Derivative gain |
| `--sample-ms` | Sensor sampling interval (ms). Must be >= 250. |
| `--control-ms` | PID calculation interval (ms). Must be >= `--sample-ms`. |
| `--write-ms` | Fan write interval (ms). Must be >= `--control-ms`. |
| `--deadband-mc` | Deadband in millidegrees (optional) |

**Cadence constraint:** `sample-ms` <= `control-ms` <= `write-ms`, and each must be >= 250 ms.

Requires root.

**Example:**

```
$ sudo kde-fan-control control set hwmon-nct6798-xxx-fan1 \
    --target-temp 65 \
    --aggregation average \
    --kp 1.0 --ki 0.5 --kd 0.3 \
    --sample-ms 500 --control-ms 1000 --write-ms 1000
```

---

### `auto-tune start`

```
kde-fan-control auto-tune start FAN_ID
```

Starts a bounded auto-tune observation for a managed fan. The fan runs at 100% during the observation window (default 30 seconds). The observation is time-bounded and reviewable — nothing is applied automatically.

Requires root.

**Example:**

```
$ sudo kde-fan-control auto-tune start hwmon-nct6798-xxx-fan1
```

---

### `auto-tune result`

```
kde-fan-control auto-tune result FAN_ID
```

Shows the latest auto-tune result for a fan.

**States:**

| State | Meaning |
|---|---|
| `idle` | No auto-tune has been run |
| `running` | Auto-tune observation is in progress |
| `completed` | Results available; proposed PID gains shown (softened for safety) |
| `failed` | Auto-tune failed; reason given |

Does **not** require root (read operation).

**Example:**

```
$ kde-fan-control auto-tune result hwmon-nct6798-xxx-fan1
```

---

### `auto-tune accept`

```
kde-fan-control auto-tune accept FAN_ID
```

Accepts the latest completed auto-tune proposal. The proposed PID gains are written into the draft entry for the fan. You must still run `apply` to make the gains live.

Requires root.

**Example:**

```
$ sudo kde-fan-control auto-tune accept hwmon-nct6798-xxx-fan1
$ sudo kde-fan-control apply
```

---

## Authorization Summary

| Command | Requires root |
|---|---|
| `inventory` | No |
| `rename` | Yes |
| `unname` | Yes |
| `draft` | No |
| `applied` | No |
| `degraded` | No |
| `events` | No |
| `enroll` | Yes |
| `unenroll` | Yes |
| `discard` | Yes |
| `validate` | No |
| `apply` | Yes |
| `state` | No |
| `control set` | Yes |
| `auto-tune start` | Yes |
| `auto-tune result` | No |
| `auto-tune accept` | Yes |

---

## Output Format Notes

- `--format text` (default): human-readable tables and labeled values
- `--format json`: valid JSON matching the DBus response schemas — suitable for scripting and piping to `jq`
- All commands return exit code `0` on success, non-zero on error
- `AccessDenied` errors produce: `Access denied: lifecycle changes require root privileges. Run with sudo or as root.`

---

## Typical Workflow

```
# 1. See what hardware is available
$ kde-fan-control inventory

# 2. Give things friendly names (optional)
$ sudo kde-fan-control rename hwmon-nct6798-xxx-temp1 "CPU Temp"
$ sudo kde-fan-control rename hwmon-nct6798-xxx-fan1 "CPU Fan" --fan

# 3. Enroll a fan with PID control
$ sudo kde-fan-control enroll hwmon-nct6798-xxx-fan1 \
    --control-mode pwm \
    --temp-sources hwmon-nct6798-xxx-temp1

# 4. Tune the PID profile
$ sudo kde-fan-control control set hwmon-nct6798-xxx-fan1 \
    --target-temp 65 --aggregation max \
    --kp 1.0 --ki 0.5 --kd 0.3 \
    --sample-ms 500 --control-ms 1000 --write-ms 1000

# 5. (Or let auto-tune find gains for you)
$ sudo kde-fan-control auto-tune start hwmon-nct6798-xxx-fan1
$ kde-fan-control auto-tune result hwmon-nct6798-xxx-fan1
$ sudo kde-fan-control auto-tune accept hwmon-nct6798-xxx-fan1

# 6. Check the draft before committing
$ kde-fan-control draft
$ kde-fan-control validate

# 7. Commit
$ sudo kde-fan-control apply

# 8. Monitor
$ kde-fan-control state
$ kde-fan-control state --detail

# 9. Check health
$ kde-fan-control degraded
$ kde-fan-control events

# 10. Undo if needed
$ sudo kde-fan-control unenroll hwmon-nct6798-xxx-fan1
$ sudo kde-fan-control apply
```