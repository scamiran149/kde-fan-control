---
status: issues_found
phase: 02-safe-enrollment-lifecycle-recovery
reviewed: 2026-04-11T15:36:10Z
depth: standard
files_reviewed: 5
files_reviewed_list:
  - crates/core/src/config.rs
  - crates/core/src/lifecycle.rs
  - crates/core/src/lib.rs
  - crates/daemon/src/main.rs
  - crates/cli/src/main.rs
findings:
  critical: 1
  warning: 3
  info: 0
  total: 4
---

# Phase 2: Code Review Report

**Reviewed:** 2026-04-11T15:36:10Z
**Depth:** standard
**Files Reviewed:** 5
**Status:** issues_found

## Summary

Reviewed the Phase 2 lifecycle/config/daemon/CLI changes with focus on enrollment safety, boot recovery, and runtime ownership. The implementation is generally structured well, but I found one blocking safety issue in live apply handling plus several correctness/authentication problems that should be addressed before relying on this lifecycle path.

## Critical Issues

### CR-01: Newly applied config does not release fans removed from management

**File:** `crates/daemon/src/main.rs:417-445`
**Issue:** `apply_draft()` claims fans that passed validation, but it never releases fans that were previously owned and are no longer present in the new applied config. After an unenroll or a narrowed apply, those stale fan IDs stay in `OwnedFanSet`, so runtime state can remain inconsistent and shutdown fallback can still write to fans that are no longer explicitly enrolled. That violates the project safety requirement that BIOS-managed fans remain untouched unless enrolled.
**Fix:** Reconcile ownership against the new `applied.fans` set before or after claiming new fans. For fans being removed, write targeted fallback if needed, then `release_fan()` them.

```rust
let new_ids: std::collections::HashSet<_> = applied.fans.keys().cloned().collect();
for old_id in owned.owned_fan_ids().map(str::to_string).collect::<Vec<_>>() {
    if !new_ids.contains(&old_id) {
        let _ = write_fallback_single(&old_id, &owned);
        owned.release_fan(&old_id);
    }
}
```

## Warnings

### WR-01: Boot reconciliation leaves stale applied config on disk when nothing restores

**File:** `crates/daemon/src/main.rs:695-708`
**Issue:** The daemon persists the reconciled applied config only when `result.restored` is non-empty. If every previously managed fan is now missing/invalid, the daemon keeps the old applied config on disk instead of saving the empty reconciled subset. That makes degraded entries recur on every restart and leaves persisted state out of sync with actual runtime ownership.
**Fix:** Persist the reconciled config whenever reconciliation changes the applied set, including the fully empty case.

### WR-02: Friendly-name write methods bypass the daemon authorization boundary

**File:** `crates/daemon/src/main.rs:89-135`
**Issue:** `set_sensor_name`, `set_fan_name`, `remove_sensor_name`, and `remove_fan_name` modify daemon-owned persistent state without calling `require_authorized()`. On the system bus, any local caller that can reach the service can mutate shared config, which is an authorization gap even if it does not directly change fan speeds.
**Fix:** Apply the same caller authorization check used by lifecycle mutators, or explicitly document and enforce a narrower policy if friendly naming is intended to be multi-user.

### WR-03: Successful boot recovery is recorded as synthetic `FanMissing` events

**File:** `crates/core/src/lifecycle.rs:613-625,668-679`
**Issue:** `perform_boot_reconciliation()` encodes successful restore events with `DegradedReason::FanMissing` and synthetic fan IDs like `__restored__...`. Consumers that format events by reason kind (including the CLI) will report successful recovery as “fan missing from hardware,” which makes lifecycle history misleading.
**Fix:** Add a distinct non-degraded lifecycle event/reason for restoration/success, or keep these as detail-only log entries instead of overloading degraded reasons.

---

_Reviewed: 2026-04-11T15:36:10Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
