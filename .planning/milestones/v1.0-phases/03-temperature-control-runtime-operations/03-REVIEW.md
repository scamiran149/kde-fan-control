---
phase: 03-temperature-control-runtime-operations
reviewed: 2026-04-11T17:30:00Z
depth: standard
files_reviewed: 5
files_reviewed_list:
  - crates/core/src/config.rs
  - crates/core/src/control.rs
  - crates/core/src/lifecycle.rs
  - crates/daemon/src/main.rs
  - crates/cli/src/main.rs
findings:
  critical: 0
  warning: 5
  info: 5
  total: 10
status: issues_found
---

# Phase 03: Code Review Report

**Reviewed:** 2026-04-11T17:30:00Z
**Depth:** Standard
**Files Reviewed:** 5
**Status:** issues_found

## Summary

Reviewed the five core source files modified across Phase 03 (temperature-control-runtime-operations). The phase adds PID fan control loops, aggregation functions, auto-tune orchestration, runtime status tracking via DBus, boot reconciliation, and fallback safety — all backed by extensive tests (67 total, all passing).

The codebase demonstrates solid safety engineering: fallback mechanisms drive fans to safe maximum on shutdown/panic/failure, ownership tracking prevents writing to unmanaged fans, and the serde backward compatibility fix correctly ensures older Phase 2 config files deserialize with safe defaults for new Phase 3 fields. Authorization is enforced at the DBus boundary for all privileged write operations.

Five warnings and five info items were identified. No critical security or crash-inducing bugs were found.

## Critical Issues

None.

## Warnings

### WR-01: Integer division truncation in `AggregationFn::compute_millidegrees` average path

**File:** `crates/core/src/control.rs:25`
**Issue:** The `Average` branch computes `readings.iter().sum::<i64>() / readings.len() as i64`. This is integer division and silently truncates (rounds toward zero). For temperature values in millidegrees, a single reading with many sensors could lose up to N-1 millidegrees of precision. While this may not cause thermal risk in practice (the error is bounded by the number of sensors), it's inconsistent with the `Median` branch which computes an average of two middle values using the same truncating division — a double-truncation compound.

**Fix:** Consider using `f64` for the average computation and rounding the result:
```rust
Self::Average => {
    let sum: i64 = readings.iter().sum();
    let len = readings.len() as f64;
    Some((sum as f64 / len).round() as i64)
}
```

### WR-02: `DraftFanControlProfilePayload` uses `Option<Option<T>>` — confusing double-optional semantics

**File:** `crates/daemon/src/main.rs:123-138`
**Issue:** The `DraftFanControlProfilePayload` struct fields use `Option<Option<i64>>`, `Option<Option<AggregationFn>>`, etc. While this is intentional to distinguish "not set in JSON" from "set to null" (clearing a value), it's a source of confusion and bugs. When `serde_json::from_str` parses a JSON payload that omits a field, the outer `Option` is `None` (because of `#[serde(default)]`). When a caller explicitly passes `null`, the inner `Option` is `None`. However, the `set_draft_fan_control_profile_inner` method (line ~1094-1114) only checks `if let Some(value) = patch.xxx` and sets `draft_entry.xxx = value` — this correctly propagates `Some(None)` as `None`, but the double-optional makes the code harder to reason about and is a maintenance hazard for future contributors.

**Fix:** Add a doc comment on `DraftFanControlProfilePayload` explaining the semantics:
```rust
/// Partial-update payload for draft fan control profile fields.
/// Outer Option = field not present in JSON (no change).
/// Inner Option = field present but set to null (clear the value).
/// Some(Some(value)) = field present with a value (set the value).
```

### WR-03: `PanicFallbackMirror` uses `StdRwLock` — poison risk could defeat panic fallback

**File:** `crates/daemon/src/main.rs:63-65, 988-991, 996-999`
**Issue:** The `PanicFallbackMirror` uses `std::sync::RwLock<Vec<...>>` for `owned_pwm_paths`. In `sync_panic_fallback_mirror_from_owned` (line 988), the code does `let Ok(mut guard) = mirror.owned_pwm_paths.write()` — silently ignoring the `Err` (poisoned) case. In `write_fallback_from_panic_mirror` (line 996), it does `mirror.owned_pwm_paths.read()` and silently returns empty results if poisoned. The silent empty return in `write_fallback_from_panic_mirror` means a poisoned lock could result in **zero fans receiving their fallback safety write**, defeating the entire purpose of the panic fallback mechanism. This is a safety-relevant concern since the panic fallback's express purpose is to guarantee fans are driven to safe maximum.

**Fix:** Use `parking_lot::RwLock` (which has no poison concept) or explicitly recover from poison in `write_fallback_from_panic_mirror`:
```rust
fn write_fallback_from_panic_mirror(mirror: &PanicFallbackMirror) -> ... {
    let paths = match mirror.owned_pwm_paths.read() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner(), // recover: use last good data
    };
    // ... continue with paths
}
```

### WR-04: `format_iso8601_now` is duplicated across `lifecycle.rs` and `main.rs`

