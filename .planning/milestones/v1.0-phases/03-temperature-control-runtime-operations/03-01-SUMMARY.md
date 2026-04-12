---
phase: 03-temperature-control-runtime-operations
plan: 01
subsystem: api
tags: [rust, pid, hwmon, config, runtime-status]
requires:
  - phase: 02-safe-enrollment-lifecycle-recovery
    provides: enrollment validation, owned-fan lifecycle state, fallback recovery contracts
provides:
  - temperature-target PID contracts and bounded output helpers in core
  - managed fan control-profile schema with validation and defaults
  - runtime control snapshot payloads for managed fan status reporting
affects: [daemon, cli, dbus, runtime-control]
tech-stack:
  added: []
  patterns: [temperature-target PID in logical percent space, applied-config-backed runtime control snapshots]
key-files:
  created: [crates/core/src/control.rs]
  modified: [crates/core/src/config.rs, crates/core/src/lib.rs, crates/core/src/lifecycle.rs]
key-decisions:
  - "Represent PID output as logical 0-100% and map to hardware ranges through actuator policy helpers."
  - "Build managed runtime control snapshots from applied config so DBus clients can consume one authoritative contract."
patterns-established:
  - "Managed fan configs resolve optional draft fields into explicit applied control profiles before runtime use."
  - "PID safety clamps live in core contracts alongside actuator mapping helpers and runtime snapshot types."
requirements-completed: [SNS-01, SNS-02, SNS-03, SNS-04, SNS-05, SNS-06, PID-01, PID-02, PID-07, SAFE-04]
duration: 4 min
completed: 2026-04-11
---

# Phase 3 Plan 1: Control Domain Contracts Summary

**Temperature-target PID contracts with validated per-fan control profiles and managed-fan runtime control snapshots in core.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-04-11T13:31:07-05:00
- **Completed:** 2026-04-11T18:34:56Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Added `crates/core/src/control.rs` with aggregation, PID gain/cadence/limit types, controller behavior, output mapping, startup-kick detection, and softened auto-tune math.
- Extended draft/applied fan config entries so managed fans carry full Phase 3 control settings with validation for required targets, sensors, cadence ordering, and actuator bounds.
- Expanded lifecycle runtime status so managed fans can report control snapshots through one serializable DBus-facing contract.

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend the core control profile schema and validation** - `af98c6c` (feat)
2. **Task 2: Rework PID behavior and runtime status contracts around temperature-target control** - `101273d` (feat)

**Plan metadata:** Pending orchestrator-owned state/docs commit.

## Files Created/Modified
- `crates/core/src/control.rs` - Core aggregation, PID controller, output mapping, startup-kick, and auto-tune proposal helpers with tests.
- `crates/core/src/config.rs` - Draft/applied control-profile schema, default resolution, and managed-fan safety validation.
- `crates/core/src/lib.rs` - Exports the new control module.
- `crates/core/src/lifecycle.rs` - Managed runtime status payloads now carry control snapshots derived from applied config.

## Decisions Made
- Represented controller output as logical percent in core and deferred hardware-specific scaling to `ActuatorPolicy` helpers, matching the phase's logical-output requirement.
- Made `ControlRuntimeSnapshot` derive from applied config defaults so runtime status remains serializable and authoritative before the daemon loop starts publishing live samples.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated lifecycle applied-entry copies for the expanded control profile**
- **Found during:** Task 1 (Extend the core control profile schema and validation)
- **Issue:** Extending `AppliedFanEntry` broke lifecycle reconciliation and tests because older initializers no longer populated the required control fields.
- **Fix:** Propagated the new control-profile fields through lifecycle reconciliation helpers and test fixtures.
- **Files modified:** `crates/core/src/lifecycle.rs`
- **Verification:** `cargo test -p kde-fan-control-core config -- --nocapture`
- **Committed in:** `af98c6c`

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Required to keep the expanded core control profile compiling through existing lifecycle consumers. No scope creep.

## Issues Encountered
- A control auto-tune expectation in the new unit test used the wrong softened `ki`/`kd` values; corrected the test to match the implemented step-response formula and re-ran the full core test set.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Core config, controller, and runtime payload contracts are ready for daemon wiring in the next Phase 3 plans.
- Verification passed for `config`, `control`, and `lifecycle` core test targets.

## Self-Check
PASSED

---
*Phase: 03-temperature-control-runtime-operations*
*Completed: 2026-04-11*
