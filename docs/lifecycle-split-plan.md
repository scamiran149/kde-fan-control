# Lifecycle Module Split Plan

## Goal

Split `crates/core/src/lifecycle.rs` (~1639 lines) into a directory module with focused submodules, maintaining full backward compatibility via re-exports from `mod.rs`.

## Prerequisite

The other agent is moving `DegradedReason`, `DegradedState`, `LifecycleEvent`, `LifecycleEventLog`, and `MAX_LIFECYCLE_EVENTS` from `config.rs` into `lifecycle.rs`. **Do not start the split until that move is complete and merged.** The line ranges below account for those additions (estimated at ~240 new lines interleaved into the existing file).

---

## 1. Current File Structure (lifecycle.rs)

### After the config→lifecycle move completes, the file will contain:

| Line Range (estimated) | Symbol | Category |
|---|---|---|
| 1–23 | Module doc, imports | preamble |
| 24–56 | `ReconcileOutcome` | reconcile |
| 58–72 | `ReconcileResult` | reconcile |
| 74–210 | `reconcile_applied_config()` | reconcile |
| 212–231 | `find_fan_in_snapshot()`, `temp_source_in_snapshot()` | reconcile (helpers) |
| 233–316 | `ReassessOutcome`, `reassess_single_fan()` | reassess |
| 318–401 | `OwnedFanSet` + impl | owned |
| 403–428 | `PWM_SAFE_MAX`, `PWM_ENABLE_MANUAL`, `FallbackResult` + impl | fallback |
| 430–458 | `FallbackIncident::from_owned_and_result()` | fallback |
| 460–469 | `lifecycle_event_from_fallback_incident()` | fallback |
| 471–547 | `write_fallback_for_owned()` | fallback |
| 549–587 | `write_fallback_single()` | fallback |
| 589–620 | `ControlRuntimeSnapshot` + impl | runtime |
| 622–642 | `FanRuntimeStatus` | runtime |
| 644–717 | `RuntimeState` + `build()` | runtime |
| 719–873 | `perform_boot_reconciliation()` | reconcile (integration) |
| 875–913 | `format_iso8601_now()`, `civil_from_days()` | time |
| **(new from config)** ~915–1170 | `MAX_LIFECYCLE_EVENTS`, `DegradedReason` + `Display` + `is_recoverable()` | state |
| **(new from config)** ~1172–1195 | `LifecycleEvent` | state |
| **(new from config)** ~1197–1260 | `LifecycleEventLog` + impl | state |
| **(new from config)** ~1262–1310 | `DegradedState` + impl | state |
| 1312–1639+ | Tests | tests |

### Types being moved IN from config.rs

| Symbol | config.rs lines | Description |
|---|---|---|
| `MAX_LIFECYCLE_EVENTS` | 892 | Const, 64 |
| `DegradedReason` | 896–934 | Enum with 9 variants |
| `DegradedReason::Display` | 936–993 | Display impl |
| `DegradedReason::is_recoverable()` | 995–1012 | Method |
| `LifecycleEvent` | 1015–1026 | Struct |
| `LifecycleEventLog` | 1030–1068 | Struct + impl |
| `DegradedState` | 1078–1126 | Struct + impl |

---

## 2. Target Module Structure

```
crates/core/src/lifecycle/
├── mod.rs          # Re-exports, public API surface
├── reconcile.rs    # Boot reconciliation logic
├── reassess.rs     # Per-fan re-assessment
├── fallback.rs     # Safe-maximum fallback writes
├── runtime.rs      # Runtime status model (DBus-facing)
├── state.rs        # Degraded state, lifecycle events, DegradedReason
├── owned.rs        # OwnedFanSet — runtime ownership tracking
└── time.rs         # format_iso8601_now, civil_from_days
```

### Per-file contents

#### `mod.rs` — Re-exports and public API surface (~50 lines)

