# Phase 3: Temperature Control & Runtime Operations - Context

**Gathered:** 2026-04-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 3 adds per-fan temperature-driven PID control, multi-sensor aggregation, runtime status inspection, basic auto-tuning, and the DBus + CLI surfaces for all of the above. This phase builds on Phase 2's enrollment, lifecycle, and fallback foundations to actually drive fans based on real-time temperature input.

</domain>

<decisions>
## Implementation Decisions

### Control Loop Architecture
- **D-01:** Each managed fan gets an independent PID control loop that reads sensor input, computes output, and writes PWM values on a fixed tick interval.
- **D-02:** v1 PID control is temperature-target-based: the setpoint is a target temperature, and the PID loop drives fan speed to reach and maintain that target. This is NOT RPM-target tracking.
- **D-03:** Sensor input aggregation (average, max, min, median) should be computed per-tick before feeding the PID error term, not pre-aggregated at config time.
- **D-04:** The PID tick interval should be configurable per fan in the draft, with a sensible default (2 seconds) that balances responsiveness against sysfs write frequency.
- **D-05:** PID output is clamped to safe PWM bounds per the fan's hardware mode. PWM range is 0–255 for PWM mode. Voltage mode uses the same 0–255 range (hardware interprets the duty cycle).

### Auto-Tuning
- **D-06:** Basic auto-tuning should use the Ziegler-Nichols step-response method: temporarily run the fan at maximum, observe the temperature response, and compute starting P/I/D gains from the observed delay and rate.
- **D-07:** Auto-tuning is an explicit user-triggered action, not a continuous background process. The user starts it, it runs for a defined period, and then the tuned values are presented for review and acceptance.
- **D-08:** During auto-tuning, the fan is driven to a known state and the results replace the current PID parameters only when the user accepts them via draft/apply.

### Safety And Validation
- **D-09:** Configuration validation must reject any managed fan that lacks a usable temperature input (no sensor assigned, or all assigned sensors are missing from current hardware) or that lacks a target temperature.
- **D-10:** If all temperature sensors for a managed fan disappear at runtime (hot-unplug), that fan must transition to a degraded state rather than continue with stale readings.
- **D-11:** The control loop must apply output only to owned fans — never to unmanaged or unavailable fans.

### DBus And CLI Surfaces
- **D-12:** Runtime status (live temperatures, fan RPM, PID output values, control state) should be readable over DBus by unprivileged callers, consistent with Phase 2's read-open policy.
- **D-13:** PID parameter changes and auto-tuning triggers should require privileged authorization, consistent with Phase 2's write-privileged policy.
- **D-14:** The v1 CLI must let users inspect runtime status and trigger auto-tuning, building on the existing thin-DBus-client pattern.

### Already Locked From Project Context
- **D-15:** v1 uses temperature-target PID, not RPM tracking, not adaptive or fuzzy control.
- **D-16:** DBus is the authority surface — CLI and future GUI are clients.
- **D-17:** Managed fans must fail to high speed on daemon failure (established in Phase 2).
- **D-18:** Writable sysfs control is root-only (established in Phase 1 and 2).

### the agent's Discretion
- Exact PID tick interval default and min/max bounds.
- Exact Ziegler-Nichols step-response parameters and tuning duration.
- Internal module layout for the PID loop, sensor reader, and control tasks within the daemon.
- Exact DBus method and signal naming, as long as it follows the established pattern.
- Exact TOML schema extensions for PID parameters — any reasonable structure is fine.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product And Scope
- `.planning/PROJECT.md` — Product constraints, daemon-owned persistence, safety boundaries.
- `.planning/REQUIREMENTS.md` — Phase 3 requirements (SNS-01 through SNS-06, PID-01 through PID-07, SAFE-04, BUS-03, BUS-05, CLI-03, CLI-04).
- `.planning/ROADMAP.md` — Phase 3 goal and success criteria.

