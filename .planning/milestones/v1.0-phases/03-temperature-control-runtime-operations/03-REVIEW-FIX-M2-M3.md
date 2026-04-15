---
phase: 03
fixed_at: 2026-04-14T18:37:00-05:00
review_path: ad-hoc fix (no REVIEW.md)
iteration: 1
findings_in_scope: 2
fixed: 2
skipped: 0
status: all_fixed
---

# M2 & M3 Security Fix Report

**Fixed at:** 2026-04-14T18:37:00-05:00
**Source:** User-requested fixes for M2 and M3

**Summary:**
- Findings in scope: 2
- Fixed: 2
- Skipped: 0

## Fixed Issues

### M2: Polkit start-time=0 weakens process identity

**Files modified:** `Cargo.toml`, `Cargo.lock`, `crates/daemon/Cargo.toml`, `crates/daemon/src/main.rs`
**Commit:** `09df313`

**Applied fix:** Added `get_process_start_time(pid)` function that reads `/proc/<pid>/stat`, extracts field 22 (starttime in clock ticks since boot), and converts to microseconds using `sysconf(_SC_CLK_TCK)`. The polkit subject dict's `start-time` field now uses this real value instead of the hardcoded `0u64`. If the start time cannot be determined, it falls back to `0` (matching polkit's behavior for missing data). Added `libc = "0.2"` as a workspace dependency and to the daemon crate.

### M3: PID fallback to 0 on lookup failure

**Files modified:** `crates/daemon/src/main.rs`
**Commit:** `09df313`

**Applied fix:** Replaced `unwrap_or(0)` on `get_connection_unix_process_id()` with proper error handling. If the DBus call fails, an `AccessDenied` error is returned with a descriptive message. If the returned PID is 0 (kernel process), an `AccessDenied` error is returned explicitly stating that kernel-process authorization is denied. This prevents an attacker from bypassing authorization by causing PID lookup failure or spoofing a kernel process.

---

_Fixed: 2026-04-14T18:37:00-05:00_
_Fixer: gsd-code-fixer_
_Iteration: 1_