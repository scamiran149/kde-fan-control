# Security Analysis

**KDE Fan Control** runs a root daemon that writes to hardware sysfs attributes
and exposes a DBus control surface. This document catalogs the security-relevant
attack surfaces, known findings, and the remediation plan.

---

## Threat Model

### Assets

| Asset | Compromise Impact |
|---|---|
| Fan hardware control (PWM writes) | Thermal damage, hardware destruction, acoustic DoS |
| Root file system (daemon writes as root) | Privilege escalation, system corruption |
| Daemon DBus control surface | Unauthorized fan reconfiguration, DoS |
| Config and state files | Config tampering, fan topology disclosure |

### Threat Actors

| Actor | Capability | Motivation |
|---|---|---|
| Local unprivileged user | DBus method calls (read-only without auth) | Information reconnaissance, DoS via rapid polling |
| Local admin-user process | DBus method calls (write with polkit cache) | Config manipulation, fan control hijacking |
| Compromised user process | Same as the user it runs as | Lateral movement, hardware sabotage |
| Malicious config file | Root-owned but world-readable | Redirect fallback binary writes via `owned-fans.json` tampering |
| Kernel / hardware-level attacker | Direct sysfs writes | Unmitigable by userspace; out of scope |

### Trust Boundaries

```
  ┌───────────┐      system bus       ┌──────────┐     sysfs      ┌───────┐
  │  CLI/GUI  │  ──── DBus ──────►   │  Daemon  │  ── writes ──► │ hwmon │
  │ (user)    │  polkit-gated writes  │  (root)  │               │ (root)│
  └───────────┘                      └─────┬────┘               └───────┘
                                            │
                                      ┌─────┴──────┐
                                      │ State files │  /var/lib/kde-fan-control/
                                      │ config.toml │  (root:root 0755 dir)
                                      │ owned-fans  │  (root:root 0644 files)
                                      └────────────┘
                                            ▲
                                      ┌─────┴──────┐
                                      │  Fallback  │  ExecStopPost (root)
                                      │  binary    │  reads owned-fans.json
                                      └────────────┘
```

1. **User ↔ Daemon**: DBus system bus. Read methods: no auth. Write methods:
   polkit `auth_admin_keep`. Bus policy allows all users to send to the service
   name; auth is enforced per-method in the daemon.

2. **Daemon ↔ sysfs**: Root-only writes to `/sys/class/hwmon/hwmon*/pwm*` and
   `pwm*_enable`. Constrained by `ReadWritePaths=/sys/class/hwmon` in the
   systemd unit.

 3. **Daemon ↔ State files**: Root-owned directory, root-written files. Explicit
    file permissions set (0600 for `owned-fans.json`, 0640 for `config.toml`).

 4. **Fallback binary ↔ State files**: Reads `owned-fans.json` as root, validates
    `sysfs_path` values against `/sys/class/hwmon/hwmon*/pwm[0-9]+` pattern.
    On unparseable JSON, scans all hwmon PWMs and writes 255 to each
    (emergency fallback).

---

## Findings

### CRITICAL

#### C1 — Corrupt `owned-fans.json` defeats fallback binary

| | |
|---|---|
| **Location** | `crates/fallback/src/main.rs:32-38` |
| **Trigger** | `owned-fans.json` is truncated or contains invalid JSON |
| **Impact** | Fallback binary exits with code 1 **without writing any PWM values**. All previously owned fans continue at whatever PWM the daemon last set — potentially low speed. This is the last line of defense (Layer 5). |
| **Root cause** | `persist_owned_fans()` was not atomic; `fs::write()` truncates before writing. |
| **Status** | **FIXED.** `persist_owned_fans()` now writes to a temp file then `rename()`s (atomic). Fallback binary now scans `/sys/class/hwmon/hwmon*/pwm*` on unparseable JSON and writes 255 to all detected PWMs (emergency fallback). |

