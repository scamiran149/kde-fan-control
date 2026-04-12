---
phase: 02-safe-enrollment-lifecycle-recovery
verified: 2026-04-11T16:18:02Z
status: human_needed
score: 5/5 must-haves verified
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 3/5
  gaps_closed:
    - "User can choose the control mode used for an enrolled fan when hardware exposes multiple safe options such as PWM or voltage."
    - "If the daemon exits unexpectedly, previously daemon-controlled fans move to safe high speed, unmanaged fans remain untouched, and the fallback state is inspectable."
  gaps_remaining: []
  regressions: []
deferred:
  - truth: "DBus exposes the full eventual fan-control configuration surface, not only lifecycle enrollment and apply flows."
    addressed_in: "Phase 3"
    evidence: "Phase 3 success criteria 1, 2, and 5 add sensor-group assignment, aggregation, target temperature, PID gains, live runtime inspection, and DBus-backed CLI flows."
  - truth: "Persisted configuration stores aggregation settings, target temperature, and PID parameters in addition to the Phase 2 lifecycle fields."
    addressed_in: "Phase 3"
    evidence: "Phase 3 success criteria 1 and 2 cover aggregation choice, target temperature, and PID gains."
  - truth: "CLI can configure aggregation and PID settings in addition to enrollment, mode choice, and apply flows."
    addressed_in: "Phase 3"
    evidence: "Phase 3 success criteria 1, 2, and 5 extend the CLI into full runtime control and tuning operations."
human_verification:
  - test: "Boot with at least one managed fan and one unmanaged fan, then restart the daemon or reboot the machine."
    expected: "Previously managed fans resume under daemon ownership, unmanaged fans remain untouched, and any mismatched fan is shown as degraded with a reason."
    why_human: "Requires real hwmon hardware, writable sysfs nodes, and actual reboot/service lifecycle behavior."
  - test: "Trigger an in-process daemon failure on a safe test system (for example a controlled panic) and restart the daemon."
    expected: "Owned fans are driven toward safe maximum, unmanaged fans are not touched, and fallback remains visible through lifecycle state/events after restart."
    why_human: "Failure-path hardware writes and post-restart observability cannot be fully verified without a live daemon, bus, and hardware."
  - test: "Call lifecycle DBus write methods once as non-root and once as root on the real system bus."
    expected: "Non-root writes are denied cleanly; privileged writes succeed and are reflected in draft/applied state."
    why_human: "Authorization behavior depends on the live DBus caller identity and deployment environment."
---

# Phase 2: Safe Enrollment & Lifecycle Recovery Verification Report

**Phase Goal:** Users can choose which fans the daemon owns, persist one authoritative configuration, and trust safe behavior across boot and daemon failure.
**Verified:** 2026-04-11T16:18:02Z
**Status:** human_needed
**Re-verification:** Yes — after gap closure

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | User can leave any detected fan unmanaged under BIOS or existing system control, or enroll a safely supported fan for daemon management, while unsafe hardware is refused. | ✓ VERIFIED | `validate_draft()` skips unmanaged entries and rejects non-`Available` fans (`crates/core/src/config.rs:408-494`); runtime ownership only claims restored/applied fans (`crates/core/src/lifecycle.rs:615-643`); CLI stages managed or unmanaged entries through DBus (`crates/cli/src/main.rs:227-255`). |
| 2 | User can choose the control mode used for an enrolled fan when hardware exposes multiple safe options such as PWM or voltage. | ✓ VERIFIED | Inventory now detects `Voltage` when writable `pwmN_mode` exists (`crates/core/src/inventory.rs:177-197`), regression tests cover PWM+Voltage, PWM-only, and non-writable selector cases (`crates/core/src/inventory.rs:307-400`), daemon accepts `voltage` (`crates/daemon/src/main.rs:520-528`), and CLI inventory output renders discovered modes (`crates/cli/src/main.rs:915-947`). |
| 3 | User can create and update the single active daemon-owned configuration over DBus-backed CLI flows, and the persisted configuration survives reboot. | ✓ VERIFIED | `AppConfig` remains the single persisted authority with `draft`, `applied`, and `fallback_incident` fields (`crates/core/src/config.rs:14-45`); lifecycle DBus writes save through the daemon boundary (`crates/daemon/src/main.rs:259-485`); CLI commands remain thin DBus clients (`crates/cli/src/main.rs:187-290`). |
| 4 | After reboot, previously managed fans resume safely from persisted configuration or surface a degraded state instead of silently claiming success. | ✓ VERIFIED | Startup loads persisted config, rebuilds persisted fallback markers, runs boot reconciliation, and persists the reconciled subset (`crates/daemon/src/main.rs:752-853`); reconciliation restores only safe matches and records degraded reasons/events for skipped fans (`crates/core/src/lifecycle.rs:576-720`). |
| 5 | If the daemon exits unexpectedly, previously daemon-controlled fans move to safe high speed, unmanaged fans remain untouched, and the fallback state is inspectable. | ✓ VERIFIED | Fallback is centralized in `record_fallback_incident_for_owned()` and writes only `OwnedFanSet` members (`crates/daemon/src/main.rs:574-603`, `crates/core/src/lifecycle.rs:376-443`); graceful shutdown and panic-path both call the shared recorder (`crates/daemon/src/main.rs:606-690`, `887-916`); fallback incidents persist in config and are reloaded into runtime/events on startup (`crates/core/src/config.rs:39-45, 139-163, 193-200, 839-869`; `crates/daemon/src/main.rs:763-800`). |

