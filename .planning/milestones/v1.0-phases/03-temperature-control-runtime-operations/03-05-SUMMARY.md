---
phase: 03-temperature-control-runtime-operations
plan: 05
subsystem: config
tags: [serde, backward-compat, toml, defaults, dead-code]

# Dependency graph
requires:
  - phase: 03-temperature-control-runtime-operations
    provides: AppliedFanEntry struct with control-profile fields, DraftFanEntry resolved_* defaults
provides:
  - Backward-compatible AppliedFanEntry deserialization with safe serde defaults
  - Zero compiler warnings from daemon build
affects: [config-load, boot-reconciliation, daemon-startup]

# Tech tracking
tech-stack:
  added: []
  patterns: ["serde(default) for backward-compatible config deserialization"]

key-files:
  created: []
  modified:
    - "crates/core/src/config.rs"
    - "crates/daemon/src/main.rs"

key-decisions:
  - "Used serde(default) on AppliedFanEntry fields rather than Option<T> — preserves non-optional type guarantees at runtime while still deserializing cleanly from Phase 2 TOML"
  - "Custom default functions for target_temp_millidegrees (65000) and deadband_millidegrees (1000) to match DraftFanEntry resolved_* semantics"
  - "Used #[allow(dead_code)] rather than #[cfg(test)] on test-only helpers because they live on public struct impls and are called from #[cfg(test)] blocks"

patterns-established:
  - "serde(default) pattern: when adding required fields to existing structs that persisted configs exist without, annotate with serde(default) using custom default functions for domain-specific safe values"

requirements-completed: [SAFE-04, SAFE-06]

# Metrics
duration: 5min
completed: 2026-04-11
---

# Phase 03 Plan 05: Backward-Compat Config Deserialize Summary

**Serde(default) on AppliedFanEntry enables Phase 2 config files to deserialize with safe defaults; dead_code warnings suppressed on test-only daemon helpers**

## Performance

- **Duration:** 5 min
- **Started:** 2026-04-11T21:02:57Z
- **Completed:** 2026-04-11T21:07:48Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- AppliedFanEntry now deserializes Phase 2 TOML configs (missing control-profile fields) with safe defaults matching DraftFanEntry resolved_* methods
- All 4 dead_code compiler warnings in the daemon eliminated with #[allow(dead_code)]
- TDD cycle completed: RED (failing test for missing field) → GREEN (serde defaults) → all tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Add serde(default) to AppliedFanEntry and backward-compat test** — TDD with 3 commits:
   - `fedf698` test(03-05): add failing backward-compat deserialization tests for AppliedFanEntry (RED)
   - `a80d419` feat(03-05): add serde(default) to AppliedFanEntry for backward compatibility (GREEN)
2. **Task 2: Suppress dead_code warnings on test-only daemon functions** — `89bc89c` (fix)

## Files Created/Modified
- `crates/core/src/config.rs` — Added `default_applied_target_temp_millidegrees()` and `default_applied_deadband_millidegrees()` default functions; added `#[serde(default)]` and `#[serde(default = "...")]` annotations to 7 AppliedFanEntry fields; added 2 backward-compat deserialization tests
- `crates/daemon/src/main.rs` — Added `#[allow(dead_code)]` to 4 test-only helper functions

## Decisions Made
- Used `#[serde(default)]` with custom default functions for `target_temp_millidegrees` (65°C) and `deadband_millidegrees` (1°C) to exactly match DraftFanEntry semantics; used plain `#[serde(default)]` for struct fields that implement Default (AggregationFn, PidGains, ControlCadence, ActuatorPolicy, PidLimits)
- Chose `#[allow(dead_code)]` over `#[cfg(test)]` for daemon test helpers because they're methods on public struct impls (ControlSupervisor, ControlIface) called from the test module — `#[cfg(test)]` would require restructuring the public API

## Deviations from Plan

None - plan executed exactly as written.

## Next Phase Readiness
- Phase 2 TOML configs now deserialize cleanly — no more "missing field" errors on cold start
- Previously managed fans are restored from config rather than silently dropped to unmanaged
- UAT smoke test (test 1) should now pass: daemon boots without errors and state returns live runtime data

## Self-Check: PASSED

- All modified files exist: config.rs, main.rs ✅
- SUMMARY.md exists ✅
- All commits found: fedf698 (RED), a80d419 (GREEN), 89bc89c (dead_code fix) ✅

---
*Phase: 03-temperature-control-runtime-operations*
*Completed: 2026-04-11*