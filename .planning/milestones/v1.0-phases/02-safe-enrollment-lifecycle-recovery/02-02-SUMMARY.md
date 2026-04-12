---
phase: 02-safe-enrollment-lifecycle-recovery
plan: 02
subsystem: dbus
tags: [zbus, dbus, lifecycle, draft-apply, authorization, signals, cli]

# Dependency graph
requires:
  - phase: 02-safe-enrollment-lifecycle-recovery
    provides: versioned draft/applied config, validation helpers, DegradedReason, LifecycleEventLog, apply_draft()
provides:
  - DBus Lifecycle interface at /org/kde/FanControl/Lifecycle with draft/apply/degraded/events read methods
  - DBus Lifecycle interface write methods gated by UID-0 authorization check
  - Lifecycle change signals: DraftChanged, AppliedConfigChanged, DegradedStateChanged, LifecycleEventAppended
  - CLI subcommands for draft, applied, degraded, events, enroll, unenroll, discard, validate, apply
  - DegradedState runtime tracking model
  - Authorization boundary helper structured for future polkit replacement
affects: [02-03, 02-04]

# Tech tracking
tech-stack:
  added: []
  patterns: [dbus-read-open-write-privileged, signal-emission-in-interface-methods, uid-based-authorization-check, iso8601-without-chrono]

key-files:
  created: []
  modified:
    - crates/core/src/config.rs
    - crates/daemon/src/main.rs
    - crates/cli/src/main.rs

key-decisions:
  - "DBus read methods (draft config, applied config, degraded summary, lifecycle events) are accessible to all local users; write methods require UID 0"
  - "Authorization check uses org.freedesktop.DBus.GetConnectionUnixUser to resolve caller identity, structured so polkit can replace the UID check without changing method semantics"
  - "Signal emission failures are logged as warnings but do not fail the DBus method call — clients rely on data, not signals, for correctness"
  - "CLI acts as a thin DBus client with no client-side privilege assumptions — access-denied errors are surfaced as user-actionable messages"
  - "ISO 8601 timestamps are generated without chrono dependency using Howard Hinnant's civil_from_days algorithm"

patterns-established:
  - "DBus authorization boundary: require_authorized() helper checks caller UID before write operations; future polkit integration only needs to modify this helper"
  - "Signal emission pattern: signals use #[zbus(signal)] declarations with SignalEmitter parameter in write methods for in-call emission"
  - "DegradedState runtime model: per-fan degraded reasons tracked in memory alongside persisted config, reconstructible from boot reconciliation"

requirements-completed: [BUS-02, BUS-04, BUS-06, CLI-02, CONF-01, CONF-02]

# Metrics
duration: 15min
completed: 2026-04-11
---

# Phase 2 Plan 2: DBus Draft-Apply Contract And Authorization Boundary Summary

**DBus lifecycle surface with staged edits, explicit apply, read-open/write-privileged access, and change signals for draft, applied, degraded, and event state**

## Performance

- **Duration:** 15 min
- **Started:** 2026-04-11T13:31:23Z
- **Completed:** 2026-04-11T14:37:27Z
- **Tasks:** 4
- **Files modified:** 3

## Accomplishments
- Added LifecycleIface DBus interface at /org/kde/FanControl/Lifecycle with read methods (GetDraftConfig, GetAppliedConfig, GetDegradedSummary, GetLifecycleEvents) and write methods (SetDraftFanEnrollment, RemoveDraftFan, DiscardDraft, ValidateDraft, ApplyDraft)
- Implemented authorization boundary using org.freedesktop.DBus.GetConnectionUnixUser to gate all write methods behind UID 0 check, structured for future polkit replacement
- Added four lifecycle change signals: DraftChanged, AppliedConfigChanged, DegradedStateChanged, LifecycleEventAppended
- Extended CLI with lifecycle subcommands: draft, applied, degraded, events, enroll, unenroll, discard, validate, apply
- Added DegradedState runtime tracking to core for per-fan degraded reasons
- Daemon registers both Inventory and Lifecycle interfaces on DBus simultaneously

## Task Commits

Each task was committed atomically:

1. **Tasks 1-3: DBus lifecycle surface, signals, and authorization boundary** - `fbf2f17` (feat)
   - Note: Tasks 1-3 are tightly coupled — the lifecycle surface, signals, and authorization boundary are interleaved in the same interface implementation.

2. **Task 4: CLI lifecycle subcommands and normalized payloads** - `89c29f1` (feat)

## Files Created/Modified
- `crates/core/src/config.rs` - Added DegradedState struct with per-fan degraded-reason tracking and helper methods (mark_degraded, clear_fan, clear_all, has_degraded, degraded_fan_ids)
- `crates/daemon/src/main.rs` - Added LifecycleIface DBus interface with read/write methods, four signals, authorization boundary helper, ISO 8601 timestamp helper, and shared state (config, snapshot, degraded, events)
- `crates/cli/src/main.rs` - Added LifecycleProxy trait, lifecycle subcommands, shared DBus connection helper, validation result display, and access-denied error surfacing

## Decisions Made
- DBus read methods are accessible to all local users; write methods require UID 0 authorization (per plan's read-open/write-privileged policy)
- Authorization helper is a standalone async function (require_authorized) so polkit can replace the UID check without changing DBus method contracts
- Signal emission failures are logged as warnings but do not propagate as method call failures — data correctness depends on read methods, not signals
- DegradedState is runtime-only (not persisted) since it's reconstructed from applied config + live inventory at boot
- Control mode strings for the DBus interface use lowercase ("pwm", "voltage", "none"/"") mapped to ControlMode enum
- ISO 8601 timestamps generated without chrono dependency using Howard Hinnant's civil_from_days algorithm

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- zbus 5.14 API for signal emission within interface methods requires using the SignalEmitter parameter via #[zbus(signal_emitter)] attribute; Self:: signals don't work as method calls — resolved by using emitter.draft_changed() instead of Self::draft_changed()
- zbus 5.14 signal method return type must be explicitly `zbus::Result<()>`, not just `()` with semicolon
- zbus header parameter type is `Header<'_>` (owned), not `&Header<'_>` (reference)
- Bus name type conversion from UniqueName requires explicit `zbus::names::BusName::Unique()` wrapper

## User Setup Required

None - no external service configuration required.

## Self-Check: PASSED

- crates/core/src/config.rs: FOUND
- crates/daemon/src/main.rs: FOUND
- crates/cli/src/main.rs: FOUND
- Commit fbf2f17: FOUND
- Commit 89c29f1: FOUND

---
*Phase: 02-safe-enrollment-lifecycle-recovery*
*Completed: 2026-04-11*