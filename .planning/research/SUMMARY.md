# Project Research Summary

**Project:** KDE Fan Control
**Domain:** Linux desktop fan-control system
**Researched:** 2026-04-10
**Confidence:** HIGH

## Executive Summary

KDE Fan Control should be built as a safety-first Linux desktop product, not as a generic hardware tweaking suite. The research strongly converges on a split architecture: a **root Rust daemon** owns hwmon/sysfs discovery, enrollment, control loops, persistence, and fail-safe behavior; unprivileged **Qt 6 + Kirigami GUI** and **Rust CLI** clients talk to it only through a **versioned D-Bus API**. This matches how experts build trustworthy Linux control software: keep hardware writes centralized, supervised by systemd, and isolated from the UI.

The right v1 is a **trustworthy replacement for `fancontrol`** with better UX and stronger safety guarantees. That means inventory and capability validation first, explicit per-fan enrollment, one reliable automatic control path, daemon-owned persistence, boot/suspend recovery, and clear runtime visibility. Early differentiators worth including are friendly sensor/fan naming, sensor aggregation, a polished KDE-native UI, and a unified daemon/GUI/CLI contract.

The biggest risks are not framework choice but unsafe assumptions: persisting unstable `hwmonN` paths, treating writable PWM as safe support, relying on clean shutdown for fail-safe recovery, and tuning control loops faster than sensors update. The roadmap should therefore be inventory-first and safety-first: prove hardware identity, enrollment rules, transactionality, crash recovery, and fallback behavior before spending time on richer UX or advanced tuning.

## Key Findings

### Recommended Stack

The research recommends a split stack because the product crosses two very different domains: privileged Linux systems control and KDE-native desktop UX. Rust is the best fit for the long-running daemon and CLI, while Qt/Kirigami remains the right choice for the GUI. The daemon should expose D-Bus as the only control boundary and run as a hardened systemd system service with explicit readiness and watchdog support.

**Core technologies:**
- **Rust 1.94.1 (Edition 2024):** daemon and CLI foundation — strong fit for safe, concurrent systems code.
- **Tokio + zbus:** async runtime and D-Bus implementation — mature, Rust-native, and ideal for long-lived service loops.
- **raw hwmon sysfs + optional udev enrichment:** hardware integration — kernel ABI is the real source of truth; identity needs more than `hwmonN`.
- **systemd system service (`Type=notify`):** lifecycle supervision — required for readiness, restart, watchdog, and safe boot behavior.
- **Qt 6 / Qt Quick / Qt DBus + KF6 Kirigami:** GUI stack — best match for a KDE-first desktop app.
- **TOML + serde:** single active config — simpler and safer than introducing a database in v1.

Critical version guidance: start greenfield Rust on **Edition 2024**, target **Qt 6.11.x upstream** while keeping code compatible with roughly **Qt 6.8+ distro floors**, and use a **versioned D-Bus namespace** from day one.

### Expected Features

The market baseline is clear: users replacing `fancontrol` expect reliable discovery, mapping of temperature sources to controllable fans, automatic control, persistence, and safe handoff back to firmware. The product does not need to match CoolerControl’s full suite in v1, but it does need to feel safer and easier to trust.

**Must have (table stakes):**
- Hardware discovery of sensors, fan tach inputs, and controllable PWM channels.
- Capability validation before enrollment, including proof of safe fallback behavior.
- Per-fan BIOS/firmware vs daemon ownership.
- Temperature-based automatic control with per-fan sensor source selection.
- Safe min/max output bounds, start behavior, and fail-safe recovery.
- Persistent single active configuration with boot auto-start.
- Live monitoring of temps, RPM, output, and control state.
- Suspend/resume re-apply or explicit recovery behavior.
- CLI-level inspectability and clear unsupported/partial-support reporting.

**Should have (competitive):**
- Friendly names for sensors and fans.
- Sensor aggregation (`max`, `avg`, `min`, `median`).
- Per-fan PID control with understandable parameters.
- KDE-native GUI + tray.
- Unified daemon + GUI + CLI over one API.
- Explicit safe-enrollment workflow.

**Defer (v2+):**
- Multiple saved profiles and global modes.
- Rich fan-curve editor alongside PID.
- Alerts, dashboards, and historical graphs.
- GPU fan support, AIO/liquidctl integration, and Web/remote UI.
- NBFC-style embedded-controller laptop support.
- Aggressive adaptive/self-learning control beyond conservative guided tuning.

