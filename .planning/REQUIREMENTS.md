# Requirements: KDE Fan Control

**Defined:** 2026-04-10
**Core Value:** Users can safely and flexibly control desktop fan behavior with understandable per-fan PID policies, without losing fail-safe behavior.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Hardware Discovery

- [ ] **HW-01**: User can list detected temperature sensors exposed by supported Linux hardware interfaces
- [ ] **HW-02**: User can list detected fan controllers and fan channels exposed by supported Linux hardware interfaces
- [ ] **HW-03**: User can inspect capabilities for each fan channel, including whether PWM control or voltage control is available
- [ ] **HW-04**: User can assign and persist a friendly name for a detected temperature sensor
- [ ] **HW-05**: User can assign and persist a friendly name for a detected fan controller or fan channel
- [ ] **HW-06**: User can see whether each discovered fan or control endpoint is `available`, `partial`, or `unavailable`
- [ ] **HW-07**: User can inspect the reason a discovered endpoint is partially supported or unavailable
- [ ] **HW-08**: User can inspect whether a fan channel exposes tach or RPM feedback

### Fan Enrollment And Modes

- [ ] **FAN-01**: User can leave a detected fan under BIOS or existing system control without daemon interference
- [ ] **FAN-02**: User can enroll a detected fan for daemon-managed control when safe managed control is supported
- [ ] **FAN-03**: User cannot enroll hardware that lacks enough control or fallback support for safe daemon management
- [ ] **FAN-04**: User can choose the hardware control mode used by the daemon when the hardware exposes multiple modes such as PWM or voltage
- [ ] **FAN-05**: User can view whether a fan is unmanaged, managed, fallback, partial, or unavailable
- [ ] **FAN-06**: User can reboot the system and have previously managed fans resume daemon control automatically from persisted configuration

### Sensor Sources And Aggregation

- [ ] **SNS-01**: User can select a single temperature sensor as the control input for a fan
- [ ] **SNS-02**: User can select multiple temperature sensors as the control input group for a fan
- [ ] **SNS-03**: User can choose `average` as the aggregation function for a multi-sensor group
- [ ] **SNS-04**: User can choose `max` as the aggregation function for a multi-sensor group
- [ ] **SNS-05**: User can choose `min` as the aggregation function for a multi-sensor group
- [ ] **SNS-06**: User can choose `median` as the aggregation function for a multi-sensor group

### PID Control

- [ ] **PID-01**: User can set a target temperature for each managed fan
- [ ] **PID-02**: User can configure P, I, and D gains for each managed fan
- [ ] **PID-03**: Daemon computes fan output continuously from the selected sensor input or aggregation and target temperature
- [ ] **PID-04**: Daemon applies output within safe supported bounds for the selected hardware control mode
- [ ] **PID-05**: User can enable basic PID auto-tuning for a managed fan
- [ ] **PID-06**: User can inspect the resulting tuned P, I, and D values after auto-tuning
- [ ] **PID-07**: User can understand that v1 managed control is thermal-control PID based on temperature input, not RPM-target tracking

### Safety And Runtime Behavior

- [ ] **SAFE-01**: Daemon restores or drives previously daemon-controlled fans to high speed when the service fails or exits unexpectedly
- [ ] **SAFE-02**: Daemon does not modify unmanaged fans during normal operation, startup, shutdown, or failure handling
- [ ] **SAFE-03**: User can inspect whether a fan is currently in a safe fallback state
- [ ] **SAFE-04**: Daemon rejects invalid configurations that would leave a managed fan without a usable temperature input or target
- [ ] **SAFE-05**: User can enroll a fan with writable control even if RPM feedback is unavailable
- [ ] **SAFE-06**: Daemon safety logic does not depend on tach presence to force safe-maximum output for managed fans
- [ ] **SAFE-07**: If startup cannot safely apply persisted control to a managed fan, the daemon surfaces a degraded state instead of silently assuming success

### DBus API

- [ ] **BUS-01**: User-space clients can query discovered hardware and capabilities over DBus
- [ ] **BUS-02**: User-space clients can create, update, and delete fan-control configuration over DBus
- [ ] **BUS-03**: User-space clients can read current runtime status for sensors, fans, and control policies over DBus
- [ ] **BUS-04**: User-space clients can trigger persistence of configuration through the daemon over DBus
- [ ] **BUS-05**: User-space clients can start auto-tuning through DBus
- [ ] **BUS-06**: DBus clients observe the daemon as the sole authority for discovery, persistence, and runtime state

### Persistence

- [ ] **CONF-01**: Daemon persists exactly one active configuration in v1
- [ ] **CONF-02**: Persisted configuration stores friendly names, enrollment state, selected sensor inputs, aggregation settings, control mode, target temperature, and PID parameters
- [ ] **CONF-03**: Persisted configuration is validated against current discovered hardware before control is resumed on boot

### CLI

