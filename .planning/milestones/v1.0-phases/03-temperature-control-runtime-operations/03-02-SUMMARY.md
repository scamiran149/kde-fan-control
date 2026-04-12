---
phase: 03-temperature-control-runtime-operations
plan: 02
subsystem: daemon
tags: [rust, tokio, zbus, dbus, hwmon, pid]
requires:
  - phase: 02-safe-enrollment-lifecycle-recovery
    provides: owned-fan lifecycle state, fallback recovery, and draft/apply flow
provides:
  - live per-fan daemon control tasks with separate sample/control/write cadences
  - read-open DBus control status surface backed by supervisor snapshots
  - applied-config reconciliation that starts and stops control tasks for owned fans
affects: [daemon, cli, dbus, runtime-control]
tech-stack:
  added: []
  patterns: [tokio select-driven per-fan control loops, read-open zbus control status serialization]
key-files:
  created: []
  modified: [crates/daemon/src/main.rs]
key-decisions:
  - "Keep live control snapshots in a daemon supervisor map and expose them directly over DBus instead of rebuilding them from lifecycle state."
  - "Reconcile control tasks from the applied config plus OwnedFanSet so only daemon-owned fans ever receive runtime writes."
patterns-established:
  - "Per-fan control tasks use three Tokio intervals with select! to separate sensor sampling, PID computation, and actuator writes."
  - "Control status changes emit best-effort DBus signals while read methods stay unprivileged and JSON-serialized."
requirements-completed: [PID-03, PID-04, BUS-03]
duration: 4 min
completed: 2026-04-11
---

# Phase 3 Plan 2: Daemon Runtime Control Wiring Summary

**Live daemon-owned fan PID loops with read-open DBus control status and applied-config task reconciliation.**

## Performance

- **Duration:** 4 min
- **Started:** 2026-04-11T13:39:26-05:00
- **Completed:** 2026-04-11T18:43:19Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- Added a control supervisor that spawns one Tokio task per owned managed fan and runs separate sample, compute, and write cadences.
- Wired live temperature reads, bounded PWM mapping, startup kick behavior, degraded transitions, and ownership-gated writes into the daemon runtime.
- Exposed supervisor-backed control status on `org.kde.FanControl.Control` and reconciled running tasks after boot recovery and successful `apply_draft` updates.

## Task Commits

Each task was committed atomically:

1. **Task 1: Add the control supervisor and per-fan runtime loop** - `b47f1cf` (test), `b760ebf` (feat)
2. **Task 2: Expose live control status over a read-open DBus interface and wire supervisor lifecycle** - `6b60a6c` (feat)

**Plan metadata:** Orchestrator-owned state/docs commit intentionally skipped by executor constraints.

## Files Created/Modified
- `crates/daemon/src/main.rs` - Adds the control supervisor, per-fan PID tasks, DBus control interface, reconciliation hooks, and daemon-level tests.

## Decisions Made
- Kept control-loop runtime data in the daemon supervisor so DBus clients read authoritative live snapshots without mutating lifecycle core contracts.
- Restarted the supervisor task set on reconciliation boundaries so applied-config changes immediately replace old loop state with new owned-fan runtime tasks.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated daemon draft/config compatibility for Phase 3 control fields**
- **Found during:** Task 1 (Add the control supervisor and per-fan runtime loop)
- **Issue:** The daemon still constructed `DraftFanEntry` and degraded-state mappings against the pre-Phase-3 shape, which blocked the new control-loop tests from compiling.
- **Fix:** Added the new optional draft control fields in the daemon enrollment path and handled the expanded validation error variants before implementing supervisor behavior.
- **Files modified:** `crates/daemon/src/main.rs`
- **Verification:** `cargo test -p kde-fan-control-daemon control -- --nocapture`
- **Committed in:** `b47f1cf`

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Required to make the daemon compile against the Phase 3 core contracts before runtime control wiring could land. No scope creep.

## Issues Encountered
- The daemon had not yet been updated for Plan 01's expanded control-profile types, so the red phase initially failed at compile time before the supervisor implementation was added.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- The daemon now publishes live managed-fan control data for CLI and future GUI consumers over DBus.
- Runtime control wiring is in place for upcoming CLI/status and auto-tuning work in the remaining Phase 3 plans.

## Self-Check
PASSED

---
*Phase: 03-temperature-control-runtime-operations*
*Completed: 2026-04-11*
