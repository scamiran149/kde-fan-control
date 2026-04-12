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
- **D-01:** Each managed fan gets an independent PID control path that samples sensor input, computes output, and performs actuator writes using separable cadences rather than one fixed loop clock.
- **D-02:** v1 PID control is temperature-target-based: the setpoint is a target temperature, and the PID loop drives fan speed to reach and maintain that target. This is NOT RPM-target tracking.
- **D-03:** Sensor input aggregation (average, max, min, median) should be computed per-tick before feeding the PID error term, not pre-aggregated at config time.
- **D-04:** Control timing should use separable cadences rather than one fixed loop clock: sensor sampling cadence, control computation cadence, and actuator write cadence are distinct configuration concerns.
- **D-05:** v1 uses a linear PID core with a deadband or quiet band rather than nonlinear squared-error control.
- **D-06:** Derivative is computed on measurement, not on error.
- **D-07:** PID behavior must clamp integral accumulation, derivative contribution, and final output.
- **D-08:** Controller output should be represented internally as a logical `0-100%` value and then mapped to hardware-specific ranges. Default PWM mapping is `0..255`, but scaling should remain configurable.
- **D-09:** Startup kick for stopped fans is a separate actuator-policy layer, not part of the PID equation.

### Auto-Tuning
- **D-10:** Basic auto-tuning should use a step-response approach, but the proposed gains should be softened for desktop thermal stability and acoustics rather than exposing raw aggressive tuning results directly.
- **D-11:** Auto-tuning is an explicit user-triggered action, not a continuous background process. The user starts it, it runs for a defined period, and then the tuned values are presented for review and acceptance.
- **D-12:** During auto-tuning, the fan is driven to a known state and the results replace the current PID parameters only when the user accepts them via draft/apply.
- **D-13:** Auto-tuning may be aggressive in signal gathering, but the only immediate hard abort condition locked for v1 is sensor loss or unreadable temperature input.

### Safety And Validation
- **D-14:** Configuration validation must reject any managed fan that lacks a usable temperature input (no sensor assigned, or all assigned sensors are missing from current hardware) or that lacks a target temperature.
- **D-15:** If all temperature sensors for a managed fan disappear at runtime (hot-unplug), that fan must transition to a degraded state rather than continue with stale readings.
- **D-16:** The control loop must apply output only to owned fans — never to unmanaged or unavailable fans.
- **D-17:** High-temperature handling in v1 is alarm or alert only, not an automatic emergency control-mode change layered on top of the normal controller.

### DBus And CLI Surfaces
- **D-18:** Runtime status (live temperatures, fan RPM, PID output values, control state) should be readable over DBus by unprivileged callers, consistent with Phase 2's read-open policy.
- **D-19:** PID parameter changes and auto-tuning triggers should require privileged authorization, consistent with Phase 2's write-privileged policy.
- **D-20:** The v1 CLI should default to a simple status view with optional deeper detail rather than exposing full PID internals by default.
- **D-21:** The v1 CLI must let users inspect runtime status and trigger auto-tuning, building on the existing thin-DBus-client pattern.

### Already Locked From Project Context
- **D-22:** v1 uses temperature-target PID, not RPM tracking, not adaptive or fuzzy control.
- **D-23:** DBus is the authority surface — CLI and future GUI are clients.
- **D-24:** Managed fans must fail to high speed on daemon failure (established in Phase 2).
- **D-25:** Writable sysfs control is root-only (established in Phase 1 and 2).

