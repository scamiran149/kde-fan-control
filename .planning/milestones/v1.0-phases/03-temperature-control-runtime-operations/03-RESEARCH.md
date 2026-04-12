# Phase 3 Research: Temperature Control & Runtime Operations

**Phase:** 03-temperature-control-runtime-operations
**Date:** 2026-04-11
**Status:** Complete

## Objective

Research how to implement per-fan PID control loops, sensor aggregation, runtime status surfaces, basic auto-tuning, and configuration validation for temperature-driven fan management, building on the Phase 2 enrollment and lifecycle foundations.

## Locked Inputs

- v1 PID is temperature-target-based (not RPM tracking, not adaptive)
- Sensor aggregation computes per-tick from current readings
- Auto-tuning is user-triggered, time-bounded, with reviewed results
- Config validation rejects fans without usable temperature input or target temperature
- Runtime status is read-open, control parameters are write-privileged

## Research Findings

### 1. PID Control Loop Architecture

- A classic PID controller computes: `output = Kp * error + Ki * integral + Kd * derivative`
- For temperature-target fans: `error = target_temp - current_temp`
- When error is positive (temp above target), increase fan speed; when negative, decrease.
- The integral term accumulates error over time (prevents steady-state offset).
- The derivative term responds to rate of temperature change (prevents overshoot).
- Output must be clamped to [0, 255] for PWM duty cycle.
- Anti-windup: the integral accumulator must be clamped or reset when output saturates to prevent integral windup.
- **Planning implication:** The PID loop is stateful (integral accumulator, previous error for derivative). Each managed fan needs its own PID state.

### 2. Tick Interval And Timing

- Linux hwmon sysfs paths can be read at ~10 Hz without issues, but fan thermal response time is in the 5–30 second range.
- A 2-second default tick interval balances responsive fan speed changes against unnecessary sysfs writes.
- Shorter intervals (0.5s–1s) increase write frequency with minimal thermal benefit for most fans.
- Longer intervals (5s–10s) reduce writes but increase response latency for temperature spikes.
- **Planning implication:** Default 2-second tick, configurable per fan. Minimum 0.5s. Tokio `tokio::time::interval()` is the natural fit for periodic ticks.

### 3. Sensor Aggregation

- Four aggregation functions: average, max, min, median.
- For `average`: sum all readings, divide by count.
- For `max`: take the highest reading.
- For `min`: take the lowest reading.
- For `median`: sort readings, take the middle value (or average of the two middle values for even count).
- Single-sensor mode is just "the one reading" — no aggregation needed.
- If any sensor in a group returns `None` (disappeared), the aggregation function must handle the reduced count. If ALL sensors are `None`, the fan must transition to degraded.
- **Planning implication:** Aggregation function is a per-fan config field stored in `DraftFanEntry` and `AppliedFanEntry`.

### 4. PID Gains And Target Temperature

- Typical thermal PID starting values for desktop fans: Kp=1.0, Ki=0.1, Kd=0.5 (relative, not absolute — these need tuning per-system).
- Target temperature is in degrees Celsius, stored as millidegrees internally for hwmon consistency, displayed as degrees in the CLI.
- PID gains are stored as floating point.
- The output maps: 0.0 = fan off (0% PWM), 255.0 = fan at maximum (100% PWM).
- **Planning implication:** DraftFanEntry needs `target_temp: Option<f64>`, `pid_kp: Option<f64>`, `pid_ki: Option<f64>`, `pid_kd: Option<f64>`, `aggregation: Option<AggregationFn>`, `tick_interval_ms: Option<u64>`.

### 5. Ziegler-Nichols Step-Response Auto-Tuning

- Ziegler-Nichols step response: drive the fan to max output, observe delay before temperature starts changing, and the rate of change. Compute Kp, Ki, Kd from lag time (L) and max rate (R).
- Classic ZN formulas for PID: Kp = 1.2 / (R * L), Ki = Kp / (2 * L), Kd = Kp * L / 0.5
- For fan thermal control, the "step" is setting fan to full speed while monitoring temperature input.
- Duration: 30–60 seconds is typically enough to observe the thermal response curve.
- The fan must already be managed (enrolled and under control) before auto-tuning can start.
- **Planning implication:** Auto-tuning is a per-fan async operation that runs within the control task. It temporarily overrides the normal PID loop for that fan, runs the step response, computes gains, and stores the results for user review.

### 6. Runtime Status Surface

- The `RuntimeState` in lifecycle.rs already tracks per-fan status (Unmanaged, Managed, Degraded, Fallback).
- For PID control, each managed fan needs additional runtime data: current temperature reading(s), aggregated temperature, PID output value, current error, integral accumulator state, and the control mode.
- This data should be accessible through DBus for the CLI and future GUI.
- **Planning implication:** A new DBus interface `org.kde.FanControl.Control` at path `/org/kde/FanControl/Control` exposes runtime control status and auto-tuning triggers.

### 7. Configuration Validation Extensions