```rust
//! Boot reconciliation, runtime ownership tracking, and fallback lifecycle.

mod reconcile;
mod reassess;
mod fallback;
mod runtime;
mod state;
mod owned;
mod time;

// Re-export all public types for backward compatibility.
// Existing imports like `kde_fan_control_core::lifecycle::OwnedFanSet` must continue to work.
pub use reconcile::{ReconcileOutcome, ReconcileResult, reconcile_applied_config, perform_boot_reconciliation};
pub use reassess::{ReassessOutcome, reassess_single_fan};
pub use fallback::{PWM_SAFE_MAX, PWM_ENABLE_MANUAL, FallbackResult, write_fallback_for_owned, write_fallback_single, lifecycle_event_from_fallback_incident};
pub use runtime::{ControlRuntimeSnapshot, FanRuntimeStatus, RuntimeState};
pub use state::{DegradedReason, DegradedState, LifecycleEvent, LifecycleEventLog, MAX_LIFECYCLE_EVENTS};
pub use owned::OwnedFanSet;
pub use time::format_iso8601_now;
```

#### `reconcile.rs` — Boot reconciliation (~300 lines)

| Symbol | Source Lines |
|---|---|
| `ReconcileOutcome` (enum, 5 variants) | 24–56 |
| `ReconcileResult` (struct) | 58–72 |
| `reconcile_applied_config()` | 74–210 |
| `find_fan_in_snapshot()` (pub(crate)) | 212–222 |
| `temp_source_in_snapshot()` (pub(crate)) | 224–231 |
| `perform_boot_reconciliation()` | 719–873 |

**Imports:** `crate::config::{AppliedConfig, AppliedFanEntry}`, `crate::inventory::{...}`, `crate::lifecycle::state::{DegradedReason, LifecycleEvent, LifecycleEventLog}`, `crate::lifecycle::owned::OwnedFanSet`, `crate::lifecycle::time::format_iso8601_now`, `super::state::{DegradedState}`

**Notes:** `find_fan_in_snapshot` and `temp_source_in_snapshot` are currently `fn` (private to the module). After the split, `reassess.rs` needs them too. Make them `pub(super)` so sibling modules in `lifecycle/` can use them, or move them to `reconcile.rs` and re-export as `pub(super)`.

**Tests:** The reconciliation tests (lines 919–1349) cover both `reconcile_applied_config` and `perform_boot_reconciliation`. They should move into `reconcile.rs` as a `#[cfg(test)] mod tests` submodule.

#### `reassess.rs` — Per-fan re-assessment (~100 lines)

| Symbol | Source Lines |
|---|---|
| `ReassessOutcome` (enum, 2 variants) | 233–247 |
| `reassess_single_fan()` | 249–316 |

**Imports:** `crate::config::AppliedFanEntry`, `crate::inventory::{...}`, `crate::lifecycle::state::DegradedReason`, `crate::lifecycle::reconcile::{find_fan_in_snapshot, temp_source_in_snapshot}` (or pub(super) re-exports)

**Tests:** `reassess_single_fan_*` tests (lines 1512–1620) move here.

#### `fallback.rs` — Safe-maximum fallback writes (~180 lines)

| Symbol | Source Lines |
|---|---|
| `PWM_SAFE_MAX` (const) | 408 |
| `PWM_ENABLE_MANUAL` (const) | 411 |
| `FallbackResult` (struct + impl) | 413–428 |
| `FallbackIncident::from_owned_and_result()` | 430–458 |
| `lifecycle_event_from_fallback_incident()` | 460–469 |
| `write_fallback_for_owned()` | 471–547 |
| `write_fallback_single()` | 549–587 |

**Imports:** `crate::config::{FallbackIncident, FallbackFailure}`, `crate::lifecycle::owned::OwnedFanSet`, `crate::lifecycle::state::{LifecycleEvent, DegradedReason}`

