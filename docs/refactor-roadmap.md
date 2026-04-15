# Refactor Roadmap: Module Breakdown

This document tracks the ongoing effort to break large monolithic files into
focused, well-documented modules across the codebase.

## Guiding Principles

- **Target size**: 120–250 LOC ideal, 250–400 acceptable, >400 needs splitting.
- **Module docs**: Every new file gets a `//!` module-level comment explaining
  purpose, ownership, key invariants, and what it must not do.
- **Public items**: Every public type and non-obvious method gets a `///` doc comment.
- **Why comments**: Short "why" comments before tricky blocks (lock ordering,
  fallback guarantees, panic safety) — not line-by-line narration.
- **Rust convention**: Prefer `lib.rs` + module directories over flat files.
  Split impl blocks across files within the same crate are fine.
- **Test placement**: Integration tests move to companion modules beside the
  code they test, not left in one giant `#[cfg(test)] mod tests` at the bottom.

## Completed

### Phase 1: Daemon monolith → modular crate

**Before**: `crates/daemon/src/main.rs` — 3862 lines
**After**: 29 files across 6 directories, largest file 334 lines

| Module | Lines | Responsibility |
|---|---:|---|
| `main.rs` (prod) | 15 | Thin binary wrapper |
| `lib.rs` | 7 | Crate root + `run()` re-export |
| `args.rs` | 19 | CLI arguments |
| `state.rs` | 98 | Shared daemon state types |
| `time.rs` | 44 | ISO 8601 timestamp helpers |
| `app/startup.rs` | 210 | Discovery, config, reconcile, DBus registration |
| `app/background.rs` | 101 | Watchdog, RPM polling, degraded reassessment |
| `app/shutdown.rs` | 76 | SIGTERM/ctrl-c wait + graceful fallback |
| `control/supervisor.rs` | 241 | ControlSupervisor struct, ctor, reconcile, accessors |
| `control/fan_loop.rs` | 238 | Per-fan PID control loop + temperature sampling |
| `control/autotune.rs` | 254 | Auto-tune state machine + proposal derivation |
| `control/recovery.rs` | 334 | Stale detection, panic checks, degraded reassessment |
| `control/helpers.rs` | 91 | Pure conversion helpers |
| `control/sampling.rs` | 68 | Sysfs sensor/RPM path resolution + PWM writes |
| `dbus/constants.rs` | 12 | Bus names, paths, max name length |
| `dbus/auth.rs` | 165 | Polkit/UID-0 authorization boundary |
| `dbus/helpers.rs` | 72 | parse_control_mode, validation_error_to_degraded_reason |
| `dbus/signals.rs` | 118 | Typed DBus signal emission helpers |
| `dbus/inventory.rs` | 167 | InventoryIface (snapshot + friendly names) |
| `dbus/lifecycle.rs` | 230 | LifecycleIface (draft/apply, degraded state, runtime state) |
| `dbus/lifecycle_apply.rs` | 206 | apply-draft transaction (split from lifecycle) |
| `dbus/control.rs` | 223 | ControlIface (live status, auto-tune, profile mutations) |
| `safety/fallback.rs` | 77 | Graceful fallback incident recording |
| `safety/ownership.rs` | 84 | Owned-fan persistence helpers |
| `safety/panic_hook.rs` | 145 | PanicFallbackMirror + panic hook installation |

### Phase 2: Core crate — config.rs (1954 → 1185 + 453 validation.rs)

Extracted `validation.rs` (453 lines) with `ValidationError`, `ValidationResult`,
`validate_draft`, `apply_draft`, `find_fan_by_id`, `temp_source_exists`, and
private validation helpers. Moved `DegradedReason`, `DegradedState`,
`LifecycleEvent`, `LifecycleEventLog`, `MAX_LIFECYCLE_EVENTS` to `lifecycle`.
Re-exports in `config.rs` preserve backward compatibility. `config.rs` now 1185 lines.

### Phase 3: Core crate — lifecycle.rs (1887 → lifecycle/ directory, 6 files)

Converted `lifecycle.rs` into `lifecycle/` directory with focused submodules:

| Module | Lines | Content |
|---|---:|---|
| `lifecycle/mod.rs` | 31 | Re-exports for backward compatibility |
| `lifecycle/state.rs` | 314 | DegradedReason, DegradedState, LifecycleEvent, LifecycleEventLog |
| `lifecycle/reconcile.rs` | 699 | ReconcileOutcome, reconcile_applied_config, perform_boot_reconciliation |
| `lifecycle/reassess.rs` | 238 | ReassessOutcome, reassess_single_fan |
| `lifecycle/fallback.rs` | 186 | FallbackResult, PWM constants, write_fallback_* |
| `lifecycle/runtime.rs` | 356 | ControlRuntimeSnapshot, FanRuntimeStatus, RuntimeState |
| `lifecycle/owned.rs` | 109 | OwnedFanSet |
| `lifecycle/time.rs` | 33 | format_iso8601_now, civil_from_days |

### Phase 4: CLI — main.rs (1587 → 318 + 5 command modules)

Extracted `commands/` directory with focused submodules:

| Module | Lines | Content |
|---|---:|---|
| `main.rs` | 318 | CLI args, proxy defs, dispatch |
| `commands/inventory.rs` | 142 | inventory subcommand |
| `commands/status.rs` | 489 | state/status subcommand |
| `commands/lifecycle.rs` | 560 | draft/apply/discard/validate/degraded etc. |
| `commands/control.rs` | 256 | control set, auto-tune subcommands |
| `commands/friendly.rs` | 35 | rename/unname sensor/fan names |

### Phase 5: Daemon — test relocation (948 → 15 LOC prod)

Moved 933 lines of integration tests from `daemon/main.rs` into companion `#[cfg(test]`
modules beside the production code they test. Added `test_support.rs` with shared
fixtures. `main.rs` is now exactly 15 lines of production code.

| Test location | Tests |
|---|---|
| `control/supervisor.rs` | 6 supervisor/degrade tests |
| `control/autotune.rs` | 3 auto-tune tests |
| `dbus/control.rs` | 4 control iface tests |
| `dbus/lifecycle_apply.rs` | 2 release_removed_owned tests |
| `safety/fallback.rs` | 1 fallback recorder test |
| `safety/panic_hook.rs` | 1 panic path test |

### Phase 6: Borderline files — review (no split needed)

| File | Lines | Assessment |
|---|---:|---|
| `overview.rs` | 512 | Coherent — ~265 LOC production + ~250 LOC tests. Leave as-is. |
| `inventory.rs` | 493 | Coherent — discovery + types form a natural unit. Leave as-is. |

### Phase 7: Documentation pass

Added `//!` module-level doc comments to 23 files across all three crates
(core, daemon, CLI). 27 other files already had docs and were skipped.

## Not in Scope (GUI files)

These are large but in different languages/frameworks and should be addressed
separately as part of GUI-specific refactoring:

- `gui/qml/FanDetailPage.qml` (841 lines)
- `gui/qml/WizardDialog.qml` (837 lines)
- `gui/src/models/draft_model.cpp` (611 lines)
- `gui/src/models/fan_list_model.cpp` (411 lines)