- Phase 2 validation already checks: fan existence, fan enrollability, control mode support, and temp source existence.
- Phase 3 adds: managed fans must have `target_temp` set, managed fans must have at least one `temp_source` assigned (already checked, but now it's safety-critical — a fan with no sensor input must be rejected).
- The `apply_draft` function already returns `ValidationResult` — extend with new error variants for missing target temp and missing sensor source for managed fans.
- **Planning implication:** New `ValidationError` variants: `MissingTargetTemp` and `NoSensorForManagedFan`.

### 8. Sysfs Temperature Reading

- Temperature values are read from `/sys/class/hwmon/hwmon*/tempN_input` as millidegrees celsius (integer strings).
- The `TemperatureSensor` in inventory.rs already has `input_millidegrees_celsius: Option<i64>` populated at discovery time.
- For the PID loop, readings must be refreshed continuously — can't rely on discovery-time snapshot.
- Read path: construct the full path from device `sysfs_path` and sensor `channel`, then `fs::read_to_string()` and parse as millidegrees.
- **Planning implication:** The control loop task needs a way to resolve sensor IDs to sysfs paths for live reading, not just rely on the discovery snapshot.

### 9. Tokio Task Architecture

- The main daemon already runs a `#[tokio::main]` async runtime.
- Each managed fan's PID loop should be a `tokio::spawn`'d task that reads from shared state, computes output, writes to sysfs, and updates runtime status.
- A supervisor task manages lifecycle: spawns PID tasks for fans that become managed, cancels tasks for fans that become unmanaged or degraded.
- The supervisor listens for config changes (applied config update) and adjusts the running tasks.
- **Planning implication:** Add a new module `crates/core/src/control.rs` for PID logic and aggregation, and integrate the control supervisor into `crates/daemon/src/main.rs`.

## Recommended Architecture

### New Core Module: `crates/core/src/control.rs`

- `PidController` struct: holds Kp, Ki, Kd, target, integral accumulator, previous error.
- `AggregationFn` enum: Average, Max, Min, Median — implements `compute(readings: &[f64]) -> f64`.
- `PidOutput` struct: output value, error, integral, derivative — for runtime status.
- `AutoTuneState` enum: Idle, Running(computed gains), Completed(proposed gains).

### Config Extensions in `crates/core/src/config.rs`

- Extend `DraftFanEntry` and `AppliedFanEntry` with PID parameters.
- Add `AggregationFn` serde enum.
- Add new validation errors: `MissingTargetTemp`, `NoSensorForManagedFan`.
- Config TOML additions are additive and backward-compatible.

### Daemon Control Task in `crates/daemon/src/main.rs`

- Control supervisor task: `tokio::spawn` that manages per-fan PID tasks.
- Per-fan PID task: periodic tick using `tokio::time::interval`, reads sensors, computes PID, writes PWM.
- On shutdown: existing fallback logic handles safe-maximum.

### New DBus Interface: `org.kde.FanControl.Control`

- Read methods: `get_control_status()` — returns per-fan runtime control data.
- Write methods: `start_auto_tune(fan_id)` — starts auto-tune for a fan.
- Write methods: `set_target_temp(fan_id, target)` — live setpoint change (future: may bypass draft/apply for tuning feel).
- Signals: `ControlStatusChanged()`, `AutoTuneCompleted(fan_id, proposed_kp, proposed_ki, proposed_kd)`.

### CLI Extensions

- `kde-fan-control state` — extend to show live temperatures and PID output for managed fans.
- `kde-fan-control auto-tune <fan_id>` — start auto-tuning.
- `kde-fan-control enroll` — extend to accept `--target-temp`, `--kp`, `--ki`, `--kd`, `--aggregation`, `--tick-interval`.

## Risks To Address In Plans

1. Integral windup leading to runaway fan speeds — need anti-windup clamping.
2. Sensor disappearance at runtime (hot-unplug) — fan must degrade, not continue with stale data.
3. Auto-tuning disrupting fan behavior for uninvolved fans — only the target fan changes.
4. Concurrent sysfs writes — the control loop and fallback logic must not conflict.
5. Config-change race conditions — applying new PID parameters while the control loop is running needs careful synchronization.

## Concrete Recommendations For The Planner

- Split the control module (PID logic + aggregation) from the DBus + daemon integration — core domain first, then wiring.
- Use Tokio `watch` channels for config changes to the control supervisor, and `RwLock` for runtime status reads from DBus.
- Auto-tuning runs within the same per-fan task that normally does PID control — when auto-tune is requested, the task switches mode.
- Sensor readings in the PID loop use sysfs reads (not the snapshot) for live data; construct the path from device sysfs_path and channel number.
- Extend the validation logic in core rather than duplicating it in the daemon.

## Suggested Plan Split

1. **Control domain model** — PID controller, aggregation functions, config extensions, validation extensions, auto-tune types.
2. **DBus Control interface + daemon integration** — new ControlIface, runtime status data, control supervisor task, per-fan PID tasks.
3. **CLI runtime status + auto-tune commands** — extend CLI with control status display and auto-tune trigger.

## Sources

- `.planning/PROJECT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/phases/03-temperature-control-runtime-operations/03-CONTEXT.md`
- `crates/core/src/config.rs` — current config model for extension
- `crates/core/src/inventory.rs` — sensor and fan data structures
- `crates/core/src/lifecycle.rs` — ownership and runtime state model
- `crates/daemon/src/main.rs` — daemon architecture and DBus integration
- `crates/cli/src/main.rs` — CLI patterns
- Context7 `/dbus2/zbus` — properties, signals, async service patterns
- Context7 `/tokio-rs/tokio` — interval, spawn, watch channels
- Linux kernel hwmon sysfs ABI: https://www.kernel.org/doc/html/latest/hwmon/sysfs-interface.html
- Ziegler-Nichols method: standard PID tuning reference

---

*Phase: 03-temperature-control-runtime-operations*
*Research completed: 2026-04-11*