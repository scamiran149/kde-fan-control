---
phase: 02-safe-enrollment-lifecycle-recovery
verified: 2026-04-11T15:39:04Z
status: gaps_found
score: 3/5 must-haves verified
overrides_applied: 0
gaps:
  - truth: "User can choose the control mode used for an enrolled fan when hardware exposes multiple safe options such as PWM or voltage."
    status: failed
    reason: "The lifecycle/config layers accept `voltage`, but live hardware discovery only exposes PWM control modes, so users cannot actually choose among multiple discovered safe modes."
    artifacts:
      - path: "crates/core/src/inventory.rs"
        issue: "`build_fan_channel()` only populates `control_modes` with `ControlMode::Pwm` when `pwm` is writable; no voltage or multi-mode discovery exists (lines 129-178, especially 139-143)."
      - path: "crates/cli/src/main.rs"
        issue: "CLI accepts `--control-mode voltage`, but that path depends on inventory surfacing voltage support first."
    missing:
      - "Detect and surface non-PWM safe control modes from live hardware inventory."
      - "Populate multi-mode `control_modes` from discovery so DBus/CLI selection reflects real hardware capabilities."
  - truth: "If the daemon exits unexpectedly, previously daemon-controlled fans move to safe high speed, unmanaged fans remain untouched, and the fallback state is inspectable."
    status: failed
    reason: "Fallback writes are only wired to the graceful `ctrl_c` shutdown path; there is no crash/unexpected-exit handler, and fallback state is marked only immediately before process exit, making it non-inspectable after failure."
    artifacts:
      - path: "crates/daemon/src/main.rs"
        issue: "Fallback is invoked only after `tokio::signal::ctrl_c().await` and the process exits immediately afterward (lines 743-779)."
      - path: "crates/core/src/lifecycle.rs"
        issue: "Fallback helpers exist, but no daemon wiring covers panic/crash/unexpected-exit paths."
    missing:
      - "Add unexpected-exit/failure-path fallback wiring, not just graceful Ctrl-C shutdown."
      - "Persist or otherwise expose fallback state so it remains inspectable after daemon failure/restart."
deferred:
  - truth: "CLI can configure full fan-control settings beyond lifecycle enrollment, including aggregation and PID settings."
    addressed_in: "Phase 3"
    evidence: "Phase 3 success criteria 1, 2, and 5 cover sensor groups, aggregation choice, target temperature/PID gains, and DBus-backed CLI flows."
  - truth: "Persisted configuration stores aggregation settings, target temperature, and PID parameters in addition to lifecycle enrollment data."
    addressed_in: "Phase 3"
    evidence: "Phase 3 success criteria 1 and 2 cover sensor assignment, aggregation, target temperature, and PID gains."
  - truth: "DBus exposes full fan-control configuration, not just lifecycle enrollment state."
    addressed_in: "Phase 3"
    evidence: "Phase 3 success criteria 1, 2, and 5 extend DBus-backed configuration into sensor inputs, PID settings, live status, and auto-tuning."
---

# Phase 2: Safe Enrollment & Lifecycle Recovery Verification Report