**Note on `FallbackIncident::from_owned_and_result`:** This is an impl block on a type defined in `config.rs`. It can stay in `fallback.rs` — Rust allows impl blocks in any module as long as the type and the impl are in the same crate.

**Tests:** `fallback_incident_records_only_owned_fans` (lines 1483–1510) moves here.

#### `runtime.rs` — Runtime status model (~170 lines)

| Symbol | Source Lines |
|---|---|
| `ControlRuntimeSnapshot` (struct + impl) | 593–620 |
| `FanRuntimeStatus` (enum) | 622–642 |
| `RuntimeState` (struct + impl + `build()`) | 644–717 |

**Imports:** `crate::config::AppliedConfig`, `crate::control::AggregationFn`, `crate::inventory::{ControlMode, InventorySnapshot}`, `crate::lifecycle::owned::OwnedFanSet`, `crate::lifecycle::state::{DegradedReason, DegradedState}`

**Tests:** `runtime_state_*` tests (lines 1351–1458, 1622–1639) move here.

#### `state.rs` — Degraded state and lifecycle events (~240 lines)

| Symbol | Source Lines |
|---|---|
| `DegradedReason` (enum + `Display` + `is_recoverable()`) | (from config.rs) ~915–1012 |
| `LifecycleEvent` (struct) | (from config.rs) ~1015–1026 |
| `LifecycleEventLog` (struct + impl) | (from config.rs) ~1030–1068 |
| `MAX_LIFECYCLE_EVENTS` (const) | (from config.rs) ~892 |
| `DegradedState` (struct + impl) | (from config.rs) ~1078–1126 |

**Imports:** `serde::{Deserialize, Serialize}`, `std::collections::HashMap`, `crate::inventory::SupportState`, `crate::inventory::ControlMode`

**Tests:** `lifecycle_event_log_bounds`, `degraded_reason_display`, `degraded_state_is_fan_recoverable`, `is_recoverable_classifies_degraded_reasons` tests move here.

#### `owned.rs` — Runtime ownership tracking (~90 lines)

| Symbol | Source Lines |
|---|---|
| `OwnedFanSet` (struct + impl) | 318–401 |

**Imports:** `serde::{Deserialize, Serialize}`, `std::collections::{HashMap, HashSet}`, `crate::inventory::ControlMode`

**Tests:** `owned_fan_set_*` tests (lines 1203–1250) move here.

#### `time.rs` — Timestamp utilities (~40 lines)

| Symbol | Source Lines |
|---|---|
| `format_iso8601_now()` | 880–898 |
| `civil_from_days()` (private) | 900–913 |

**Imports:** `std::time::{SystemTime, UNIX_EPOCH}` (currently inline in the function body, but can be at module level)

---

## 3. Estimated File Sizes

| File | Est. Lines | Description |
|---|---|---|
| `mod.rs` | ~50 | Re-exports + module doc |
| `reconcile.rs` | ~380 | Types, functions, and tests |
| `reassess.rs` | ~120 | Types, function, and tests |
| `fallback.rs` | ~210 | Constants, types, functions, and tests |
| `runtime.rs` | ~200 | Structs, enum, and tests |
| `state.rs` | ~260 | Types from config.rs + tests |
| `owned.rs` | ~120 | Type + impl + tests |
| `time.rs` | ~50 | Utility functions |

**Total:** ~1390 lines (slight reduction from removing duplicated imports / comment padding)

---

## 4. Dependency Graph

```
                    mod.rs (re-exports only)
                   /    |    |    \    \     \
                  /     |    |     \    \     \
           reconcile  reassess  fallback  runtime  owned  time
              |   \      |       /    \    /    |
              |    \     |      /      \  /     |
              |     +----+------+(uses)  +       |
              |           |            (uses)    |
              |         state         state      |
              |           |             |        |
              +-----------+-------------+--------+
                        (all use state)
```

### Detailed internal dependency matrix

