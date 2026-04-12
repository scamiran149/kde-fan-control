---
phase: 02-safe-enrollment-lifecycle-recovery
plan: 05
subsystem: inventory
tags: [hwmon, inventory, control-modes, pwm, voltage, serde, regression-tests]

# Dependency graph
requires:
  - phase: 01-hwmon-inventory-discovery
    provides: live inventory snapshot model, fan support classification, control mode enum
  - phase: 02-safe-enrollment-lifecycle-recovery
    provides: lifecycle validation and downstream mode consumers in config, daemon, and cli layers
provides:
  - multi-mode fan discovery that advertises voltage only when writable hwmon pwm_mode proves it
  - regression fixtures covering pwm-plus-voltage, pwm-only, and non-writable mode selector hardware
  - serialization coverage proving inventory snapshots carry discovered control_modes unchanged downstream
affects: [02 verification, daemon lifecycle mode selection, cli enrollment choices]

# Tech tracking
tech-stack:
  added: []
  patterns: [hwmon-pwm-mode-heuristic, fixture-backed-sysfs-discovery-tests, snapshot-roundtrip-regression]

key-files:
  created:
    - .planning/phases/02-safe-enrollment-lifecycle-recovery/02-05-SUMMARY.md
  modified:
    - crates/core/src/inventory.rs

key-decisions:
  - "Voltage support is advertised only when pwmN is writable and pwmN_mode is also writable, because hwmon uses that selector as the concrete proof that direct-current control is user-selectable"
  - "Channels stay PWM-only when pwmN_mode is absent or read-only instead of inferring voltage support from board-specific naming"
  - "Inventory snapshot round-trip tests live in core so DBus and CLI consumers stay bound to the same authoritative control_modes list"

patterns-established:
  - "Mode discovery stays inventory-authoritative: downstream layers consume FanChannel.control_modes instead of re-deriving hardware capability"
  - "Sysfs capability tests use temp fixture directories with explicit permissions to model writable vs non-writable hwmon nodes"

requirements-completed: [FAN-04]

# Metrics
duration: 1min
completed: 2026-04-11
---

# Phase 2 Plan 5: Control-Mode Discovery Gap Closure Summary

**Hwmon inventory now distinguishes PWM-only fans from channels with writable direct-current mode switching, and serialized snapshots preserve those discovered control choices for daemon and CLI consumers.**

## Performance

- **Duration:** 1 min
- **Started:** 2026-04-11T16:03:35Z
- **Completed:** 2026-04-11T16:04:39Z
- **Tasks:** 2
- **Files modified:** 1

## Accomplishments
- Added fixture-backed inventory tests for writable pwm_mode, PWM-only hardware, and read-only mode selectors
- Extended live fan discovery to surface `ControlMode::Voltage` only when hwmon exposes a writable `pwmN_mode` selector alongside writable `pwmN`
- Added snapshot serialization round-trip coverage proving downstream consumers receive the discovered `control_modes` list unchanged

## Task Commits

Each task was committed atomically:

1. **Task 1: Add real multi-mode discovery rules** - `3f3d313` (test), `d44212b` (feat)
2. **Task 2: Prove downstream surfaces now reflect discovered modes** - `5d540c8` (test)

**Plan metadata:** pending (STATE.md and ROADMAP.md intentionally not updated by this executor)

## Files Created/Modified
- `crates/core/src/inventory.rs` - Added hwmon control-mode detection via writable `pwmN_mode` and fixture-backed regression coverage for discovery plus snapshot serialization
- `.planning/phases/02-safe-enrollment-lifecycle-recovery/02-05-SUMMARY.md` - Recorded execution outcome, task commits, and verification results for the orchestrator

## Decisions Made
- Used writable `pwmN_mode` as the safe kernel-backed heuristic for `ControlMode::Voltage`, because hwmon documents that selector as the PWM vs direct-current mode switch
- Kept support classification unchanged for PWM-capable hardware even when voltage switching is unavailable, so the inventory reports real capability without over-promoting ambiguous hardware
- Verified downstream propagation in core snapshot serialization rather than adding daemon or CLI wiring changes, because those layers already consume the shared inventory shape

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 2 verification can now re-check FAN-04 against real inventory-discovered multi-mode capabilities
- Daemon and CLI lifecycle flows can rely on `FanChannel.control_modes` as the single authoritative source for selectable hardware modes

## Self-Check: PASSED

- crates/core/src/inventory.rs: FOUND
- .planning/phases/02-safe-enrollment-lifecycle-recovery/02-05-SUMMARY.md: FOUND
- Commit 3f3d313: FOUND
- Commit d44212b: FOUND
- Commit 5d540c8: FOUND

---
*Phase: 02-safe-enrollment-lifecycle-recovery*
*Completed: 2026-04-11*