### Architecture Approach

The architecture research is strongly aligned with the stack and feature findings: use a **single authoritative root daemon** with four internal layers — hardware adapter, control core, service/API layer, and persistence/runtime integration. GUI, tray, and CLI must remain unprivileged D-Bus clients. Internally, the daemon should separate persistent config, derived runtime plan, telemetry, and safety state; apply config changes transactionally; serialize writes per chip/channel; and expose a versioned object tree with read-only telemetry and polkit-gated mutating operations.

**Major components:**
1. **Inventory + capability layer** — discovers hwmon devices, builds stable identities, and classifies channels as safe, unsafe, read-only, or unsupported.
2. **Control core + fail-safe manager** — owns enrollment, aggregation, PID scheduling, actuator clamping, and emergency fallback.
3. **D-Bus service + config transaction manager** — exposes versioned API, authorizes mutations, stages/validates/apply-rolls back config changes.
4. **Persistence + systemd integration** — stores the single active config, reconciles on boot, and participates in restart/watchdog/safe-stop flows.
5. **Qt/QML GUI and CLI clients** — render inventory/runtime state and submit intent, but never touch sysfs or config files directly.

### Critical Pitfalls

1. **Crash-only safety is not the same as clean shutdown safety** — use systemd restart/watchdog plus an out-of-process recovery path that can still force safe fan state after a crash.
2. **Persisting `hwmonN` identifiers will break reenrollment** — store stable physical identity (real devpath + chip/controller metadata) and refuse auto-ownership when confidence drops.
3. **Writable does not mean safely controllable** — require capability tiers and explicit enrollment validation before daemon ownership.
4. **Over-eager PID loops will create noisy, unstable control** — make sampling sensor-aware and ship clamps, anti-windup, deadband, and rate limits from day one.
5. **Unsafe sysfs writes can coerce to zero** — centralize typed validation, clamp before writes, and verify read-back on critical operations.

## Implications for Roadmap

Based on the combined research, the roadmap should be organized around risk retirement, not UI completeness.

### Phase 1: Hardware Identity, Safety Contract, and Read-Only Inventory
**Rationale:** All later work depends on correctly understanding what hardware exists and whether it is safe to manage. This is the highest-risk area and the foundation for every feature.
**Delivers:** Sysfs/hwmon adapter, stable identity model, capability classifier, read-only inventory/telemetry D-Bus API, basic CLI inspect commands, initial systemd service skeleton, sensor normalization.
**Addresses:** hardware discovery, unsupported/partial-support reporting, CLI inspectability.
**Avoids:** unstable `hwmonN` persistence, malformed writes, fake “supported” hardware, boot-time partial discovery.

### Phase 2: Enrollment, Transactions, Persistence, and Lifecycle Safety
**Rationale:** Before enabling active control, the daemon must prove it can own channels safely, apply changes atomically, and recover sanely across boot, suspend, and failure.
**Delivers:** Safe enrollment workflow, per-fan ownership states, config transaction manager, TOML persistence, boot restore/reconciliation, suspend/resume handling, polkit-gated mutating API, external recovery helper/watchdog strategy.
**Uses:** Rust daemon, zbus D-Bus layer, TOML persistence, systemd hardening and watchdog integration.
**Implements:** config manager, persistence/runtime layer, safety state machine.
**Avoids:** split-brain state, crash-only fail-safe gaps, unsafe reenrollment, blind config replay at boot.

### Phase 3: Conservative Automatic Control Core
**Rationale:** Control logic should come only after inventory and safety rules are trustworthy. The first goal is reliable thermal behavior, not tuning sophistication.
**Delivers:** Per-fan sensor selection, sensor aggregation, bounded PI/PID control, output clamps, anti-windup, deadband, slew-rate limiting, actuator serialization, real hardware write/readback verification, degraded/fallback states.
**Addresses:** automatic temperature-based control, min/max bounds, live runtime state, fail-safe control transitions.
**Avoids:** PID instability, noisy fan hunting, unsafe assumptions about sensor cadence, uncontrolled concurrent writes.