| Module | Depends on |
|---|---|
| `reconcile` | `state`, `owned`, `time`, `config`, `inventory` |
| `reassess` | `state`, `reconcile` (for `find_fan_in_snapshot`, `temp_source_in_snapshot`), `config`, `inventory` |
| `fallback` | `state`, `owned`, `config` |
| `runtime` | `state`, `owned`, `config`, `control`, `inventory` |
| `state` | `config` (for `SupportState`, `ControlMode` — if those move to inventory), `inventory` |
| `owned` | `inventory` (for `ControlMode`) |
| `time` | (no internal deps — pure utility) |

### Circular dependency risk

`reassess.rs` needs `find_fan_in_snapshot` and `temp_source_in_snapshot` from `reconcile.rs`. Two options:

1. **Move helpers to `mod.rs`** as `pub(super)` functions — both `reconcile` and `reassess` can access them.
2. **Keep in `reconcile.rs` and re-export as `pub(super)`** — `reassess` imports via `super::reconcile::find_fan_in_snapshot`.

**Recommendation:** Option 1. Move `find_fan_in_snapshot` and `temp_source_in_snapshot` into a private helper section in `mod.rs` (or a small `helpers.rs` file) and mark them `pub(super)` so both `reconcile` and `reassess` can use them. This avoids reassess depending on reconcile.

Actually, cleaner option: just put them in `reconcile.rs` as `pub(super)` and have `reassess.rs` import via `super::reconcile::find_fan_in_snapshot`. This is simpler.

---

## 5. Re-export Requirements for Backward Compatibility

All external code imports from `kde_fan_control_core::lifecycle` using paths like:
- `kde_fan_control_core::lifecycle::OwnedFanSet`
- `kde_fan_control_core::lifecycle::ReconcileOutcome`
- `kde_fan_control_core::lifecycle::RuntimeState`
- etc.

The `mod.rs` must re-export **every** `pub` item that is currently exported from the flat module. The complete list:

### Must re-export from mod.rs

From `reconcile`:
- `ReconcileOutcome`
- `ReconcileResult`
- `reconcile_applied_config`
- `perform_boot_reconciliation`

From `reassess`:
- `ReassessOutcome`
- `reassess_single_fan`

From `fallback`:
- `PWM_SAFE_MAX`
- `PWM_ENABLE_MANUAL`
- `FallbackResult`
- `lifecycle_event_from_fallback_incident`
- `write_fallback_for_owned`
- `write_fallback_single`

From `runtime`:
- `ControlRuntimeSnapshot`
- `FanRuntimeStatus`
- `RuntimeState`

From `state` (being moved IN from config.rs):
- `DegradedReason`
- `DegradedState`
- `LifecycleEvent`
- `LifecycleEventLog`
- `MAX_LIFECYCLE_EVENTS`

From `owned`:
- `OwnedFanSet`

From `time`:
- `format_iso8601_now`

### config.rs must also re-export

After `DegradedReason`, `DegradedState`, `LifecycleEvent`, `LifecycleEventLog`, and `MAX_LIFECYCLE_EVENTS` move from `config.rs` to `lifecycle/state.rs`, `config.rs` must add re-exports so existing code using `kde_fan_control_core::config::DegradedReason` continues to compile:

```rust
// In config.rs — backward-compat re-exports
pub use crate::lifecycle::{DegradedReason, DegradedState, LifecycleEvent, LifecycleEventLog, MAX_LIFECYCLE_EVENTS};
```

This is the other agent's responsibility, but documenting it here for completeness.

---

## 6. Execution Order (Minimize Breakage)

The goal is to make the split in a sequence where each step compiles and all tests pass.

### Step 0: Ensure config→lifecycle move is complete

Wait for the other agent to finish moving `DegradedReason`, `DegradedState`, `LifecycleEvent`, `LifecycleEventLog`, `MAX_LIFECYCLE_EVENTS` from config.rs to lifecycle.rs, with re-exports from config.rs. Verify `cargo test -p kde-fan-control-core` passes.

