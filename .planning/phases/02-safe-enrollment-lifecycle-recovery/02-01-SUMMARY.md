---
phase: 02-safe-enrollment-lifecycle-recovery
plan: 01
subsystem: config
tags: [toml, serde, lifecycle-config, validation, degraded-state]

# Dependency graph
requires:
  - phase: 01-hwmon-inventory-discovery
    provides: stable fan IDs, support classification, control modes, inventory snapshot model
provides:
  - versioned daemon-owned config model (draft + applied + friendly names)
  - validation helpers that check fan IDs, control modes, and enrollability against live inventory
  - best-effort apply_draft() for partial promotion with rejection reporting
  - DegradedReason and LifecycleEventLog for boot-mismatch and fallback state tracking
  - PartialEq/Eq on ControlMode and SupportState for comparison support
affects: [02-02, 02-03, 02-04]

# Tech tracking
tech-stack:
  added: []
  patterns: [draft-apply-config-pattern, best-effort-partial-apply, bounded-lifecycle-event-log, versioned-config-schema]

key-files:
  created: []
  modified:
    - crates/core/src/config.rs
    - crates/core/src/inventory.rs

key-decisions:
  - "AppConfig uses a version field for future schema migration; loads reject future versions"
  - "Draft and Applied config are explicitly separate data structures — draft cannot implicitly become live"
  - "Validation is centralized in core and reusable by both DBus apply path and boot reconciliation"
  - "apply_draft performs best-effort partial apply — valid fans are promoted, invalid ones are reported but don't block"
  - "LifecycleEventLog is bounded to 64 entries, drops oldest on overflow, keeps recent history inspectable"
  - "ControlMode and SupportState now derive PartialEq/Eq to support validation comparisons"

patterns-established:
  - "Draft-apply config pattern: draft edits are validated before promotion to applied; no write-through to live config"
  - "Best-effort partial apply: each fan validated independently, invalid entries reported but not blocking"
  - "Bounded event log: ring buffer with MAX_LIFECYCLE_EVENTS cap, drops oldest entries"
  - "Stable fan ID references: persisted config references inventory stable IDs, not sysfs paths or labels"

requirements-completed: [FAN-01, FAN-02, FAN-03, FAN-05, BUS-04, CONF-01, CONF-02, CONF-03]

# Metrics
duration: 6min
completed: 2026-04-11
---

# Phase 2 Plan 1: Managed Config Domain And Persistence Summary

**Versioned draft/applied lifecycle config with validation helpers, degraded-state tracking, and preserved friendly-name persistence**

## Performance

- **Duration:** 6 min
- **Started:** 2026-04-11T13:19:06Z
- ** **Completed:** 2026-04-11T13:25:15Z
- **Tasks:** 4
- **Files modified:** 2

## Accomplishments
- Extended AppConfig with versioned schema, draft config, and applied config fields
- Added DraftFanEntry and AppliedFanEntry structs for per-fan lifecycle state
- Preserved Phase 1 friendly-name persistence behavior under the expanded config model
- Implemented validation helpers: fan ID existence, control mode support, and enrollability checks
- Added apply_draft() for best-effort partial apply with per-fan rejection reporting
- Modeled degraded-state data: DegradedReason enum, LifecycleEvent, and bounded LifecycleEventLog
- Added PartialEq/Eq derives on ControlMode and SupportState
- Wrote 14 unit tests covering config round-trip, validation rules, and lifecycle log bounds

## Task Commits

Each task was committed atomically:

1. **Tasks 1-4: Lifecycle config domain, validation, degraded state, naming** - `0ddbd59` (feat)

Note: All four tasks modified the same files (config.rs and inventory.rs) with tightly coupled changes, so they were committed as one atomic unit.

**Plan metadata:** pending (docs commit deferred per orchestrator instructions)

## Files Created/Modified
- `crates/core/src/config.rs` - Extended with versioned AppConfig, DraftConfig, AppliedConfig, DraftFanEntry, AppliedFanEntry, FriendlyNames, validation helpers, degraded-state model, lifecycle events, and 14 unit tests
- `crates/core/src/inventory.rs` - Added PartialEq/Eq derives on ControlMode and SupportState

## Decisions Made
- AppConfig uses a version field (CONFIG_VERSION = 1); future schema versions are rejected at load time to force migration
- Draft and Applied are separate structs — no implicit write-through from draft to live config
- Validation lives in core and is reusable by both DBus apply paths and boot reconciliation (per threat model)
- apply_draft() does best-effort partial apply — valid fans are promoted, invalid ones are reported via ValidationResult
- LifecycleEventLog capped at 64 entries, drops oldest on overflow
- ControlMode and SupportState now derive PartialEq/Eq for validation use without breaking serialization

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Config domain model ready for DBus draft/apply contract (Plan 02-02)
- Validation helpers ready for boot reconciliation (Plan 02-03)
- Degraded-state model ready for lifecycle and fallback tracking (Plan 02-03)
- LifecycleEventLog ready for DBus signal emission (Plan 02-02 and 02-04)

## Self-Check: PASSED

- crates/core/src/config.rs: FOUND
- crates/core/src/inventory.rs: FOUND
- 02-01-SUMMARY.md: FOUND
- Commit 0ddbd59: FOUND

---
*Phase: 02-safe-enrollment-lifecycle-recovery*
*Completed: 2026-04-11*