#### C2 — Stale-fan degradation writes no fallback PWM

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs:446-462` |
| **Trigger** | A fan's telemetry data becomes stale for >100 publish intervals (~25s) |
| **Impact** | `degrade_and_stop()` marks the fan degraded and terminates its control task, but **does not write PWM 255**. The fan remains in the `OwnedFanSet` at whatever PWM the last control iteration produced — potentially low speed, indefinitely. |
| **Root cause** | `degrade_and_stop()` only updated degraded state and cleared status; it did not call `write_fallback_single()`. Other callers (sensor-missing path, PWM-write-failure path) correctly wrote fallback before calling it, but the stale-data path did not. |
| **Status** | **FIXED.** `degrade_and_stop()` now writes PWM=255 before marking a fan degraded. Degraded fans stay in `OwnedFanSet` at safe maximum. |

---

### HIGH

#### H1 — `owned-fans.json` non-atomic write

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs:50` |
| **Status** | **FIXED.** `persist_owned_fans()` now writes to `owned-fans.json.tmp` then `rename()`s to `owned-fans.json`. Sets 0600 permissions on final file. |

#### H2 — NaN/Inf in PID gains not validated

| | |
|---|---|
| **Location** | `crates/core/src/control.rs:42-47` (PidGains), `crates/core/src/config.rs` (validate_draft), `crates/daemon/src/main.rs` (SetDraftFanControlProfile) |
| **Status** | **FIXED.** `PidGains::is_finite()` and `PidLimits::is_finite()` added. Validation at config load, draft validation, and DBus input boundary. |

#### H3 — Fallback binary trusts `sysfs_path` without validation

| | |
|---|---|
| **Location** | `crates/fallback/src/main.rs` |
| **Status** | **FIXED.** Added `is_valid_sysfs_pwm_path()` that validates paths start with `/sys/class/hwmon/hwmon`, contain no `..`, and match `pwm[0-9]+`. Invalid entries are skipped with a warning log. |

#### H4 — Panic mirror race window on fan claim

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs` (ApplyDraft claim loop) |
| **Status** | **FIXED.** Panic mirror sync moved inside the claim loop — `sync_panic_fallback_mirror()` now called after each individual `claim_fan()`. |

#### H5 — Silently-panicked control tasks not detected

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs` (publish loop) |
| **Status** | **FIXED.** `check_task_panics()` method added to `ControlSupervisor`. Called in the publish loop after `check_stale_fans()`. Uses `JoinHandle::is_finished()` to detect panicked tasks, then degrades the affected fan with `FanNoLongerEnrollable`. |

---

### MEDIUM

#### M1 — DBus policy provides no defense-in-depth (revised)

| | |
|---|---|
| **Location** | `packaging/dbus/org.kde.FanControl.conf:10-12` |
| **Current** | `<policy context="default"><allow send_destination="org.kde.FanControl"/></policy>` |
| **Impact** | All authorization relies on per-method `require_authorized()` calls in the daemon. A single missed call exposes a write method to any local user. |
| **Resolution** | **Keep the current broad DBus policy.** Restricting at the bus layer (e.g., denying `Lifecycle` and `Control` interfaces to non-root) would break the polkit authentication flow — the GUI calls `RequestAuthorization()` on the `Lifecycle` interface to trigger the auth dialog, and that call must reach the daemon for polkit to prompt the user. DBus policy cannot distinguish between "call that triggers auth" and "call that performs a write" within the same interface. The correct defense is per-method `require_authorized()` in the daemon code, documented in the authorization matrix above. **Action: audit `require_authorized()` coverage on all DBus methods quarterly, and verify every method in the authorization matrix matches the code.** |

#### M2 — Polkit `start-time=0` weakens process identity

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs` (require_authorized) |
| **Status** | **FIXED.** Now reads `/proc/<pid>/stat` to extract real process start time (field 22) and converts to microseconds using `sysconf(_SC_CLK_TCK)`. Falls back to 0 only if parsing fails. |

#### M3 — PID fallback to 0 on lookup failure

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs` (require_authorized) |
| **Status** | **FIXED.** Now returns `fdo::Error::AccessDenied` if PID lookup fails or returns 0. |

#### M4 — No input length limits on friendly names

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs` (SetSensorName, SetFanName, RemoveSensorName, RemoveFanName) |
| **Status** | **FIXED.** All four methods now validate `id` ≤ 128 chars, `name` ≤ 128 chars, and `name` is non-empty. Returns `fdo::Error::InvalidArgs` on violation. |

#### M5 — `expect()` on auto-tune state

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs` (record_auto_tune_sample) |
| **Status** | **FIXED.** Restructured to avoid second `get_mut()` borrow; auto-tune state transitions now build a fully-owned replacement value and apply it conditionally. Returns gracefully on missing state. |