**Score:** 5/5 truths verified

### Deferred Items

Items not yet met but explicitly addressed in later milestone phases.

| # | Item | Addressed In | Evidence |
|---|---|---|---|
| 1 | Full DBus fan-control configuration beyond lifecycle enrollment/apply | Phase 3 | Success criteria 1, 2, and 5 |
| 2 | Persisted aggregation, target temperature, and PID parameters | Phase 3 | Success criteria 1 and 2 |
| 3 | CLI configuration of aggregation and PID settings | Phase 3 | Success criteria 1, 2, and 5 |

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/src/config.rs` | Single daemon-owned config with draft/applied persistence, validation, and durable fallback incident model | ✓ VERIFIED | Stores exactly one authoritative config shape, validates draft entries, persists fallback incidents, and has round-trip tests including persisted fallback (`1-45`, `408-531`, `839-869`). |
| `crates/core/src/inventory.rs` | Live multi-mode discovery for safe control modes | ✓ VERIFIED | `detect_control_modes()` now surfaces `Voltage` only when `pwmN_mode` is writable and tests prove downstream snapshot round-trip (`129-197`, `307-400`). |
| `crates/core/src/lifecycle.rs` | Boot reconciliation, owned-fan authority, fallback writer, runtime-state reconstruction | ✓ VERIFIED | Restores only validated fans, writes fallback for owned fans only, and rebuilds fallback runtime status after restart (`335-443`, `509-564`, `576-720`, `1254-1332`). |
| `crates/daemon/src/main.rs` | DBus lifecycle/auth boundary, startup reconciliation, graceful + panic fallback persistence wiring | ✓ VERIFIED | Lifecycle interface is wired, panic hook installed, persisted fallback is reloaded on startup, and shutdown still drives fallback (`177-496`, `574-690`, `752-916`). |
| `crates/cli/src/main.rs` | Thin DBus-backed lifecycle CLI and readable state/inventory output | ✓ VERIFIED | Lifecycle commands remain DBus-backed, `state`/`events`/`degraded` are inspectable, and inventory printing exposes discovered modes (`115-132`, `187-290`, `480-859`, `915-947`). |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| `crates/core/src/inventory.rs` | `crates/daemon/src/main.rs` | Inventory snapshot `fan.control_modes` | WIRED | Inventory discovery populates `control_modes`; daemon snapshot DBus method serializes the shared snapshot shape unchanged (`inventory.rs:177-197`; `daemon/main.rs:83-87`). |
| `crates/core/src/inventory.rs` | `crates/cli/src/main.rs` | Inventory/state output and enrollment expectations | WIRED | CLI inventory display prints the discovered `control_modes` list, including `voltage` (`cli/main.rs:922-933`). |
| `crates/cli/src/main.rs` | `org.kde.FanControl.Lifecycle` | `LifecycleProxy` methods | WIRED | CLI commands call lifecycle proxy methods for draft, apply, degraded, events, and runtime state (`cli/main.rs:115-132`, `187-290`). |
| `crates/daemon/src/main.rs` | `crates/core/src/config.rs` | `apply_draft()`, `set_applied()`, `save()`, fallback incident save/load | WIRED | Apply persists authoritative config; fallback recorder persists `fallback_incident`; startup reloads it (`daemon/main.rs:372-457`, `763-800`). |
| `crates/daemon/src/main.rs` | `crates/core/src/lifecycle.rs` | Boot reconciliation | WIRED | Daemon calls `perform_boot_reconciliation()` at startup and claims ownership from the result (`daemon/main.rs:803-853`). |
| `crates/daemon/src/main.rs` | `crates/core/src/lifecycle.rs` | Shared fallback handler invocation | WIRED | Both `run_fallback_recorder()` and `run_panic_fallback_recorder()` invoke `record_fallback_incident_for_owned()` / `write_fallback_for_owned()` (`daemon/main.rs:574-690`, `894-900`). |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| `crates/core/src/inventory.rs` | `control_modes` | Writable `pwmN` + optional writable `pwmN_mode` sysfs nodes | Yes | ✓ FLOWING |
| `crates/daemon/src/main.rs` | `fallback_fan_ids` | Persisted `config.fallback_incident` loaded at startup, then rebuilt by shared fallback recorder | Yes | ✓ FLOWING |
| `crates/cli/src/main.rs` | Rendered runtime/inventory mode and fallback text | DBus JSON from daemon lifecycle/inventory methods | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Workspace implementation compiles and tests | `cargo test --workspace` | 41 tests passed, 0 failed | ✓ PASS |
| CLI exposes lifecycle management and inspection commands | `cargo run -q -p kde-fan-control-cli -- --help` | Help lists `draft`, `applied`, `degraded`, `events`, `enroll`, `unenroll`, `discard`, `validate`, `apply`, `state` | ✓ PASS |
| Daemon binary remains runnable with lifecycle surface | `cargo run -q -p kde-fan-control-daemon -- --help` | Help renders daemon options successfully | ✓ PASS |
| Real boot/failure hardware behavior | Not run | Requires live daemon + DBus + writable hwmon hardware; skipped without side effects | ? SKIP |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| FAN-01 | 02-01, 02-04 | Leave detected fan unmanaged without daemon interference | ✓ SATISFIED | Unmanaged draft entries are not promoted and non-owned fans stay `Unmanaged` (`config.rs:412-416`; `lifecycle.rs:526-556`). |
| FAN-02 | 02-01, 02-04 | Enroll safely supported fan for daemon management | ✓ SATISFIED | Available fans with supported mode validate and are promoted/applied (`config.rs:429-487`, `503-531`; `daemon/main.rs:417-455`). |
| FAN-03 | 02-01, 02-04 | Refuse unsafe hardware enrollment | ✓ SATISFIED | Validation rejects non-`Available` fans with explicit reasons (`config.rs:429-443`). |
| FAN-04 | 02-04, 02-05 | Choose hardware control mode when multiple modes exist | ✓ SATISFIED | Inventory now discovers `Voltage` alongside `Pwm` where safe, and CLI/daemon consume the shared mode list (`inventory.rs:177-197`, `307-400`; `daemon/main.rs:520-528`; `cli/main.rs:922-933`). |
| FAN-05 | 02-01, 02-04, 02-06 | View whether a fan is unmanaged, managed, fallback, partial, or unavailable | ✓ SATISFIED | Inventory output shows support state and reasons; runtime state exposes unmanaged/managed/degraded/fallback; persisted fallback survives restart (`cli/main.rs:768-859`, `915-947`; `daemon/main.rs:223-249`, `763-800`). |
| FAN-06 | 02-03 | Managed fans resume after reboot from persisted config | ✓ SATISFIED | Boot reconciliation restores validated applied fans and persists reconciled config (`lifecycle.rs:576-720`; `daemon/main.rs:803-853`). |
| SAFE-01 | 02-03, 02-06 | Unexpected daemon failure drives controlled fans to high speed | ✓ SATISFIED | Shared fallback recorder is used for ctrl-c and panic-path failure handling; writes target owned fans only (`daemon/main.rs:574-690`, `887-916`; `lifecycle.rs:388-443`). |
| SAFE-02 | 02-03 | Unmanaged fans remain untouched in normal/startup/shutdown/failure paths | ✓ SATISFIED | Ownership is explicit and fallback iterates only the owned set (`lifecycle.rs:386-443`, `576-643`). |
| SAFE-03 | 02-03, 02-04, 02-06 | User can inspect current safe fallback state | ✓ SATISFIED | Fallback incidents persist in config, repopulate runtime fallback IDs on startup, and are visible through `state` and `events` (`config.rs:39-45`, `139-163`; `daemon/main.rs:763-800`; `cli/main.rs:603-645`, `768-859`). |
| SAFE-05 | 02-03 | Writable control without tach can still be enrolled | ✓ SATISFIED | Support classification depends on writable control node, not tach feedback (`inventory.rs:135-162`). |
| SAFE-06 | 02-03 | Safe-maximum fallback does not depend on tach | ✓ SATISFIED | Fallback writes only use recorded sysfs PWM paths and never read RPM/tach (`lifecycle.rs:388-443`). |
| SAFE-07 | 02-03 | Unsafe startup restore surfaces degraded state | ✓ SATISFIED | Reconciliation records degraded reasons and lifecycle events for skipped fans (`lifecycle.rs:646-704`). |
| BUS-02 | 02-02 | Create, update, and delete fan-control configuration over DBus | ↷ DEFERRED | Phase 2 covers lifecycle enrollment/apply CRUD; Phase 3 success criteria 1, 2, and 5 extend the DBus contract to the full control surface. |
| BUS-04 | 02-01, 02-02 | Trigger persistence of configuration through the daemon over DBus | ✓ SATISFIED | Lifecycle mutation paths save daemon-owned config after writes (`daemon/main.rs:279-285`, `307-313`, `332-337`, `449-455`). |
| BUS-06 | 02-02 | DBus clients observe daemon as sole authority | ✓ SATISFIED | Draft/applied/degraded/events/runtime state all flow through daemon-owned DBus interfaces (`daemon/main.rs:177-496`). |
| CONF-01 | 02-01, 02-02 | Persist exactly one active configuration in v1 | ✓ SATISFIED | `AppConfig` contains one `applied: Option<AppliedConfig>` authority (`config.rs:20-45`). |
| CONF-02 | 02-01, 02-02 | Persist friendly names, enrollment, sensor inputs, aggregation, control mode, target temperature, and PID params | ↷ DEFERRED | Phase 2 persists friendly names, enrollment, temp sources, control mode, and fallback state now; Phase 3 success criteria 1 and 2 add aggregation, target temperature, and PID gains. |
| CONF-03 | 02-01, 02-03 | Validate persisted config against current hardware before boot resume | ✓ SATISFIED | Boot reconciliation revalidates existence, enrollability, mode support, and temp sources before claiming ownership (`lifecycle.rs:576-720`). |
| CLI-02 | 02-02, 02-04 | Configure fan-control settings from CLI | ↷ DEFERRED | Phase 2 CLI covers enrollment, mode choice, temp sources, validate/apply/discard, and recovery inspection now; Phase 3 success criteria 1, 2, and 5 add aggregation and PID controls. |

**Orphaned requirements:** None. All Phase 2 requirement IDs in `.planning/REQUIREMENTS.md` appear in at least one Phase 2 plan.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| — | — | No blocking anti-patterns found in scanned phase artifacts | ℹ️ Info | Placeholder comments, empty implementations, and stub-style returns were not found in the verified crate sources. |

### Human Verification Required

### 1. Boot Reconciliation On Real Hardware

**Test:** Enroll at least one safe fan, leave another unmanaged, reboot or restart the daemon, then inspect `state`, `degraded`, and `events`.
**Expected:** Managed fans resume, unmanaged fans remain untouched, and any mismatched fan is shown as degraded with a clear reason.
**Why human:** Requires real hardware, writable hwmon nodes, and actual service lifecycle behavior.

### 2. Failure-Path Fallback Persistence

**Test:** On a safe test machine, trigger a controlled in-process daemon failure (for example a panic in a non-production run), restart the daemon, and inspect `state` and `events`.
**Expected:** Owned fans are driven toward safe maximum, unmanaged fans are untouched, and fallback remains visible after restart.
**Why human:** Programmatic verification here would require side-effecting hardware writes and a live DBus/system-service environment.

### 3. DBus Authorization Boundary

**Test:** Invoke lifecycle write methods once as a non-root user and once as root.
**Expected:** Non-root receives access denied; root can change draft/apply state successfully.
**Why human:** Authorization depends on live DBus caller identity and deployment context, not just code structure.

### Gaps Summary

The two previously failing Phase 2 gaps are closed: multi-mode control discovery now exposes real PWM/voltage choices from inventory, and failure-path fallback is now centralized, persisted, and rehydrated after restart. Automated verification now finds the Phase 2 must-haves satisfied with no regressions.

Remaining follow-up is limited to two non-blocking categories: broader control-surface work explicitly deferred to Phase 3, and live-system validation that can only be confirmed on real hardware and DBus/service infrastructure.

---

_Verified: 2026-04-11T16:18:02Z_
_Verifier: the agent (gsd-verifier)_
