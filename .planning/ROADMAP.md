# Roadmap: KDE Fan Control

## Overview

KDE Fan Control reaches v1 by retiring the highest thermal-risk unknowns first: trustworthy hardware inventory, safe enrollment and persistence, conservative daemon-owned control, and finally a KDE-native operator surface on top of the stabilized DBus contract.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Hardware Inventory & Visibility** - Users can inspect supported hardware, capabilities, and support state through daemon-owned read-only surfaces.
- [ ] **Phase 2: Safe Enrollment & Lifecycle Recovery** - Users can safely hand fan ownership to the daemon and trust boot, persistence, and failure behavior.
- [ ] **Phase 3: Temperature Control & Runtime Operations** - Managed fans follow configured temperature-driven PID policies with inspectable runtime state and guided tuning.
- [ ] **Phase 4: KDE GUI & Tray Experience** - Users can operate the system comfortably from a native KDE/Qt6 interface and tray.

## Phase Details

### Phase 1: Hardware Inventory & Visibility
**Goal**: Users can inspect discovered sensors and fan hardware, understand what is safely supported, and access that inventory through daemon-owned read-only interfaces.
**Depends on**: Nothing (first phase)
**Requirements**: HW-01, HW-02, HW-03, HW-04, HW-05, HW-06, HW-07, HW-08, BUS-01, CLI-01
**Success Criteria** (what must be TRUE):
  1. User can list detected temperature sensors, fan controllers, and fan channels from the daemon through DBus and the CLI.
  2. User can inspect each fan channel's capabilities, including available control modes and whether tach or RPM feedback exists.
  3. User can see every discovered endpoint classified as available, partial, or unavailable, with a visible reason when full support is not possible.
  4. User can assign friendly names to detected sensors and fan hardware and see those names persist in inventory views.
**Plans**: TBD

### Phase 2: Safe Enrollment & Lifecycle Recovery
**Goal**: Users can choose which fans the daemon owns, persist one authoritative configuration, and trust safe behavior across boot and daemon failure.
**Depends on**: Phase 1
**Requirements**: FAN-01, FAN-02, FAN-03, FAN-04, FAN-05, FAN-06, SAFE-01, SAFE-02, SAFE-03, SAFE-05, SAFE-06, SAFE-07, BUS-02, BUS-04, BUS-06, CONF-01, CONF-02, CONF-03, CLI-02
**Success Criteria** (what must be TRUE):
  1. User can leave any detected fan unmanaged under BIOS or existing system control, or enroll a safely supported fan for daemon management, while unsafe hardware is refused.
  2. User can choose the control mode used for an enrolled fan when hardware exposes multiple safe options such as PWM or voltage.
  3. User can create and update the single active daemon-owned configuration over DBus-backed CLI flows, and the persisted configuration survives reboot.
  4. After reboot, previously managed fans resume safely from persisted configuration or surface a degraded state instead of silently claiming success.
  5. If the daemon exits unexpectedly, previously daemon-controlled fans move to safe high speed, unmanaged fans remain untouched, and the fallback state is inspectable.
**Plans**: TBD

### Phase 3: Temperature Control & Runtime Operations
**Goal**: Users can run conservative per-fan temperature-based PID control with valid sensor inputs, inspect live runtime state, and use basic auto-tuning.
**Depends on**: Phase 2
**Requirements**: SNS-01, SNS-02, SNS-03, SNS-04, SNS-05, SNS-06, PID-01, PID-02, PID-03, PID-04, PID-05, PID-06, PID-07, SAFE-04, BUS-03, BUS-05, CLI-03, CLI-04
**Success Criteria** (what must be TRUE):
  1. User can assign each managed fan either one temperature sensor or a multi-sensor group and choose average, max, min, or median aggregation.
  2. User can set a target temperature and P, I, and D gains for each managed fan and can tell that v1 control is temperature-target PID, not RPM-target tracking.
  3. Daemon continuously drives each managed fan from the selected temperature input within safe supported bounds for the chosen hardware mode.
  4. Daemon rejects configurations that would leave a managed fan without a usable temperature input or target temperature.
  5. User can inspect live temperatures, fan-control status, fault state, and tuned PID values, and can trigger basic PID auto-tuning through DBus-backed CLI flows.
**Plans**: TBD

### Phase 4: KDE GUI & Tray Experience
**Goal**: Users can monitor and configure KDE Fan Control from a native KDE/Qt6/QML interface and system tray without bypassing the daemon.
**Depends on**: Phase 3
**Requirements**: GUI-01, GUI-02, GUI-03, GUI-04, GUI-05
**Success Criteria** (what must be TRUE):
  1. User can view discovered sensors, fans, support state, and current runtime status in a KDE-native Qt6/QML GUI.
  2. User can configure fan enrollment, temperature inputs, aggregation function, target temperature, control mode, and PID settings from the GUI.
  3. User can trigger PID auto-tuning from the GUI and review the resulting settings for the managed fan.
  4. User can use the system tray to inspect current status and quickly distinguish unmanaged fans, daemon-controlled fans, and unsupported or degraded hardware.
**Plans**: TBD
**UI hint**: yes

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Hardware Inventory & Visibility | 0/TBD | Not started | - |
| 2. Safe Enrollment & Lifecycle Recovery | 0/TBD | Not started | - |
| 3. Temperature Control & Runtime Operations | 0/TBD | Not started | - |
| 4. KDE GUI & Tray Experience | 0/TBD | Not started | - |