**Phase Goal:** Users can choose which fans the daemon owns, persist one authoritative configuration, and trust safe behavior across boot and daemon failure.
**Verified:** 2026-04-11T15:39:04Z
**Status:** gaps_found
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
| --- | --- | --- | --- |
| 1 | User can leave any detected fan unmanaged under BIOS or existing system control, or enroll a safely supported fan for daemon management, while unsafe hardware is refused. | ✓ VERIFIED | `validate_draft()` skips unmanaged entries and rejects non-`Available` fans (`crates/core/src/config.rs:358-439`); CLI stages managed/unmanaged draft entries via DBus (`crates/cli/src/main.rs:227-247`); runtime state defaults inventory fans to `Unmanaged` then upgrades only owned fans (`crates/core/src/lifecycle.rs:485-515`). |
| 2 | User can choose the control mode used for an enrolled fan when hardware exposes multiple safe options such as PWM or voltage. | ✗ FAILED | CLI/DBus parse `voltage` (`crates/cli/src/main.rs:66-71`, `crates/daemon/src/main.rs:518-526`), but inventory discovery only ever surfaces PWM in `control_modes` (`crates/core/src/inventory.rs:139-143`). |
| 3 | User can create and update the single active daemon-owned configuration over DBus-backed CLI flows, and the persisted configuration survives reboot. | ✓ VERIFIED | `AppConfig` persists one versioned config with distinct `draft` and `applied` layers (`crates/core/src/config.rs:14-38, 126-157`); daemon write methods save through the daemon boundary (`crates/daemon/src/main.rs:259-295, 364-485`); CLI lifecycle commands are thin DBus clients (`crates/cli/src/main.rs:187-290`). |
| 4 | After reboot, previously managed fans resume safely from persisted configuration or surface a degraded state instead of silently claiming success. | ✓ VERIFIED | Boot reconciliation validates persisted applied fans against live inventory and builds degraded reasons for mismatches (`crates/core/src/lifecycle.rs:83-200, 535-683`); daemon runs reconciliation at startup and persists the reconciled subset (`crates/daemon/src/main.rs:659-708`). |
| 5 | If the daemon exits unexpectedly, previously daemon-controlled fans move to safe high speed, unmanaged fans remain untouched, and the fallback state is inspectable. | ✗ FAILED | Only graceful `ctrl_c` shutdown triggers `write_fallback_for_owned()` (`crates/daemon/src/main.rs:743-779`); no unexpected-exit/crash hook was found; fallback IDs are populated only immediately before exit, so users cannot inspect fallback after failure. |

**Score:** 3/5 truths verified

### Deferred Items

Items not yet met but explicitly addressed in later milestone phases.

| # | Item | Addressed In | Evidence |
|---|---|---|---|
| 1 | CLI can configure aggregation and PID settings, not just lifecycle enrollment | Phase 3 | Success criteria 1, 2, and 5 |
| 2 | Persisted config stores aggregation, target temperature, and PID parameters | Phase 3 | Success criteria 1 and 2 |
| 3 | DBus exposes full fan-control configuration, not just lifecycle enrollment | Phase 3 | Success criteria 1, 2, and 5 |

### Required Artifacts

| Artifact | Expected | Status | Details |
| --- | --- | --- | --- |
| `crates/core/src/config.rs` | Versioned daemon-owned draft/applied config, validation, degraded/event models | ✓ VERIFIED | 991 lines; substantive persistence and validation logic with 14 config tests. |
| `crates/core/src/lifecycle.rs` | Reconciliation, owned-fan tracking, fallback writes, runtime state | ⚠️ PARTIAL | Substantive and wired, but unexpected-exit fallback is not wired; event stream also encodes successful restore events as fake `FanMissing` reasons (`621`, `673`). |
| `crates/daemon/src/main.rs` | DBus lifecycle/auth boundary, boot reconciliation, fallback wiring | ⚠️ PARTIAL | Lifecycle DBus surface and boot reconciliation are wired; fallback is wired only to graceful Ctrl-C shutdown (`743-779`). |
| `crates/cli/src/main.rs` | Thin DBus-backed lifecycle CLI and inspectable state output | ✓ VERIFIED | CLI proxies and lifecycle subcommands are wired to DBus (`97-132`, `187-290`, `768-864`). |
| `crates/core/src/inventory.rs` | Live hardware capability exposure including selectable control modes | ✗ PARTIAL | Discovery exposes support state, tach, and PWM, but no real voltage/multi-mode detection (`129-178`). |

### Key Link Verification

