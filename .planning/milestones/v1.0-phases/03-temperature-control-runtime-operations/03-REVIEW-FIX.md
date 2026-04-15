---
phase: 03-temperature-control-runtime-operations
fixed_at: 2026-04-15T00:05:00Z
review_path: .planning/milestones/v1.0-phases/03-temperature-control-runtime-operations/03-REVIEW.md
iteration: 3
findings_in_scope: 2
fixed: 2
skipped: 0
status: all_fixed
---

# Phase 03: Code Review Fix Report

**Fixed at:** 2026-04-15T00:05:00Z
**Source review:** .planning/milestones/v1.0-phases/03-temperature-control-runtime-operations/03-REVIEW.md
**Iteration:** 3

**Summary:**
- Findings in scope: 2
- Fixed: 2
- Skipped: 0

## Fixed Issues

### M5: `expect()` on auto-tune state

**Files modified:** `crates/daemon/src/main.rs`
**Commit:** e780fd9
**Applied fix:** Replaced two `.expect("state should exist")` calls in `ControlSupervisor::record_auto_tune_sample()` with a safe defensive pattern. The original code called `auto_tune.get_mut(fan_id).expect("state should exist")` twice, which could panic if the state was removed between the initial `if let` match and the subsequent mutation.

The fix restructures the method to:
1. Compute a local `obs_window: u64` copy from the pattern binding `observation_window_ms: &u64` before attempting re-mutation, releasing the borrow from the `if let` pattern.
2. Build the replacement `AutoTuneExecutionState` as a fully-owned value (using `obs_window` and the `proposal`/`error` from the result) inside the `if let` block, producing an `Option<AutoTuneExecutionState>` transition value.
3. Apply the transition outside the `if let` block using `if let Some(new_state) = transition { if let Some(state) = auto_tune.get_mut(fan_id) { *state = new_state; } }`, which gracefully handles the `None` case instead of panicking.

This eliminates two possible panic points in the daemon's auto-tune control loop while maintaining the same behavior when the state entry exists (which is the expected case).

### M6: `unwrap()` on poisoned RwLock in production

**Files modified:** `crates/daemon/src/main.rs`
**Commit:** e780fd9
**Applied fix:** Replaced `config.try_read().unwrap()` in the `panic_path_uses_same_fallback_recorder` test with `config.try_read().expect("config lock should be available after panic recorder completes")`. 

The original `.unwrap()` would panic without context if the Tokio `RwLock` was still held. Tokio's `RwLock` does not support poisoning (unlike `std::sync::RwLock`), so `into_inner()` recovery is not applicable — `TryLockError` simply means "would block" with no inner data. The `.expect()` with a descriptive message is the appropriate replacement for Tokio's `RwLock::try_read()` in test contexts, providing clear diagnostics if the lock is unexpectedly held.

All other `try_read()`/`try_write()` usage in production code (in `run_panic_fallback_recorder`) already uses the proper `let Ok(...) else { ... }` pattern with graceful degradation. No `lock().unwrap()` patterns were found in the file.

## Skipped Issues

None.

---

_Fixed: 2026-04-15T00:05:00Z_
_Fixer: the agent (gsd-code-fixer)_
_Iteration: 3_