# KDE Fan Control

## What This Is

KDE Fan Control is a Linux desktop fan-control application for machines where `fancontrol` is too rigid or cumbersome. It runs a privileged daemon that manages per-fan PID control using selectable temperature inputs or aggregated sensor groups, and exposes control through DBus to a KDE/Qt6 GUI, system tray app, and CLI.

## Core Value

Users can safely and flexibly control desktop fan behavior with understandable per-fan PID policies, without losing fail-safe behavior.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Discover supported temperature sensors, fan channels, and control interfaces from Linux hardware nodes
- [ ] Allow users to assign friendly names to sensors and fan controllers
- [ ] Allow each fan to remain under BIOS control or be managed by the daemon
- [ ] Auto-start management for enrolled fans on boot from the persisted active configuration
- [ ] Allow each controlled fan to choose a temperature source or a sensor aggregation
- [ ] Support sensor aggregation functions including average, max, min, and median
- [ ] Allow each controlled fan to define a target temperature setpoint
- [ ] Run per-fan PID loops with configurable P, I, and D terms
- [ ] Provide basic PID auto-tuning for controlled fans
- [ ] Surface partially supported or unavailable hardware while refusing unsafe enrollment
- [ ] Support PWM or voltage control mode when exposed by hardware
- [ ] Expose hardware, configuration, and runtime control over DBus
- [ ] Persist a single active configuration in the daemon
- [ ] Provide a CLI for inspection and configuration
- [ ] Provide an attractive KDE/Qt6 GUI with system tray integration
- [ ] On daemon failure, set previously daemon-controlled fans to high speed

### Out of Scope

- Cross-desktop UI parity in v1 — KDE/Qt6 is the primary target first
- Remote or web-based fan control — not core to local desktop control
- Multiple saved profiles in v1 — start with one active persisted configuration and add profiles later
- Highly advanced tuning science beyond basic auto-tuning — defer until the core control loop is proven
- Broad vendor-specific customization UI for every edge-case chipset — first focus on a solid generic Linux hardware model

## Context

This project targets Linux desktop systems with hardware-exposed fan and sensor interfaces, primarily through sysfs hwmon and related kernel interfaces. The system should prefer direct understanding of Linux hardware nodes, while remaining open to using `lm-sensors` and `sensors-detect` as optional discovery assistance rather than required runtime dependencies.

The user interaction model is split between a privileged backend and unprivileged frontends. DBus is the primary API boundary. The daemon owns persistence and authoritative runtime state; CLI and GUI clients should behave as control and monitoring surfaces rather than direct config-file editors.

Safety is central. The daemon must avoid interfering with fans left under BIOS control, drive previously controlled fans to a safe high-speed state if the service exits unexpectedly or fails, and auto-manage enrolled fans on boot from the persisted configuration. Writable control without tach feedback is acceptable, but safe enrollment still depends on confidence that the daemon can both control the fan and force a safe maximum output when needed.

The GUI should feel native in KDE and use Qt6/QML patterns, including tray-based visibility for quick inspection and access. The backend and CLI should be implemented in Rust.

Continuously self-tuning or fuzzy PID control is interesting, especially from industrial control and PLC ecosystems, but should be treated as a research track rather than a v1 requirement.

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
| KDE/Qt6-first GUI | Prioritize a strong native KDE desktop experience instead of generic lowest-common-denominator UI | — Pending |
| Rust daemon and CLI with Qt6/QML GUI | Split the system along natural strengths for systems work and desktop UI | — Pending |
| DBus as primary management API | Keeps the daemon authoritative and unifies CLI and GUI behavior | — Pending |
| Daemon-owned persistence | Avoids split-brain config management across clients | — Pending |
| Single active persisted configuration in v1 | Reduces complexity while leaving room for profiles later | — Pending |
| Enrolled fans are continuously daemon-owned | Keeps runtime semantics and safety behavior explicit | — Pending |
| Managed fans auto-start on boot | Restores the intended cooling policy without extra user action after reboot | — Pending |
| Multiple sensor aggregation modes in v1 | Desktop thermal control often needs more than a single sensor | — Pending |
| Optional lm-sensors assistance | Improves discovery without making users depend on another layer for core control | — Pending |
| Fail-safe high-speed fallback for controlled fans | Safety behavior must match or exceed fancontrol expectations | — Pending |

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
*Last updated: 2026-04-10 after initialization*