### Existing Implementation (MUST READ)
- `crates/core/src/config.rs` — AppConfig, DraftConfig, AppliedFanEntry, validation, lifecycle types, DegradedReason. All PID and sensor config extensions MUST build on these.
- `crates/core/src/inventory.rs` — InventorySnapshot, TemperatureSensor, FanChannel, ControlMode, SupportState. The PID loop reads from this.
- `crates/core/src/lifecycle.rs` — OwnedFanSet, RuntimeState, FanRuntimeStatus, boot reconciliation, fallback. The control loop integrates with ownership and fallback.
- `crates/daemon/src/main.rs` — Daemon main, shared state (Arc<RwLock<...>>), DBus interfaces (InventoryIface, LifecycleIface), signal emission, boot reconciliation, and shutdown. The PID task loop runs within this daemon context.
- `crates/cli/src/main.rs` — CLI thin DBus client pattern. Runtime status and auto-tune commands extend this.

</canonical_refs>

<code_context>
## Existing Code Insights

### Key Types For Extension
- `AppliedFanEntry` in config.rs currently has `control_mode` and `temp_sources`. Needs extension to include `target_temp`, `pid_gains`, `aggregation`, and `tick_interval`.
- `DraftFanEntry` in config.rs currently has `managed`, `control_mode`, `temp_sources`. Needs matching extension for PID parameters and aggregation choice.
- `ValidationError` already covers fan existence, enrollability, control mode, and temp source existence. Needs extension for target temp validation and missing temperature input.
- `FanRuntimeStatus` in lifecycle.rs already distinguishes Unmanaged, Managed, Degraded, Fallback. Needs extension to carry PID runtime data (output value, sensor readings, control state).
- `OwnedFanSet` in lifecycle.rs tracks per-fan sysfs paths. The control loop writes to these paths on each tick.

### Integration Points
- The PID control loop task runs as a Tokio spawned task, reading from shared state (Arc<RwLock<...>>), computing output, and writing to sysfs.
- Sensor readings come from `/sys/class/hwmon/hwmon*/tempN_input` — these are already discovered and represented as `TemperatureSensor.input_millidegrees_celsius` in inventory.
- Fan control writes go to `/sys/class/hwmon/hwmon*/pwmN` — already represented as `OwnedFanSet.fan_sysfs_paths`.
- The DBus LifecycleIface already has `get_runtime_state()`. The new ControlIface needs its own DBus path and interface.

### Established Patterns
- DBus interfaces use `#[interface(name = "...")]` with JSON-serialized return values.
- Write methods use `require_authorized()` for the UID-0 check.
- Signal emission uses `SignalEmitter<'_>` parameters and propagates errors as warnings, not failures.
- Shared daemon state uses `Arc<RwLock<...>>` for thread-safe mutable access.
- The daemon uses `tokio::signal::ctrl_c().await` for shutdown and drives fallback before exit.

</code_context>

<specifics>
## Specific Ideas

- The user wants to see live temperature values and fan-control output change in near real-time through the status surface.
- The user wants PID parameters to be adjustable per fan through the draft/apply flow, with the option of live tuning (though the draft/apply path is the primary mechanism).
- The user wants auto-tuning to be a clear, time-bounded operation that the user starts and then reviews the results of — not an ever-present background process.
- The user expects the CLI `state` command to show live temperatures and PID output alongside the existing lifecycle status.

</specifics>

<deferred>
## Deferred Ideas

- RPM-target tracking control mode (v1 is temperature-target PID only).
- Adaptive or fuzzy PID tuning strategies (v1 uses basic Ziegler-Nichols step response).
- Historical temperature and fan graphs (observability feature for a future phase).
- Live setpoint adjustment without draft/apply (the infrastructure may appear in Phase 3, but the user-facing workflow is draft/apply).
- KDE GUI controls for PID parameters (belongs to Phase 4).

</deferred>

---

*Phase: 03-temperature-control-runtime-operations*
*Context gathered: 2026-04-11*