- [ ] **CLI-01**: User can list sensors, fans, capabilities, and support state from the CLI
- [ ] **CLI-02**: User can configure friendly names, fan enrollment, sensor inputs, aggregation function, and PID settings from the CLI
- [ ] **CLI-03**: User can inspect current fan-control status, active temperatures, and fault state from the CLI
- [ ] **CLI-04**: User can trigger PID auto-tuning from the CLI

### KDE GUI

- [ ] **GUI-01**: User can view discovered sensors, fans, support state, and current status in a Qt6 or QML GUI
- [ ] **GUI-02**: User can configure fan enrollment, temperature inputs, aggregation function, target temperature, control mode, and PID settings in the GUI
- [ ] **GUI-03**: User can trigger auto-tuning from the GUI
- [ ] **GUI-04**: User can access current status from a system tray icon
- [ ] **GUI-05**: User can recognize unmanaged fans versus daemon-controlled fans and unsupported hardware in the GUI

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Profiles

- **PROF-01**: User can store multiple named fan-control profiles
- **PROF-02**: User can switch between profiles safely

### Advanced Control

- **CTRL-01**: User can define additional statistical aggregation functions beyond average, max, min, and median
- **CTRL-02**: User can define weighted sensor combinations
- **CTRL-03**: User can use adaptive or fuzzy tuning strategies when proven safe on supported hardware

### Observability

- **OBS-01**: User can view historical sensor and fan graphs
- **OBS-02**: User can export diagnostic snapshots for troubleshooting

### Compatibility Expansion

- **COMP-01**: User can use richer vendor-specific integrations for supported hardware families
- **COMP-02**: User can receive compatibility guidance for unsupported or partially supported controllers

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Web UI | Local desktop management is the priority |
| Non-KDE polished parity in v1 | KDE and Qt6-first reduces UI surface area and design overhead |
| Remote multi-machine control | Not required for the desktop local-control use case |
| User-authored custom formulas or plugins in v1 | Adds complexity before core control and safety are proven |
| GPU, AIO, and embedded-controller breadth-first support | v1 should focus on a trustworthy generic hwmon or sysfs desktop path |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| HW-01 | Phase 1 | Pending |
| HW-02 | Phase 1 | Pending |
| HW-03 | Phase 1 | Pending |
| HW-04 | Phase 1 | Pending |
| HW-05 | Phase 1 | Pending |
| HW-06 | Phase 1 | Pending |
| HW-07 | Phase 1 | Pending |
| HW-08 | Phase 1 | Pending |
| FAN-01 | Phase 2 | Pending |
| FAN-02 | Phase 2 | Pending |
| FAN-03 | Phase 2 | Pending |
| FAN-04 | Phase 2 | Pending |
| FAN-05 | Phase 2 | Pending |
| FAN-06 | Phase 2 | Pending |
| SNS-01 | Phase 3 | Pending |
| SNS-02 | Phase 3 | Pending |
| SNS-03 | Phase 3 | Pending |
| SNS-04 | Phase 3 | Pending |
| SNS-05 | Phase 3 | Pending |
| SNS-06 | Phase 3 | Pending |
| PID-01 | Phase 3 | Pending |
| PID-02 | Phase 3 | Pending |
| PID-03 | Phase 3 | Pending |
| PID-04 | Phase 3 | Pending |
| PID-05 | Phase 3 | Pending |
| PID-06 | Phase 3 | Pending |
| PID-07 | Phase 3 | Pending |
| SAFE-01 | Phase 2 | Pending |
| SAFE-02 | Phase 2 | Pending |
| SAFE-03 | Phase 2 | Pending |
| SAFE-04 | Phase 3 | Pending |
| SAFE-05 | Phase 2 | Pending |
| SAFE-06 | Phase 2 | Pending |
| SAFE-07 | Phase 2 | Pending |
| BUS-01 | Phase 1 | Pending |
| BUS-02 | Phase 2 | Pending |
| BUS-03 | Phase 3 | Pending |
| BUS-04 | Phase 2 | Pending |
| BUS-05 | Phase 3 | Pending |
| BUS-06 | Phase 2 | Pending |
| CONF-01 | Phase 2 | Pending |
| CONF-02 | Phase 2 | Pending |
| CONF-03 | Phase 2 | Pending |
| CLI-01 | Phase 1 | Pending |
| CLI-02 | Phase 2 | Pending |
| CLI-03 | Phase 3 | Pending |
| CLI-04 | Phase 3 | Pending |
| GUI-01 | Phase 4 | Pending |
| GUI-02 | Phase 4 | Pending |
| GUI-03 | Phase 4 | Pending |
| GUI-04 | Phase 4 | Pending |
| GUI-05 | Phase 4 | Pending |

**Coverage:**
- v1 requirements: 52 total
- Mapped to phases: 52
- Unmapped: 0 ✓

---
*Requirements defined: 2026-04-10*
*Last updated: 2026-04-10 after roadmap creation*
