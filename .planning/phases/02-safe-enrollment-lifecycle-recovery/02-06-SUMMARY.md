---
phase: 02-safe-enrollment-lifecycle-recovery
plan: 06
subsystem: lifecycle
tags: [fallback, safety, panic-hook, persistence, runtime-state, lifecycle-events]

# Dependency graph
requires:
  - phase: 02-safe-enrollment-lifecycle-recovery
    provides: boot reconciliation, owned-fan tracking, graceful fallback writes, lifecycle DBus surface
provides:
  - durable fallback incident persistence keyed from OwnedFanSet
  - restart-visible fallback runtime state and reconstructed lifecycle event coverage
  - shared fallback recorder used by ctrl-c shutdown and panic-hook failure paths
  - explicit boot reconciliation lifecycle events without sentinel degraded reasons
affects: [phase-02-verification, daemon runtime status, cli lifecycle inspection]

# Tech tracking
tech-stack:
  added: []
  patterns: [daemon-owned-fallback-incident, shared-fallback-recorder, panic-hook-best-effort-recovery, persisted-fallback-runtime-state]

key-files:
  created:
    - .planning/phases/02-safe-enrollment-lifecycle-recovery/02-06-SUMMARY.md
  modified:
    - crates/core/src/config.rs
    - crates/core/src/lifecycle.rs
    - crates/daemon/src/main.rs

key-decisions:
  - "Fallback persistence stores only daemon-owned evidence: timestamp, affected owned fan IDs, per-fan write failures, and a trigger detail string"
  - "Explicit apply clears any persisted fallback incident and fallback runtime markers so operator action supersedes stale failure state"
  - "Unexpected-exit protection uses a best-effort panic hook with non-blocking lock acquisition, while explicitly not claiming protection for SIGKILL or power loss"

patterns-established:
  - "Fallback state is reconstructed from daemon-owned persisted incidents instead of ephemeral shutdown-only memory"
  - "Lifecycle history uses explicit boot and fallback reason variants instead of sentinel FanMissing payloads"
  - "Daemon shutdown and panic paths share one fallback-recording helper before persistence"

requirements-completed: [FAN-05, SAFE-01, SAFE-03]

# Metrics
duration: 9min
completed: 2026-04-11
---

# Phase 2 Plan 6: Failure-Path Fallback Persistence Summary

**Crash and shutdown fallback now share one durable recorder that persists owned-fan incidents, reloads fallback visibility after restart, and keeps lifecycle history distinct from boot reconciliation state.**

## Performance

- **Duration:** 9 min
- **Started:** 2026-04-11T16:05:00Z
- **Completed:** 2026-04-11T16:13:48Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Added a daemon-owned `FallbackIncident` record that persists affected owned fan IDs, failure details, and timestamps across restart
- Rebuilt fallback runtime visibility and lifecycle-event inspection from persisted incident state on daemon startup
- Centralized fallback recording so both Ctrl-C shutdown and panic-hook failure paths write the same safe-maximum and persistence flow
- Replaced misleading boot-success sentinel events with explicit lifecycle reason variants

## Task Commits

Each task was committed atomically:

1. **Task 1: Add persistent fallback incident state and tests** - `68de1e6` (test), `ef03dc0` (feat)
2. **Task 2: Wire fallback for graceful and unexpected exit paths** - `50dfe5f` (test), `6dc61cb` (feat)

**Plan metadata:** pending summary commit

## Files Created/Modified
- `crates/core/src/config.rs` - Added durable fallback incident and explicit lifecycle reason variants for boot and fallback history
- `crates/core/src/lifecycle.rs` - Added fallback-incident reconstruction helpers and replaced sentinel boot lifecycle events with explicit variants
- `crates/daemon/src/main.rs` - Centralized fallback recording, reloaded persisted fallback state at startup, and installed panic-hook best-effort fallback handling
- `.planning/phases/02-safe-enrollment-lifecycle-recovery/02-06-SUMMARY.md` - Recorded execution outcomes and task commits for this gap-closure plan

## Decisions Made
- Persisted only minimal fallback evidence needed for diagnosis instead of a full durable event log, then reconstructed the inspectable fallback lifecycle event on restart from that incident
- Kept fallback targeting strictly bound to `OwnedFanSet`, so unmanaged fans cannot enter persisted fallback state or receive failure-path writes
- Cleared persisted fallback state on explicit draft apply because operator action supersedes the prior failure incident and returns runtime status authority to the new applied config

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Replaced boot-success sentinel events with explicit lifecycle reasons**
- **Found during:** Task 2 (Wire fallback for graceful and unexpected exit paths)
- **Issue:** Boot restore success was encoded as fake `FanMissing` events, which made lifecycle history misleading and prevented fallback incidents from being clearly distinct
- **Fix:** Added explicit `BootRestored` and `BootReconciled` lifecycle reasons and updated reconciliation event emission to use them
- **Files modified:** crates/core/src/config.rs, crates/core/src/lifecycle.rs
- **Verification:** `cargo test -p kde-fan-control-core lifecycle -- --nocapture`, `cargo test --workspace`
- **Committed in:** `6dc61cb`

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** The fix was necessary to make fallback incidents diagnosable and satisfy the plan’s distinct lifecycle-history requirement without scope creep.

## Issues Encountered

- Panic-path fallback cannot safely block on async locks, so the daemon uses `try_read`/`try_write` in the panic hook and logs best-effort behavior instead of over-claiming impossible guarantees.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 2 verification can now re-check SAFE-01 and SAFE-03 against durable fallback visibility after restart
- Runtime-state and lifecycle-event inspection now distinguish fallback incidents from normal boot reconciliation outcomes
- STATE.md and ROADMAP.md were intentionally left untouched for the orchestrator to update

## Self-Check: PASSED

- .planning/phases/02-safe-enrollment-lifecycle-recovery/02-06-SUMMARY.md: FOUND
- Commit 68de1e6: FOUND
- Commit ef03dc0: FOUND
- Commit 50dfe5f: FOUND
- Commit 6dc61cb: FOUND

---
*Phase: 02-safe-enrollment-lifecycle-recovery*
*Completed: 2026-04-11*
