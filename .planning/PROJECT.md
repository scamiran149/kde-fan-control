# KDE Fan Control

## What This Is

KDE Fan Control is a Linux desktop fan-control application for machines where `fancontrol` is too rigid or cumbersome. It runs a privileged daemon that manages per-fan PID control using selectable temperature inputs or aggregated sensor groups, and exposes control through DBus to a KDE/Qt6 GUI, system tray app, and CLI.

## Core Value

Users can safely and flexibly control desktop fan behavior with understandable per-fan PID policies, without losing fail-safe behavior.

## Current State

**Shipped v1.0 MVP** (2026-04-12)
- ~8,280 LOC Rust (daemon + CLI + core), ~5,462 LOC C++, ~5,462 LOC QML
- Tech stack: Rust (Tokio + zbus + clap), Qt6 + Kirigami + KStatusNotifierItem
- 4 phases, 15 plans, 29 tasks, 77 commits
- All 52 v1 requirements validated

## Current Milestone: Awaiting Definition

Phases 5-8 (v1.1 Packaging & System Integration) have been removed for restructuring. Next milestone to be defined.

## Requirements

### Validated

- ✓ Discover supported temperature sensors, fan channels, and control interfaces from Linux hardware nodes — v1.0
- ✓ Allow users to assign friendly names to sensors and fan controllers — v1.0
- ✓ Allow each fan to remain under BIOS control or be managed by the daemon — v1.0
- ✓ Auto-start management for enrolled fans on boot from the persisted active configuration — v1.0
- ✓ Allow each controlled fan to choose a temperature source or a sensor aggregation — v1.0
- ✓ Support sensor aggregation functions including average, max, min, and median — v1.0
- ✓ Allow each controlled fan to define a target temperature setpoint — v1.0
- ✓ Run per-fan PID loops with configurable P, I, and D terms — v1.0
- ✓ Provide basic PID auto-tuning for controlled fans — v1.0
- ✓ Surface partially supported or unavailable hardware while refusing unsafe enrollment — v1.0
- ✓ Support PWM or voltage control mode when exposed by hardware — v1.0
- ✓ Expose hardware, configuration, and runtime control over DBus — v1.0
- ✓ Persist a single active configuration in the daemon — v1.0
- ✓ Provide a CLI for inspection and configuration — v1.0
- ✓ Provide an attractive KDE/Qt6 GUI with system tray integration — v1.0
- ✓ On daemon failure, set previously daemon-controlled fans to high speed — v1.0

### Deferred (from removed phases 5-8)

- [ ] All installed artifacts follow standard FHS paths
- [ ] CLI binary installed to /usr/bin with kfc symlink
- [ ] systemd unit file with Type=notify, boot-enabled, watchdog, hardening
- [ ] ExecStopPost fallback helper for crash-safe fan recovery
- [ ] DBus service activation for on-demand daemon start
- [ ] polkit policy with granular actions and auth_admin_keep
- [ ] Daemon replaces UID=0 check with polkit CheckAuthorization
- [ ] .desktop file and SVG icon for the GUI
- [ ] .deb package with maintainer scripts
- [ ] install.sh fallback installer

### Out of Scope

- Cross-desktop UI parity in v1 — KDE/Qt6 is the primary target first
- Remote or web-based fan control — not core to local desktop control
- Multiple saved profiles in v1 — start with one active persisted configuration and add profiles later
- Highly advanced tuning science beyond basic auto-tuning — defer until the core control loop is proven
- Broad vendor-specific customization UI for every edge-case chipset — first focus on a solid generic Linux hardware model

## Context

Shipped v1.0 with ~8,280 LOC Rust and ~10,924 LOC C++/QML across 104 files.
Tech stack: Rust (Tokio 1.x + zbus 5.x + clap 4.x), Qt6 + Kirigami 6 + KStatusNotifierItem.

The system successfully implements: hardware inventory via sysfs hwmon, draft/apply config lifecycle, boot reconciliation with degraded state, safe-maximum fallback for owned fans, per-fan PID temperature control with auto-tune, and a KDE-native GUI with system tray and notifications.

Known technical debt: StatusMonitor uses polling instead of reactive DBus signal subscriptions; KF6 dev packages need proper CMake support; some GUI navigation stubs remain (tray→main window, popover integration).

The user interaction model is split between a privileged backend and unprivileged frontends. DBus is the primary API boundary with read-open/write-privileged access. The daemon owns persistence and authoritative runtime state; CLI and GUI are thin control surfaces.

Safety is central. The daemon avoids interfering with BIOS-managed fans, drives owned fans to safe-maximum on failure, and auto-manages enrolled fans on boot. Fallback incidents are persisted and inspectable across restarts.

## Constraints

- **Platform**: Linux desktop only — hardware control depends on Linux kernel sensor/fan interfaces
- **Privilege boundary**: Hardware control runs as a root daemon — direct fan writes require elevated privileges
- **IPC**: DBus-first architecture — CLI and GUI should use the same control surface
- **Backend stack**: Rust for the daemon and CLI — prioritize safety, concurrency control, and predictable deployment
- **UI stack**: KDE/Qt6 with QML — optimize for native Linux desktop experience
- **Persistence**: Single active daemon-owned configuration in v1 — avoid split-brain config management across clients
- **Safety**: Controlled fans must fail to high speed on service failure — prevent thermal risk
- **Compatibility**: BIOS-managed fans must remain untouched when not explicitly enrolled — avoid breaking existing system behavior
- **Hardware variability**: PWM and voltage control may vary by board or controller — implementation must tolerate partial capability exposure

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| KDE/Qt6-first GUI | Prioritize a strong native KDE desktop experience instead of generic lowest-common-denominator UI | ✓ Good |
| Rust daemon and CLI with Qt6/QML GUI | Split the system along natural strengths for systems work and desktop UI | ✓ Good |
| DBus as primary management API | Keeps the daemon authoritative and unifies CLI and GUI behavior | ✓ Good |
| Daemon-owned persistence | Avoids split-brain config management across clients | ✓ Good |
| Single active persisted configuration in v1 | Reduces complexity while leaving room for profiles later | ✓ Good |
| Enrolled fans are continuously daemon-owned | Keeps runtime semantics and safety behavior explicit | ✓ Good |
| Managed fans auto-start on boot | Restores the intended cooling policy without extra user action after reboot | ✓ Good |
| Multiple sensor aggregation modes in v1 | Desktop thermal control often needs more than a single sensor | ✓ Good |
| Optional lm-sensors assistance | Improves discovery without making users depend on another layer for core control | ✓ Good |
| Fail-safe high-speed fallback for controlled fans | Safety behavior must match or exceed fancontrol expectations | ✓ Good |
| Draft/apply config pattern | No write-through to live config; explicit promotion prevents accidental changes | ✓ Good |
| Best-effort partial apply | Valid fans promoted, invalid ones reported; no all-or-nothing blocking | ✓ Good |
| Read-open/write-privileged DBus access | Unprivileged reads for monitoring; root-only writes for safety | ✓ Good |
| PID output as logical 0-100% | Hardware-specific scaling deferred to actuator helpers | ✓ Good |
| StatusMonitor uses refreshAll() polling | Qt6 QDBusConnection::connect() lacks lambda support | ⚠️ Revisit |
| serde(default) for backward-compatible config | Phase 2 TOML configs deserialize with safe defaults | ✓ Good |
| KF5 compat headers for KNotifications | KF6 dev packages not available; linking .so.6 directly | ⚠️ Revisit |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition**:
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone**:
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-12 after removing phases 5-8 for restructuring*