### the agent's Discretion
- Exact default values and bounds for the three separate cadences, as long as they remain independently configurable.
- Exact deadband implementation details and default width.
- Exact derivative and integral clamp formulas and bounds.
- Exact softened step-response tuning formulas and observation window.
- Internal module layout for the PID loop, sensor reader, control tasks, and actuator policy within the daemon.
- Exact DBus method and signal naming, as long as it follows the established pattern.
- Exact TOML schema extensions for PID parameters, deadband, cadence controls, and actuator-policy settings.

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
- `AppliedFanEntry` in config.rs currently has `control_mode` and `temp_sources`. Needs extension to include `target_temp`, `pid_gains`, `aggregation`, cadence controls, deadband, and actuator-policy settings.
- `DraftFanEntry` in config.rs currently has `managed`, `control_mode`, `temp_sources`. Needs matching extension for PID parameters, aggregation, cadence controls, deadband, and actuator policy.
- `ValidationError` already covers fan existence, enrollability, control mode, and temp source existence. Needs extension for target temp validation and missing temperature input.
- `FanRuntimeStatus` in lifecycle.rs already distinguishes Unmanaged, Managed, Degraded, Fallback. Needs extension to carry PID runtime data (output value, sensor readings, control state).
- `OwnedFanSet` in lifecycle.rs tracks per-fan sysfs paths. The control loop writes to these paths on each tick.
- The current Phase 3 plans assume a single `tick_interval_ms`; those plans need replanning to reflect the newly locked separable cadence model and logical `0-100%` output model.

### Integration Points
- The PID control loop task runs as a Tokio spawned task, reading from shared state (Arc<RwLock<...>>), computing output, and writing to sysfs.
- Sensor readings come from `/sys/class/hwmon/hwmon*/tempN_input` — these are already discovered and represented as `TemperatureSensor.input_millidegrees_celsius` in inventory.
- Fan control writes go to `/sys/class/hwmon/hwmon*/pwmN` — already represented as `OwnedFanSet.fan_sysfs_paths`.
- The DBus LifecycleIface already has `get_runtime_state()`. The new ControlIface needs its own DBus path and interface.
- Actuator policy needs its own integration point for startup kick, output scaling, and write-rate limiting instead of burying these behaviors inside the PID math.

### Established Patterns
- DBus interfaces use `#[interface(name = "...")]` with JSON-serialized return values.
- Write methods use `require_authorized()` for the UID-0 check.
- Signal emission uses `SignalEmitter<'_>` parameters and propagates errors as warnings, not failures.
- Shared daemon state uses `Arc<RwLock<...>>` for thread-safe mutable access.
- The daemon uses `tokio::signal::ctrl_c().await` for shutdown and drives fallback before exit.
- Phase 3 should preserve the “daemon-authoritative, CLI-thin-client” pattern rather than moving control logic into the CLI.

</code_context>

<specifics>
## Specific Ideas

- The user wants to see live temperature values and fan-control output change in near real-time through the status surface.
- The user wants PID parameters to be adjustable per fan through the draft/apply flow, with the option of live tuning (though the draft/apply path is the primary mechanism).
- The user wants auto-tuning to be a clear, time-bounded operation that the user starts and then reviews the results of — not an ever-present background process.
- The user expects the CLI `state` command to show live temperatures and PID output alongside the existing lifecycle status.
- The user has experience with large control systems and wants the control loop design treated as a first-class design problem rather than an implementation detail.
- The user is comfortable with alarm-oriented high-temperature handling rather than an overly conservative control policy.
- The user wants runtime status to stay simple by default, with more detailed PID internals available as an optional deeper view.
- The user wants a future path to feed-forward control using predictive signals like CPU or GPU utilization, but that is explicitly deferred beyond v1.

</specifics>

<deferred>
## Deferred Ideas

- RPM-target tracking control mode (v1 is temperature-target PID only).
- Adaptive or fuzzy PID tuning strategies (v1 uses basic Ziegler-Nichols step response).
- Historical temperature and fan graphs (observability feature for a future phase).
- Live setpoint adjustment without draft/apply (the infrastructure may appear in Phase 3, but the user-facing workflow is draft/apply).
- KDE GUI controls for PID parameters (belongs to Phase 4).
- Feed-forward control using signals such as CPU or GPU utilization. This is promising and should be considered as a future phase after the base PID loop is proven.

</deferred>

---

*Phase: 03-temperature-control-runtime-operations*
*Context gathered: 2026-04-11*