| From | To | Via | Status | Details |
| ---- | --- | --- | ------ | ------- |
| `crates/cli/src/main.rs` | `org.kde.FanControl.Lifecycle` | `LifecycleProxy` methods | WIRED | CLI lifecycle commands call DBus proxy methods for draft/apply/degraded/events/state (`116-131`, `187-290`). |
| `crates/daemon/src/main.rs` | `crates/core/src/config.rs` | `apply_draft()` + `config.set_applied()` + `save()` | WIRED | Apply path validates, promotes, persists, and signals (`364-485`). |
| `crates/daemon/src/main.rs` | `crates/core/src/lifecycle.rs` | `perform_boot_reconciliation()` | WIRED | Startup runs reconciliation and persists reconciled applied config (`659-708`). |
| `crates/daemon/src/main.rs` | `crates/core/src/lifecycle.rs` fallback writer | `tokio::signal::ctrl_c()` | PARTIAL | Graceful shutdown path exists, but no crash/unexpected-exit wiring was found (`743-779`). |
| `crates/core/src/inventory.rs` | Lifecycle mode selection | `FanChannel.control_modes` | NOT_WIRED | Discovery never populates multi-mode/voltage options, so mode selection cannot reflect real alternate hardware modes. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
| -------- | ------------- | ------ | ------------------ | ------ |
| `crates/cli/src/main.rs` | `json` in `Applied` command | `LifecycleProxy::get_applied_config()` → daemon serializes `config.applied` | Yes | ✓ FLOWING |
| `crates/cli/src/main.rs` | `json` in `State` command | `LifecycleProxy::get_runtime_state()` → daemon `RuntimeState::build()` from `owned`, `degraded`, `fallback_fan_ids`, `snapshot` | Partially | ⚠️ STATIC for fallback: `fallback_fan_ids` is only populated in the Ctrl-C shutdown path immediately before exit |
| `crates/daemon/src/main.rs` | `config.applied` at startup | `AppConfig::load()` → `perform_boot_reconciliation()` | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
| -------- | ------- | ------ | ------ |
| Workspace implementation compiles and tests | `cargo test --workspace` | 31 core tests passed, 0 failed | ✓ PASS |
| CLI exposes lifecycle management commands | `cargo run -q -p kde-fan-control-cli -- --help` | Help lists `draft`, `applied`, `degraded`, `events`, `enroll`, `unenroll`, `discard`, `validate`, `apply`, `state` | ✓ PASS |
| Daemon exposes runnable lifecycle service binary | `cargo run -q -p kde-fan-control-daemon -- --help` | Help renders daemon options | ✓ PASS |
| Real DBus lifecycle mutation / boot restore / fallback behavior | Not run | Requires live daemon + bus + hardware/sysfs interactions | ? SKIP |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
| ----------- | ---------- | ----------- | ------ | -------- |
| FAN-01 | 02-01, 02-04 | Leave detected fan unmanaged without daemon interference | ✓ SATISFIED | Unmanaged draft entries are skipped by validation; runtime state shows unmanaged fans (`config.rs:358-362`, `lifecycle.rs:485-515`). |
| FAN-02 | 02-01, 02-04 | Enroll safely supported fan for daemon management | ✓ SATISFIED | Available fans with supported mode validate and are promoted/applied (`config.rs:375-439, 449-478`). |
| FAN-03 | 02-01, 02-04 | Refuse unsafe hardware enrollment | ✓ SATISFIED | Non-`Available` fans are rejected with explicit reasons (`config.rs:375-389`). |
| FAN-04 | 02-04 | Choose hardware control mode when multiple modes exist | ✗ BLOCKED | Selection surface exists, but discovery does not expose real multi-mode hardware options (`inventory.rs:139-143`). |
| FAN-05 | 02-03, 02-04 | View unmanaged/managed/fallback/partial/unavailable state | ✗ BLOCKED | Partial/unavailable are visible in inventory and managed/unmanaged in runtime state, but fallback is not inspectable after failure because fallback state is only set during exit (`daemon/main.rs:743-779`). |
| FAN-06 | 02-03 | Managed fans resume after reboot from persisted config | ✓ SATISFIED | Startup reconciliation restores valid applied fans and persists reconciled subset (`lifecycle.rs:535-683`, `daemon/main.rs:659-708`). |
| SAFE-01 | 02-03 | Unexpected daemon failure drives controlled fans to high speed | ✗ BLOCKED | Only graceful Ctrl-C path calls fallback writer; no crash/unexpected-exit handler found. |
| SAFE-02 | 02-03 | Unmanaged fans remain untouched in normal/startup/shutdown/failure paths | ✓ SATISFIED | Fallback writes iterate only `OwnedFanSet`; unmanaged fans are never claimed (`lifecycle.rs:227-306, 335-402`). |
| SAFE-03 | 02-03, 02-04 | User can inspect current safe fallback state | ✗ BLOCKED | Runtime model has `Fallback`, but daemon marks it only during shutdown just before exit (`daemon/main.rs:767-776`). |
| SAFE-05 | 02-03 | Writable control without tach can still be enrolled | ✓ SATISFIED | `support_state` becomes `Available` based on writable control, not tach presence (`inventory.rs:134-166`). |
| SAFE-06 | 02-03 | Safe-maximum fallback does not depend on tach | ✓ SATISFIED | Fallback writes use owned sysfs PWM paths only; no RPM/tach dependency in writer (`lifecycle.rs:335-433`). |
| SAFE-07 | 02-03 | Unsafe startup restore surfaces degraded state | ✓ SATISFIED | Reconciliation records degraded reasons and events for skipped fans (`lifecycle.rs:89-199, 605-666`). |
| BUS-02 | 02-02 | Create/update/delete fan-control config over DBus | ↷ DEFERRED | Lifecycle config CRUD exists now; broader control config is covered by Phase 3 success criteria 1, 2, and 5. |
| BUS-04 | 02-01, 02-02 | Trigger persistence of configuration through daemon over DBus | ✓ SATISFIED | Lifecycle write methods save daemon-owned config after mutations (`daemon/main.rs:280-285, 308-313, 332-337, 449-455`). |
| BUS-06 | 02-02 | Daemon is sole authority for persistence/runtime state | ✓ SATISFIED | Lifecycle mutations, applied config, degraded state, events, and runtime state all flow through daemon DBus methods (`daemon/main.rs:193-250, 259-485`). |
| CONF-01 | 02-01, 02-02 | Persist exactly one active configuration in v1 | ✓ SATISFIED | `AppConfig` has one `applied: Option<AppliedConfig>` authoritative state (`config.rs:20-38`). |
| CONF-02 | 02-01, 02-02 | Persist friendly names, enrollment, sensor inputs, aggregation, control mode, target temperature, PID params | ↷ DEFERRED | Friendly names, enrollment, control mode, and temp sources exist now; aggregation/target/PID are not yet present and are covered by Phase 3 success criteria 1 and 2. |
| CONF-03 | 02-01, 02-03 | Validate persisted config against current hardware before boot resume | ✓ SATISFIED | Boot reconciliation checks fan existence, enrollability, control mode, and temp sources before restore (`lifecycle.rs:83-200`). |
| CLI-02 | 02-02, 02-04 | Configure fan lifecycle/control settings from CLI | ↷ DEFERRED | Lifecycle enrollment/apply/degraded inspection exist now; aggregation and PID configuration are deferred to Phase 3 success criteria 1, 2, and 5. |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
| ---- | ---- | ------- | -------- | ------ |
| `crates/core/src/lifecycle.rs` | 621 | Successful boot restore event encoded as fake `DegradedReason::FanMissing` with `__restored__...` sentinel ID | ⚠️ Warning | CLI event output will describe a successful restore as a missing-fan event, making lifecycle history misleading. |
| `crates/core/src/lifecycle.rs` | 673 | Full boot success event encoded as fake `DegradedReason::FanMissing` with `__boot_reconciled__...` sentinel ID | ⚠️ Warning | Event history misreports success, undermining diagnosability/trust. |

### Gaps Summary

Phase 2 delivered the core lifecycle scaffolding: daemon-owned draft/applied persistence, DBus-backed CLI flows, boot reconciliation, owned-fan tracking, degraded-state tracking, and substantive unit coverage. The phase goal is **not fully achieved**, though, because two critical user-facing promises are still broken.

First, control-mode choice is only nominal. The CLI and DBus layers accept `voltage`, but the live inventory layer never discovers or exposes real voltage/multi-mode fan capabilities, so users cannot actually choose among multiple supported modes on real hardware.

Second, safety on daemon failure is incomplete. The code can drive owned fans to safe maximum during a graceful Ctrl-C shutdown, but there is no wiring for unexpected exit/crash handling, and fallback state is only marked immediately before the daemon exits. That means users cannot yet trust the promised fail-safe behavior across daemon failure, nor inspect fallback after it happens.

Separately, some broader requirement text (full aggregation/PID configuration over DBus/CLI and in persisted config) is only partially implemented in this phase. Those gaps are explicitly covered by Phase 3 roadmap success criteria and are therefore reported as deferred rather than blocking this phase verdict.

---

_Verified: 2026-04-11T15:39:04Z_
_Verifier: the agent (gsd-verifier)_
