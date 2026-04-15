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

## Remaining Work

### Phase 2: Core crate — config.rs (1954 → ~3 files)

| Target file | Est. LOC | Content |
|---|---:|---|
| `config.rs` | ~290 | AppConfig, DraftConfig, DraftFanEntry, AppliedConfig, AppliedFanEntry, FriendlyNames, FallbackIncident, load/save, resolved_* accessors |
| `validation.rs` | ~330 | ValidationError, ValidationResult, validate_draft, apply_draft, validate_cadence/actuator_limits/pid_limits, find_fan_by_id, temp_source_exists |
| `paths.rs` | ~15 | app_state_dir, state_directory_from_env, CONFIG_VERSION |

Move these types from `config.rs` to `lifecycle.rs` (where they belong semantically):
- `DegradedReason` + Display + is_recoverable
- `DegradedState` + impl
- `LifecycleEvent`, `LifecycleEventLog`, `MAX_LIFECYCLE_EVENTS`

### Phase 3: Core crate — lifecycle.rs (1639 → ~3 files)

| Target file | Est. LOC | Content |
|---|---:|---|
| `lifecycle.rs` | ~350 | OwnedFanSet, FallbackResult, lifecycle_event_from_fallback_incident, DegradedReason/State/LifecycleEvent (moved from config) |
| `lifecycle/reconcile.rs` | ~200 | reconcile_applied_config, ReconcileOutcome, ReconcileResult, perform_boot_reconciliation |
| `lifecycle/fallback.rs` | ~150 | write_fallback_for_owned, write_fallback_single, PWM_SAFE_MAX, PWM_ENABLE_MANUAL |
| `lifecycle/runtime.rs` | ~250 | ControlRuntimeSnapshot, FanRuntimeStatus, RuntimeState, RuntimeState::build |
| `lifecycle/paths.rs` | ~10 | format_iso8601_now (or merge into existing time helper) |

### Phase 4: CLI — main.rs (1587 → command module directory)

| Target file | Est. LOC | Content |
|---|---:|---|
| `main.rs` | ~80 | Arg parsing, dispatch to command modules |
| `commands/mod.rs` | ~20 | Re-exports |
| `commands/inventory.rs` | ~120 | `inventory` subcommand |
| `commands/status.rs` | ~120 | `status` subcommand |
| `commands/lifecycle.rs` | ~250 | draft/apply/discard/validate/degraded/events/runtime subcommands |
| `commands/control.rs` | ~180 | control status, auto-tune, profile subcommands |
| `commands/friendly.rs` | ~100 | set-sensor-name/set-fan-name/remove-sensor-name/remove-fan-name |

Pattern: each command module owns its zbus proxy calls, JSON output formatting,
and arg definitions. `main.rs` just wires clap subcommands to dispatch.

### Phase 5: Daemon — test relocation (948 → ~15 LOC prod)

Move the 933-line `#[cfg(test)] mod tests` block from `daemon/main.rs` into
companion test modules beside the production code they test:

| Test area | Current location | Move to |
|---|---|---|
| Control supervisor (fan loops, degradation, auto-tune) | `main.rs` | `control/supervisor.rs` / `control/autotune.rs` bottom |
| DBus control iface (auth, profile mutations) | `main.rs` | `dbus/control.rs` bottom |
| Fallback / ownership (incident recording) | `main.rs` | `safety/fallback.rs` / `safety/ownership.rs` bottom |

### Phase 6: Core crate — borderline files (review, not commit)

| File | Lines | Assessment |
|---|---:|---|
| `overview.rs` | 512 | Review — may be coherent enough as-is. Contains overview snapshot types + serialization. Split if it grows. |
| `inventory.rs` | 493 | Review — hardware discovery + inventory types. Discovery logic could extract to `inventory/discover.rs` if it grows. |

### Phase 7: Documentation pass

Add `//!` module docs to every file created during Phase 1 that doesn't yet
have one. Add `///` doc comments on all `pub` types and non-obvious methods.
Add "why" comments at lock-ordering, fallback-safety, and panic-safety points.

## Not in Scope (GUI files)

These are large but in different languages/frameworks and should be addressed
separately as part of GUI-specific refactoring:

- `gui/qml/FanDetailPage.qml` (841 lines)
- `gui/qml/WizardDialog.qml` (837 lines)
- `gui/src/models/draft_model.cpp` (611 lines)
- `gui/src/models/fan_list_model.cpp` (411 lines)