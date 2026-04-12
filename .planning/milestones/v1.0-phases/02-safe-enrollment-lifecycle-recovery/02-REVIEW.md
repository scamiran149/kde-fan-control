---
phase: 02-safe-enrollment-lifecycle-recovery
reviewed: 2026-04-11T16:16:24Z
depth: standard
files_reviewed: 6
files_reviewed_list:
  - crates/core/src/config.rs
  - crates/core/src/inventory.rs
  - crates/core/src/lifecycle.rs
  - crates/core/src/lib.rs
  - crates/daemon/src/main.rs
  - crates/cli/src/main.rs
findings:
  critical: 1
  warning: 3
  info: 0
  total: 4
status: issues_found
---

# Phase 02: Code Review Report

**Reviewed:** 2026-04-11T16:16:24Z
**Depth:** standard
**Files Reviewed:** 6
**Status:** issues_found

## Summary

Reviewed the Phase 2 lifecycle/config/inventory/daemon/CLI changes with emphasis on enrollment safety and recovery behavior. The core validation/reconciliation logic is generally sound and the current test suite passes, but there is still one blocking ownership regression plus several correctness/auth gaps that should be fixed before considering Phase 2 clean.

## Critical Issues

### CR-01: Applying a smaller config leaves previously owned fans under daemon control

**File:** `crates/daemon/src/main.rs:417-446`
**Issue:** `apply_draft()` only claims newly enrollable fans; it never releases fans that were previously owned but are no longer present in the newly applied config. If an operator unenrolls a fan or marks it unmanaged, that fan can remain in `OwnedFanSet`, continue showing as managed in runtime state, and still receive fallback writes on shutdown/panic. That violates the project safety/compatibility rule that non-enrolled fans must remain untouched.
**Fix:** Rebuild ownership from the newly applied config instead of only appending to it. Release any owned fan not present in `applied.fans` before/while claiming the new set, and add an explicit handoff path if unenrollment should restore firmware/auto control.

```rust
let desired: std::collections::HashSet<_> = applied.fans.keys().cloned().collect();

for existing in owned.owned_fan_ids().map(str::to_string).collect::<Vec<_>>() {
    if !desired.contains(&existing) {
        owned.release_fan(&existing);
    }
}

for (fan_id, applied_entry) in &applied.fans {
    // resolve sysfs path and claim_fan(...)
}
```

## Warnings

### WR-01: Boot reconciliation does not persist an all-skipped reconciled config

**File:** `crates/daemon/src/main.rs:839-852`
**Issue:** After boot reconciliation, the daemon only persists `result.reconciled_config` when at least one fan was restored. If every previously applied fan is now unsafe/missing, the stale pre-reconciliation applied config stays on disk. That leaves persisted state claiming fans are managed when runtime ownership is empty, and the daemon will retry the stale config on every restart.
**Fix:** Persist the reconciled subset whenever it differs from the previous applied config, including the fully empty case. If no fans remain valid, clear or replace `config.applied` with the empty reconciled config.

### WR-02: CLI prefers the session bus even though the daemon is designed for the system bus

**File:** `crates/cli/src/main.rs:324-331`
**Issue:** `connect_dbus()` returns the first successfully opened bus connection, not the bus that actually hosts `org.kde.FanControl`. On a normal desktop, opening the session bus usually succeeds, so lifecycle commands can fail against the wrong bus instead of reaching the system service.
**Fix:** Try the system bus first for normal operation, or probe the target service before committing to a connection.

```rust
async fn connect_dbus() -> zbus::Result<zbus::Connection> {
    if let Ok(conn) = zbus::connection::Builder::system()?.build().await {
        return Ok(conn);
    }
    zbus::connection::Builder::session()?.build().await
}
```

### WR-03: Inventory write methods bypass the daemon's authorization boundary

**File:** `crates/daemon/src/main.rs:89-135`
**Issue:** `set_sensor_name`, `set_fan_name`, `remove_sensor_name`, and `remove_fan_name` mutate daemon-owned configuration but do not call `require_authorized()`. Any local caller that can reach the bus can persist naming changes, which weakens the stated privileged-write boundary.
**Fix:** Gate these mutating DBus methods with the same connection/header authorization check used by lifecycle writes.

```rust
async fn set_fan_name(
    &self,
    #[zbus(connection)] connection: &zbus::Connection,
    #[zbus(header)] header: zbus::message::Header<'_>,
    id: &str,
    name: &str,
) -> fdo::Result<()> {
    require_authorized(connection, &header).await?;
    // existing mutation logic
}
```

---

_Reviewed: 2026-04-11T16:16:24Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_