**File:** `crates/core/src/lifecycle.rs:777-810` and `crates/daemon/src/main.rs:1844-1882`
**Issue:** The exact same `format_iso8601_now()` and `civil_from_days()` functions are implemented twice — once in `lifecycle.rs` (public) and once in `main.rs` (private). Both use `SystemTime::now().duration_since(UNIX_EPOCH)`, compute the same civil date algorithm, and produce the same format string. If a bug is fixed in one, the other will be missed. The `lifecycle.rs` version is already `pub`, but the daemon doesn't use it.

**Fix:** The daemon should import and use `kde_fan_control_core::lifecycle::format_iso8601_now` instead of redefining it. Remove the duplicate from `main.rs` and add the import at the top of the daemon file.

### WR-05: Auto-tune sample collection `.expect()` panics in async context

**File:** `crates/daemon/src/main.rs:371, 379`
**Issue:** In `record_auto_tune_sample`, after transitioning from `Running` to `Completed`/`Failed`, the code calls `auto_tune.get_mut(fan_id).expect("state should exist")` to replace the state. If a concurrent `fail_auto_tune` call has removed the fan's state between the initial `if let` check (line 355) and the `.expect()` call, the daemon will panic inside an async task. While this is unlikely (the auto_tune `RwLock` is held during the entire operation from line 354-393), it's a defensive programming concern — `.expect()` on a mutable re-lookup of state that was just confirmed to exist is fragile and could be avoided.

**Fix:** Replace `.expect("state should exist")` with a pattern that handles the concurrent-removal case gracefully:
```rust
if let Some(state) = auto_tune.get_mut(fan_id) {
    *state = AutoTuneExecutionState::Completed { observation_window_ms, proposal };
    should_emit = true;
} // else: state was concurrently removed, skip
```

## Info

### IN-01: `#[allow(dead_code)]` on test-only helper functions

**File:** `crates/daemon/src/main.rs:176, 760, 1125, 1135`
**Issue:** Four `#[allow(dead_code)]` annotations suppress warnings on `set_auto_tune_observation_window_ms`, `require_test_authorized`, `accept_auto_tune_for_test`, and `set_draft_fan_control_profile_for_test`. These are used exclusively in tests but are on non-`#[cfg(test)]` items. The suppression is justified (they are called only from `#[cfg(test)]` blocks), but an alternative would be to gate them with `#[cfg(test)]` to make the intent clearer.

**Fix:** Consider adding `#[cfg(test)]` to `require_test_authorized`, `accept_auto_tune_for_test`, and `set_draft_fan_control_profile_for_test` since they're only called from test code. The `set_auto_tune_observation_window_ms` method may be needed for future runtime use, so the `#[allow(dead_code)]` is acceptable there.

### IN-02: Magic number `5_000` for high-temperature alert threshold

**File:** `crates/daemon/src/main.rs:478, 522`
**Issue:** The high-temperature alert threshold `5_000` (5°C above target) is hardcoded in two places within the `run_fan_loop` method. This should be a named constant for clarity and future configurability.

**Fix:**
```rust
const ALERT_HIGH_TEMP_OFFSET_MILLIDEGREES: i64 = 5_000;
```

### IN-03: `OwnedFanSet` has private serialized fields — inconsistent with other types

**File:** `crates/core/src/lifecycle.rs:244-254`
**Issue:** `OwnedFanSet` is `#[derive(Serialize, Deserialize)]` with private fields `owned`, `control_modes`, `fan_sysfs_paths`. While this works because serde accesses private fields via the derive macro, it's inconsistent with `FallbackResult` (line 329) which uses public fields. The private fields also make it impossible to construct an `OwnedFanSet` with specific `control_modes` and `fan_sysfs_paths` for testing from outside the module without going through `claim_fan`.

**Fix:** Consider making the fields `pub` consistent with other serialized types, or removing `Serialize, Deserialize` from `OwnedFanSet` if it's not meant to be serialized across module boundaries (it appears to be runtime-only state).

### IN-04: CLI `connect_dbus` silently falls back from system bus to session bus

**File:** `crates/cli/src/main.rs:500-509`
**Issue:** The `connect_dbus()` function attempts to connect to the system bus first and silently falls back to the session bus on failure. While this is useful for local development, it means a user running the CLI without the daemon running on the system bus will get a confusing connection to a session bus where no daemon exists, rather than a clear error. The daemon already supports an explicit `--session-bus` flag, but the CLI doesn't expose this choice to the user.

**Fix:** Consider logging the fallback or making it opt-in via a `--session-bus` flag (consistent with the daemon's `--session-bus` flag) rather than silent fallback, or at minimum adding a `tracing::warn!` message.

### IN-05: `ControlRuntimeSnapshot::from_applied_entry` is private — duplicated logic in daemon

**File:** `crates/core/src/lifecycle.rs:503-517` and `crates/daemon/src/main.rs:658-670`
**Issue:** `ControlRuntimeSnapshot::from_applied_entry` is a private method, making it inaccessible from outside the `lifecycle` module. The daemon's `control_snapshot_from_applied` function in `main.rs:658-670` duplicates the same logic instead of reusing the core method. This is minor code duplication that could be eliminated.

**Fix:** Make `from_applied_entry` `pub(crate)` or `pub` so the daemon can reuse it, removing the duplicate `control_snapshot_from_applied` function.

---

_Reviewed: 2026-04-11T17:30:00Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_