#### M6 — `unwrap()` on poisoned RwLock in production

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs` (test code) |
| **Status** | **FIXED.** Replaced `try_read().unwrap()` with `.expect()` with descriptive message in test code. Production code already uses graceful error handling patterns. |

#### M7 — No `User=`/`Group=` in service unit (won't fix)

| | |
|---|---|
| **Location** | `packaging/systemd/kde-fan-control-daemon.service` |
| **Current** | The daemon runs as full root (no `User=` directive) |
| **Impact** | Any compromise of the daemon process gives full root access. |
| **Resolution** | **Won't fix.** Writing to `/sys/class/hwmon/hwmonN/pwmN` and setting `pwmN_enable=1` require root on most distributions. Some hwmon drivers enforce root-only access regardless of capabilities; others check file ownership. A dedicated service user with `CAP_SYS_RAWIO` would require per-driver udev rules to change PWM file ownership, which is fragile and distro-specific. The `ReadWritePaths=/sys/class/hwmon` systemd directive also assumes root-level write access. Practical mitigation is limiting what the root process can do via other sandboxing directives (SystemCallFilter, ProtectSystem, etc. — see L1), not trying to drop root entirely. |

#### M8 — No target temperature bounds validation

| | |
|---|---|
| **Location** | `crates/core/src/config.rs` (validate_draft) |
| **Status** | **FIXED.** Added validation: `0 < target_temp_millidegrees <= 150_000` in `validate_draft()`. Returns `InvalidTargetTemperature` on violation. |

#### M9 — No fallback PWM between stop_all and reconcile

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs` (ApplyDraft) |
| **Status** | **FIXED.** `write_fallback_for_owned()` now called immediately after `stop_all()` in ApplyDraft, ensuring all owned fans are at PWM=255 before reconcile starts new tasks. |