### Phase 4: KDE GUI, Tray, and Operator UX
**Rationale:** Once the daemon contract is stable, the GUI can be built quickly and safely on top of real behavior rather than guesses.
**Delivers:** Kirigami GUI, tray/status integration, inventory and telemetry views, enrollment/config screens, friendly naming, clear health/degraded-state UX, parity with CLI on core operations.
**Addresses:** native KDE UX, live monitoring, friendly labels, safe enrollment usability.
**Avoids:** GUI-owned config, direct D-Bus from ad hoc QML, client/runtime divergence.

### Phase 5: Guided Tuning and Selected v1 Differentiators
**Rationale:** Only after the core loop is proven should the project expand into higher-value usability features.
**Delivers:** Conservative guided PID tuning, better diagnostic views, refined aggregation UX, optional basic auto-tuning on validated hardware only.
**Addresses:** early differentiators without expanding into v2 surface area.
**Avoids:** unsafe one-click tuning, premature profile complexity, overpromising unsupported hardware.

### Phase Ordering Rationale

- Discovery, identity, and safety classification must precede enrollment, persistence, and control because every feature depends on trustworthy hardware modeling.
- Transactionality and lifecycle recovery must precede live control because incorrect persistence or crash handling is a thermal safety issue, not a polish issue.
- GUI/tray work belongs after the D-Bus and runtime contract stabilizes; otherwise the project risks rebuilding UX around moving backend semantics.
- Advanced tuning should come last because the research explicitly flags PID sampling, derivative behavior, and auto-tune guardrails as areas requiring extra caution.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1:** hardware enrollment criteria and stable identity heuristics across varied boards need continued validation on real machines.
- **Phase 2:** crash-safe failover, boot activation semantics, and recovery-helper behavior deserve dedicated implementation research.
- **Phase 3:** sensor-aware sampling strategy and conservative tuning defaults need targeted control-loop research/testing.
- **Phase 5:** any auto-tuning work should be treated as research-heavy and opt-in only.

Phases with standard patterns (skip research-phase):
- **Phase 4:** Qt/Kirigami client structure on top of a stable D-Bus API is well-documented and comparatively low-risk.
- **Most of persistence/API plumbing in Phase 2:** versioned D-Bus naming, daemon-owned state, and polkit-gated mutations follow standard patterns.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Strong convergence across official Rust, Qt, kernel hwmon, and systemd sources. |
| Features | HIGH | Market baseline is clear across `fancontrol`, `thinkfan`, NBFC-Linux, CoolerControl, and kernel docs. |
| Architecture | HIGH | Boundaries and sequencing are well supported by D-Bus, systemd, Qt, and hwmon documentation. |
| Pitfalls | MEDIUM-HIGH | Core failure modes are well grounded; some PID/tuning advice is expert synthesis rather than a single authoritative spec. |

**Overall confidence:** HIGH

### Gaps to Address

- **Real-hardware coverage:** research is strong on patterns but cannot guarantee which boards expose safe controllable channels; plan for validation on representative hardware early.
- **Exact fail-safe mechanism:** the recovery-helper design is clearly needed, but the final implementation details should be proven with crash-path tests.
- **Sensor normalization edge cases:** some temperature channels may need board/driver-specific interpretation; ambiguous sensors should stay read-only until confidence is high.
- **Control defaults:** default PI/PID parameters and sampling intervals should be calibrated empirically against actual hwmon update cadence.

## Sources

### Primary (HIGH confidence)
- Linux kernel hwmon sysfs interface — canonical control and sensor ABI.
- D-Bus specification — object model, naming, and standard interface guidance.
- systemd service documentation — lifecycle, readiness, restart, watchdog, stop semantics.
- Qt 6 / Qt Quick / Qt DBus docs — GUI and client architecture guidance.
- KDE Kirigami docs — KDE-native application shell and component guidance.
- Context7 `/dbus2/zbus`, `/tokio-rs/tokio`, `/clap-rs/clap` — Rust daemon/runtime/API implementation details.

### Secondary (MEDIUM confidence)
- `fancontrol(8)` manual — defensive validation patterns and expected operator workflow.
- ArchWiki fan-speed-control overview — suspend, vendor, and partial-support realities.
- `udev(7)` / related Linux integration docs — device identity enrichment and operational behavior.

### Tertiary (LOW confidence)
- Control-loop tuning guidance synthesized from Linux sensor/update constraints and general controls practice — useful, but must be validated on real hardware.

---
*Research completed: 2026-04-10*
*Ready for roadmap: yes*