### Step 1: Create directory structure, move everything at once

This is the simplest approach — do the whole split in a single commit since we're just reorganizing within one crate.

1. Create `crates/core/src/lifecycle/` directory.
2. Create all sub-module files with their contents extracted from the current lifecycle.rs.
3. Create `mod.rs` with all re-exports.
4. Delete `crates/core/src/lifecycle.rs` (the file module).
5. `lib.rs` already says `pub mod lifecycle;` — Rust will find `lifecycle/mod.rs` automatically. No change needed.
6. Run `cargo test -p kde-fan-control-core` to verify.
7. Run `cargo build` to verify daemon and CLI compile.
8. Run `cargo clippy` and `cargo fmt`.

### Step 2: Verify all downstream crates compile

```bash
cargo build
cargo test
cargo clippy
```

### Alternative: Incremental extraction

If preferred, extract modules one at a time:

1. **Extract `time.rs`** — no dependencies on other lifecycle items. Purest extraction.
2. **Extract `owned.rs`** — depends only on `ControlMode` from inventory. Simple.
3. **Extract `state.rs`** — the types moved from config. Depends on inventory types.
4. **Extract `reassess.rs`** — depends on state, reconcile helpers.
5. **Extract `fallback.rs`** — depends on owned, state, config.
6. **Extract `runtime.rs`** — depends on owned, state, config, control, inventory.
7. **Extract `reconcile.rs`** — depends on state, owned, time, config, inventory. What remains becomes `mod.rs`.

After each extraction, verify `cargo test -p kde-fan-control-core` passes.

---

## 7. Test Distribution

Tests in the current `lifecycle.rs` (lines 915–1639) should move to their respective submodules:

| Test function | Target module |
|---|---|
| `reconcile_exact_match_restore` | `reconcile.rs` |
| `reconcile_missing_fan_id` | `reconcile.rs` |
| `reconcile_changed_support_state` | `reconcile.rs` |
| `reconcile_changed_control_mode` | `reconcile.rs` |
| `reconcile_missing_temp_source` | `reconcile.rs` |
| `reconcile_partial_restore` | `reconcile.rs` |
| `boot_reconciliation_restores_matching_fans` | `reconcile.rs` |
| `boot_reconciliation_skips_missing_fans` | `reconcile.rs` |
| `boot_reconciliation_no_applied_config` | `reconcile.rs` |
| `boot_reconciliation_empty_applied_config` | `reconcile.rs` |
| `owned_fan_set_claim_and_release` | `owned.rs` |
| `owned_fan_set_never_contains_unmanaged` | `owned.rs` |
| `owned_fan_set_release_all` | `owned.rs` |
| `runtime_state_build_managed_and_unmanaged` | `runtime.rs` |
| `runtime_state_build_degraded_fan` | `runtime.rs` |
| `runtime_state_build_fallback_fan` | `runtime.rs` |
| `runtime_state_rebuild_marks_persisted_fallback_after_restart` | `runtime.rs` |
| `lifecycle_runtime_snapshot_serializes_control_payload` | `runtime.rs` |
| `fallback_incident_records_only_owned_fans` | `fallback.rs` |
| `reassess_single_fan_recovers_when_temp_source_returns` | `reassess.rs` |
| `reassess_single_fan_still_degraded_when_fan_missing` | `reassess.rs` |
| `reassess_single_fan_recovers_when_control_mode_available` | `reassess.rs` |
| `runtime_state_all_unmanaged_by_default` | `runtime.rs` |

Tests from config.rs that move with the types:

| Test function | Target module |
|---|---|
| `lifecycle_event_log_bounds` | `state.rs` |
| `degraded_reason_display` | `state.rs` |
| `is_recoverable_classifies_degraded_reasons` | `state.rs` |
| `degraded_state_is_fan_recoverable` | `state.rs` |

---