#### M10 — `owned-fans.json` no explicit file permissions

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs` (persist_owned_fans) |
| **Status** | **FIXED.** Sets 0600 on `owned-fans.json` after atomic rename. 0640 on `config.toml` after write. |

---

### LOW

#### L1 — Missing systemd sandboxing directives

| | |
|---|---|
| **Location** | `packaging/systemd/kde-fan-control-daemon.service` |
| **Current** | Has `ProtectSystem=strict`, `ProtectHome=yes`, `NoNewPrivileges=yes`, `PrivateTmp=yes` |
| **Missing** | `SystemCallFilter=`, `SystemCallArchitectures=`, `RestrictNamespaces=`, `LockPersonality=`, `MemoryDenyWriteExecute=`, `RestrictRealtime=`, `RestrictSUIDSGID=`, `ProtectClock=`, `ProtectKernelTunables=`, `ProtectKernelModules=`, `ProtectControlGroups=` |
| **Remediation** | Add appropriate directives. The daemon needs: `read`, `write`, `openat`, `close`, `fstat`, `newfstatat`, `lseek`, `mmap`, `munmap`, `mprotect`, `socket`, `connect`, `sendmsg`, `recvmsg`, `epoll_wait`, `clock_nanosleep`, `nanosleep`, `futex`, `sched_yield`, `sigaltstack`, `rt_sigreturn`, `ioctl` (for udev), `getdents64`, `rename` (for atomic writes). |

#### L2 — `ReadWritePaths` broader than needed

| | |
|---|---|
| **Location** | `packaging/systemd/kde-fan-control-daemon.service:23` |
| **Current** | `ReadWritePaths=/sys/class/hwmon` |
| **Impact** | Grants write access to the entire hwmon tree. The daemon only needs `pwm*` and `pwm*_enable` attributes within specific `hwmonN/` subdirectories. However, sysfs itself enforces per-attribute write permissions, so this is a limited concern. |
| **Remediation** | Not practically restrictable further with systemd path-based rules. Document the rationale. |

#### L3 — Config file world-readable

| | |
|---|---|
| **Location** | `crates/core/src/config.rs:313` |
| **Current** | `fs::write()` with default umask → 0644 |
| **Impact** | Any local user can read the full fan control configuration, including PID gains, target temperatures, and sensor associations. No secrets are stored, but it aids reconnaissance. |
| **Remediation** | Set 0640 on config file after write (paired with M10). |

#### L4 — Path derivation by string concatenation

| | |
|---|---|
| **Location** | `crates/core/src/lifecycle.rs` (write_fallback_for_owned, write_fallback_single) |
| **Status** | **FIXED.** Replaced `format!("{}_enable", pwm_path)` with `PathBuf::with_file_name()` which replaces only the filename component. |

#### L5 — TOCTOU in `is_writable()` check

| | |
|---|---|
| **Location** | `crates/core/src/inventory.rs:293-294` |
| **Status** | **FIXED.** `is_writable()` documented as advisory-only best-effort check for inventory reporting, not a gate for writes. The daemon writes directly and handles errors. |

#### L6 — Partial sensor loss + Max aggregation = missed high temp

| | |
|---|---|
| **Location** | `crates/core/src/control.rs:19-39` (`compute_millidegrees`) |
| **Current** | If some sensors in a group fail, the aggregation function operates on the remaining subset. With `Max` aggregation and partial data, the highest temperature reading might be missed if the hottest sensor failed. |
| **Impact** | The PID controller could produce a lower output than intended, potentially undercooling. The staleness detector eventually catches persistent sensor failures. |
| **Remediation** | Document this as a known tradeoff. Consider adding a "minimum sensor count" requirement per fan config (e.g., `min_sources: 1` by default) that forces degradation if too many sensors drop out. |

#### L7 — Signal information leakage

| | |
|---|---|
| **Location** | `crates/daemon/src/main.rs:1978` (`LifecycleEventAppended`), `main.rs:1586` (`AutoTuneCompleted`) |
| **Current** | DBus signals are broadcast to all listeners on the system bus. `LifecycleEventAppended` includes `event_kind` and `detail` strings that may reveal hardware identifiers. `AutoTuneCompleted` reveals which fan completed tuning. |
| **Impact** | Any local process can passively observe fan control activity. Information disclosed is consistent with what read-only DBus methods already expose. |
| **Remediation** | Accept as-by-design for a desktop application. Could add a DBus `send_interface` match rule in the bus policy for clients that want to opt in to signals, but DBus system bus signals are inherently broadcast. |

#### L8 — No AppArmor/SELinux profile

| | |
|---|---|
| **Current** | No MAC profile exists for the daemon or fallback binary |
| **Impact** | Missing defense-in-depth layer. A compromise of the root process has no MAC confinement. |
| **Remediation** | Create a minimal AppArmor profile for the daemon: allow read on `/sys/class/hwmon/**`, write on `/sys/class/hwmon/**/pwm*`, read/write on `/var/lib/kde-fan-control/**`, DBus system bus access. |

#### L9 — Documentation-code mismatch: ValidateDraft auth

| | |
|---|---|
| **Location** | `docs/dbus-api.md` |
| **Status** | **FIXED.** Updated auth column for `ValidateDraft` from `root`/`polkit` to `none`, matching the code. |

---

## DBus Authorization Matrix

| Method | Interface | Auth (code) | Auth (doc) | Impact if bypassed |
|---|---|---|---|---|
| `Snapshot` | Inventory | none | none | By design |
| `SetSensorName` | Inventory | polkit | root | Config write, name injection |
| `SetFanName` | Inventory | polkit | root | Config write, name injection |
| `RemoveSensorName` | Inventory | polkit | root | Config modification |
| `RemoveFanName` | Inventory | polkit | root | Config modification |
| `GetDraftConfig` | Lifecycle | none | none | Exposes full PID gains, targets |
| `GetAppliedConfig` | Lifecycle | none | none | Exposes live PID configuration |
| `GetDegradedSummary` | Lifecycle | none | none | Reveals failing fans |
| `GetLifecycleEvents` | Lifecycle | none | none | Timestamped event history |
| `GetRuntimeState` | Lifecycle | none | none | Fan ownership + status |
| `GetOverviewStructure` | Lifecycle | none | none | Fan IDs, names, states |
| `GetOverviewTelemetry` | Lifecycle | none | none | Live temps, RPMs, output |
| `RequestAuthorization` | Lifecycle | polkit | root | Triggers auth dialog |
| `SetDraftFanEnrollment` | Lifecycle | polkit | root | Enroll fans for control |
| `RemoveDraftFan` | Lifecycle | polkit | root | Modify draft |
| `DiscardDraft` | Lifecycle | polkit | root | Wipe draft |
| `ValidateDraft` | Lifecycle | **none** | ~~root~~ | Read-only validation (doc bug: L9) |
| `ApplyDraft` | Lifecycle | polkit | root | **Claims hardware, starts PID loops** |
| `GetControlStatus` | Control | none | none | Live PID state |
| `GetAutoTuneResult` | Control | none | none | Auto-tune PID proposals |
| `StartAutoTune` | Control | polkit | root | Forces 100% PWM for 30s |
| `AcceptAutoTune` | Control | polkit | root | Writes gains to draft |
| `SetDraftFanControlProfile` | Control | polkit | root | Arbitrary PID parameters |

### Auth enforcement mechanism

Authorization is **per-method, opt-in**. Each write method must:
1. Accept `#[zbus(connection)]` and `#[zbus(header)]` parameters
2. Call `require_authorized(connection, &header).await?` as the first action

A method that omits either step is **open to all users**. There is no global
auth interceptor or middleware. The DBus `.conf` policy does not filter methods.

---

## Safety Layer Gap Analysis

The [safety model](safety-model.md) defines 5 layers (plus Layer 3.5 for
re-assessment). This section maps each layer to known gaps discovered in this
audit.

| Layer | Mechanism | Gaps |
|---|---|---|
| **1 — Graceful shutdown** | Signal handler → `stop_all()` → `write_fallback_for_owned()` → exit | Config save on critical path (could delay exit beyond watchdog). Between `stop_all()` and fallback write, fans are uncontrolled. |
| **2 — Panic hook** | `PanicFallbackMirror` (StdRwLock) → sync `fs::write(pwm, 255)` | Mirror can be stale if panic occurs between ownership change and sync (H4). Poisoned mirror lock silently skips that fan. |
| **3 — Runtime degradation** | Per-fan fallback on sensor/write failure | ~~Stale-fan degradation does not write fallback PWM (C2)~~ — **FIXED**. Silently-panicked control tasks not detected (H5). Auto-tune sensor failure path doesn't write fallback. |
| **3.5 — Re-assessment** | Periodic re-assessment of degraded fans (10s default) | Recoverable fans retry indefinitely. Non-recoverable reasons (`FanMissing`, `FanNoLongerEnrollable`) are never re-assessed. `FanNoLongerEnrollable` from PWM write failure could be transient but is not re-assessed. |
| **4 — Boot reconciliation** | Restart → verify applied config vs hardware | Sound. Always rebuilds from persisted config. |
| **5 — ExecStopPost fallback** | `kde-fan-control-fallback` reads `owned-fans.json` | Non-atomic write can corrupt file → fallback exits without action (C1+H1). No path validation on `sysfs_path` (H3). Fallback binary can't recover from corrupt JSON. |

---

## Remediation Plan

### Priority 0 — Safety-Critical (fix immediately)

| Finding | Action | Status |
|---|---|---|
| **C2** | `degrade_and_stop()` now writes PWM=255 before marking degraded | **FIXED** |
| **C1 + H1** | Atomic write for `owned-fans.json` (write-to-temp + rename). Fallback binary scans all hwmon PWMs if JSON is unparseable. | **FIXED** |
| **H2** | `is_finite()` validation on PID gains at deserialization and DBus entry points. `PidLimits::is_finite()` added. | **FIXED** |

### Priority 1 — High-Impact (fix before next release)

| Finding | Action | Status |
|---|---|---|
| **H3** | Path prefix validation in fallback binary: `sysfs_path` must start with `/sys/class/hwmon/`, no `..`, match `pwm[0-9]+`. | **FIXED** |
| **H4** | Sync panic mirror after each `claim_fan()` call. | **FIXED** |
| **H5** | `check_task_panics()` monitor in publish loop detects panicked `JoinHandle`s and degrades affected fans. | **FIXED** |
| **M8** | Target temp bounds (1 ≤ target ≤ 150,000 mC) in `validate_draft()`. | **FIXED** |
| **M1** | Quarterly audit of `require_authorized()` coverage on all DBus methods; authorization matrix in SECURITY.md is the source of truth. | Ongoing |

### Priority 2 — Hardening (fix soon)

| Finding | Action | Status |
|---|---|---|
| **M2 + M3** | Read `/proc/<pid>/stat` for polkit start-time; reject auth if PID is 0 or lookup fails. | **FIXED** |
| **M4** | 128-char limit on names, non-empty validation on name strings. | **FIXED** |
| **M5 + M6** | Replace `expect()` on auto-tune state with graceful pattern; replace `try_read().unwrap()` with descriptive `.expect()`. | **FIXED** |
| **M9** | Write PWM=255 to all owned fans immediately after `stop_all()` in ApplyDraft. | **FIXED** |
| **M10 + L3** | Set 0600 on `owned-fans.json`, 0640 on `config.toml` after write. | **FIXED** |

### Priority 3 — Defense-in-Depth (track as tech debt)

| Finding | Action | Status |
|---|---|---|
| **L1** | Add systemd sandboxing directives (SystemCallFilter, etc.) | Pending |
| **M7** | Won't fix — root required for sysfs PWM writes; harden via L1 directives | Won't fix |
| **L4** | Use `PathBuf::with_file_name()` instead of string concatenation for `_enable` derivation | **FIXED** |
| **L6** | Add `min_sources` config option for sensor group degradation | Pending |
| **L8** | Create AppArmor profile | Pending |
| **L9** | Fix ValidateDraft auth in docs (root → none) | **FIXED** |
| **L5** | `is_writable()` pre-check documented as advisory-only | **FIXED** |

---

## Security Audit Framework

For ongoing or periodic security review, the following multi-agent structure
ensures exhaustive coverage:

### Agent 1: Fallback Path Integrity
**Scope**: All 5 safety layers. Prove each layer works under adversarial conditions.
**Tests to add**:
- Simulate truncated `owned-fans.json` → verify fallback scans hwmon
- Simulate panic during fan claim → verify mirror contains all fans
- Simulate control task panic → verify monitor detects and degrades
- Verify stale-fan degradation writes PWM 255

### Agent 2: Input Validation Hardening
**Scope**: Every DBus method input, config deserialization, f64 in PID gains.
**Tests to add**:
- NaN/Inf in all f64 PID fields → rejected
- Target temp out of bounds → rejected
- Name strings > 128 chars → rejected
- `sysfs_path` with path traversal in fallback → rejected

### Agent 3: Atomicity & Persistence Safety
**Scope**: `owned-fans.json`, config save, shutdown path persistence.
**Tests to add**:
- Kill daemon mid-write of `owned-fans.json` → verify temp+rename leaves valid file
- Kill daemon during `ApplyDraft` at each phase → verify boot reconciliation recovers
- Slow disk I/O during shutdown → verify fans still get fallback

### Agent 4: DBus Auth & Policy Hardening
**Scope**: DBus policy, polkit action, `require_authorized()` robustness.
**Tests to add**:
- Non-root user calls write method → verify AccessDenied
- PID lookup returns 0 → verify rejection
- Polkit start-time validation → verify correct value
- Method without `require_authorized()` → audit all methods quarterly

### Agent 5: Systemd Sandboxing & Privilege Reduction
**Scope**: Service unit hardening, capabilities, file permissions.
**Tests to add**:
- Daemon cannot write outside `/sys/class/hwmon/` → verify EPERM
- Daemon cannot read `/home/` → verify EACCES
- State files have 0600/0640 permissions → verify after write

### Agent 6: Control Loop Safety Verification
**Scope**: PID math, sensor failure modes, task lifecycle.
**Tests to add**:
- All sensors fail → fan gets degraded + fallback PWM
- PID gains = NaN → output clamped safe or rejected
- Control task JoinHandle panics → monitor detects within 1 tick
- Partial sensor loss with Max aggregation → document behavior