## 8. Items to NOT Move (Staying in config.rs)

These types remain in config.rs and should NOT be extracted into lifecycle:

- `FallbackIncident` (struct definition) — stays in config.rs
- `FallbackFailure` (struct definition) — stays in config.rs

The `FallbackIncident::from_owned_and_result()` impl block will be in `lifecycle/fallback.rs`, which is valid since the struct and impl are in the same crate.

---

## 9. Summary of Imports Needed Per Submodule

Each submodule will need to import from other crate modules and from sibling submodules within `lifecycle/`. Here's the full listing:

### `time.rs`
```rust
use std::time::{SystemTime, UNIX_EPOCH};
```

### `owned.rs`
```rust
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use crate::inventory::ControlMode;
```

### `state.rs`
```rust
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::inventory::{ControlMode, SupportState};
```

### `reconcile.rs`
```rust
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::config::{AppliedConfig, AppliedFanEntry};
use crate::inventory::{ControlMode, FanChannel, InventorySnapshot, SupportState};
use super::state::{DegradedReason, DegradedState, LifecycleEvent, LifecycleEventLog};
use super::owned::OwnedFanSet;
use super::time::format_iso8601_now;
```

### `reassess.rs`
```rust
use serde::{Deserialize, Serialize};
use crate::config::AppliedFanEntry;
use crate::inventory::{ControlMode, InventorySnapshot, SupportState};
use super::state::DegradedReason;
use super::reconcile::{find_fan_in_snapshot, temp_source_in_snapshot};
```

### `fallback.rs`
```rust
use serde::{Deserialize, Serialize};
use crate::config::{FallbackFailure, FallbackIncident};
use super::owned::OwnedFanSet;
use super::state::{DegradedReason, LifecycleEvent};
```

### `runtime.rs`
```rust
use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use crate::config::AppliedConfig;
use crate::control::AggregationFn;
use crate::inventory::{ControlMode, InventorySnapshot};
use super::owned::OwnedFanSet;
use super::state::{DegradedReason, DegradedState};
```

---

## 10. Checklist for the Implementor

- [ ] Wait for the config→lifecycle type move to complete and merge
- [ ] Create `crates/core/src/lifecycle/` directory
- [ ] Create `lifecycle/mod.rs` with module declarations and re-exports
- [ ] Create `lifecycle/time.rs` — extract `format_iso8601_now` + `civil_from_days`
- [ ] Create `lifecycle/owned.rs` — extract `OwnedFanSet`
- [ ] Create `lifecycle/state.rs` — extract `DegradedReason`, `DegradedState`, `LifecycleEvent`, `LifecycleEventLog`, `MAX_LIFECYCLE_EVENTS` (the types already moved from config)
- [ ] Create `lifecycle/reconcile.rs` — extract reconciliation types, functions, `perform_boot_reconciliation`, helper functions, and tests; mark helpers `pub(super)`
- [ ] Create `lifecycle/reassess.rs` — extract `ReassessOutcome` + `reassess_single_fan` + tests
- [ ] Create `lifecycle/fallback.rs` — extract PWM constants, `FallbackResult`, `FallbackIncident::from_owned_and_result`, `lifecycle_event_from_fallback_incident`, `write_fallback_for_owned`, `write_fallback_single`, and tests
- [ ] Create `lifecycle/runtime.rs` — extract `ControlRuntimeSnapshot`, `FanRuntimeStatus`, `RuntimeState`, and tests
- [ ] Delete `crates/core/src/lifecycle.rs` (the old single file)
- [ ] Verify `lib.rs` contains `pub mod lifecycle;` (no change needed — Rust resolves to `lifecycle/mod.rs`)
- [ ] Run `cargo test -p kde-fan-control-core`
- [ ] Run `cargo build` (full workspace)
- [ ] Run `cargo clippy`
- [ ] Run `cargo fmt`
- [ ] Update `AGENTS.md` crate map if line references have shifted significantly