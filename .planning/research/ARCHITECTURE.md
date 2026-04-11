# Architecture Patterns

**Domain:** Linux desktop fan-control system with root daemon, D-Bus IPC, systemd integration, hwmon/sysfs discovery, and Qt6/QML GUI  
**Researched:** 2026-04-10  
**Confidence:** HIGH for systemd/D-Bus/hwmon/Qt boundaries; MEDIUM for hardware edge-case handling across diverse boards

## Recommended Architecture

Use a **single authoritative root daemon** with four internal layers:

1. **Hardware adapter layer** — discovers hwmon/sysfs capabilities and performs validated reads/writes.
2. **Control core** — owns enrollment, policy validation, sensor aggregation, PID loops, and fail-safe decisions.
3. **Service/API layer** — exposes a versioned D-Bus API, authorization checks, config transactions, and runtime events.
4. **Persistence/runtime integration layer** — stores the single active config, integrates with systemd lifecycle, and restores safe state on boot/shutdown.

All GUI, tray, and CLI clients should be **unprivileged D-Bus clients only**. They must never write sysfs directly and must never edit daemon-owned config files.

## High-Level Structure

```text
Qt6/QML GUI ─┐
Tray client  ├── system bus D-Bus API ──> Root daemon
Rust CLI     ─┘                             │
                                             ├─ Config store
                                             ├─ Runtime state store
                                             ├─ Control engine
                                             ├─ Discovery/inventory engine
                                             └─ Sysfs/hwmon adapter
                                                    │
                                                    └─ /sys/class/hwmon, related sysfs nodes

systemd supervises daemon lifecycle, restart, watchdog, and boot ordering.
polkit gates privileged mutating operations.
```

## Component Boundaries

| Component | Responsibility | Communicates With |
|-----------|----------------|-------------------|
| Qt6/QML GUI | Visualizes inventory, config, and live telemetry; submits user intents | D-Bus client adapter only |
| Tray/UI helpers | Quick status, notifications, shortcuts | D-Bus client adapter only |
| CLI | Scriptable inspection and configuration | D-Bus client adapter only |
| D-Bus client SDK layer | Typed request/reply API, signal subscriptions, DTO mapping | Daemon D-Bus API |
| Authz boundary | polkit checks for enroll/apply/override/write operations | D-Bus service layer |
| D-Bus service layer | Versioned objects, properties, methods, signals; transaction entrypoint | Control core, persistence |
| Config transaction manager | Validates edits, computes diff, applies atomically, rolls back on failure | D-Bus layer, persistence, control core |
| Inventory/discovery engine | Enumerates hwmon/sysfs devices, normalizes capabilities, tracks hotplug/reload | Sysfs adapter, runtime state |
| Capability classifier | Decides controllable vs read-only vs unsafe/unsupported | Inventory engine, control core |
| Control core | Enrollment ownership, PID loop scheduling, aggregation, safety state machine | Inventory, config, sysfs adapter |
| Sensor aggregation engine | Computes avg/max/min/median and health of source groups | Control core, runtime state |
| Fan actuator layer | Converts target output to hardware writes with clamps and mode checks | Control core, sysfs adapter |
| Fail-safe manager | Drives controlled fans to safe maximum on crash path, shutdown path, invalid state, or lost control confidence | Control core, systemd hooks, sysfs adapter |
| Persistence layer | Stores single active config and last-known enrolled set | Config manager |
| Sysfs/hwmon adapter | Raw file I/O, capability probing, unit parsing, write verification | Kernel hwmon/sysfs |
| systemd unit integration | Service startup ordering, restart policy, watchdog, stop/restart hooks | Daemon process |

## Internal Daemon Modules

### 1. Inventory Model
- Discover from `/sys/class/hwmon/hwmon*` first.
- Resolve stable identity from a combination of hwmon path, device symlink target, chip name, labels, and channel names.
- Keep a distinction between:
  - **Physical device**
  - **Sensor channel**
  - **Fan tach channel**
  - **Controllable output channel**
  - **Logical user alias**

**Why:** kernel hwmon naming is standardized, but board wiring/labels are not; user-space must handle labeling and interpretation.

### 2. Capability/Safety Classifier
Each candidate controllable fan should be classified into one of:

- **Controllable + safe-max available**
- **Controllable but unsafe enrollment**
- **Readable only**
- **Unsupported/ambiguous**

Unsafe enrollment should include cases like:
- writable output exists but no reliable path to force maximum output
- control node behavior is unclear or inconsistent after probe
- fan/sensor mapping confidence is too low

### 3. Control Core
Use one control task per enrolled fan, but do **not** let each task own hardware directly. Instead:

- task computes desired output
- actuator layer validates/clamps
- shared hardware executor serializes writes per chip/channel

This prevents race conditions between concurrent loops, manual override, startup restoration, and emergency fallback.

### 4. Config Transaction Manager
Config changes should be staged and applied as a transaction:

1. validate referenced sensors/fans still exist
2. validate capability/safety invariants
3. precompute required mode switches
4. persist candidate config
5. apply runtime changes
6. verify hardware accepted state
7. commit active config or roll back

Never mutate the live control graph incrementally from arbitrary UI edits.

### 5. Runtime State Store
Separate:

- **Persistent config**: user intent
- **Derived runtime plan**: resolved sensors, fan mappings, loop parameters
- **Ephemeral telemetry**: current temps, RPM, PWM, alarms, faults
- **Safety state**: normal / degraded / fallback / emergency

This separation is the core maintainability seam.

## D-Bus API Shape

Use a **versioned object tree** and standard D-Bus interfaces where appropriate.

### Suggested Object Model

```text
/com/example/KDEFanControl1
  ├─ Manager
  ├─ Inventory
  ├─ Config
  ├─ Runtime
  ├─ Fans/<id>
  ├─ Sensors/<id>
  └─ Groups/<id>
```

### Interface Guidance
- Use a versioned bus/interface namespace, e.g. `com.example.KDEFanControl1.*`.
- Do **not** use `/` as the main object path.
- Implement `org.freedesktop.DBus.ObjectManager` for inventory/runtime objects.
- Use properties for current state, methods for mutations, and signals for change events.
- Keep large telemetry snapshots as pull-based reads; use signals for deltas/state transitions.

### API Split
- **Read-only methods/properties**: inventory, capability, telemetry, effective config, service health.
- **Privileged mutations**: enroll/unenroll fan, apply config, switch control mode, force fallback, rename aliases, tune PID.
- **Operational controls**: reload hardware inventory, reload config, reset failed state, request re-probe.

### Authorization
Gate mutating methods with **polkit**. Treat GUI/CLI callers as untrusted subjects. Read-only calls can often be open; write paths should require explicit authorization.

## Data Flow

### 1. Boot / Service Start

```text
systemd starts daemon
  → daemon initializes logging, watchdog, D-Bus name
  → inventory engine scans hwmon/sysfs
  → capability classifier marks channels safe/unsafe
  → persistence loads active config
  → config transaction manager resolves config against current hardware
  → control core enrolls only safe, valid fans
  → PID loops start
  → runtime state published on D-Bus
```

If config cannot be fully restored, the daemon should enter **degraded mode**, keep unsafe fans unmanaged, and surface explicit reasons to clients.

### 2. Telemetry Loop

```text
sysfs reads
  → normalization/parsing
  → runtime telemetry store
  → aggregation engine recomputes logical sources
  → control tasks consume effective temperatures
  → D-Bus properties/signals publish state
  → GUI/CLI render state
```

### 3. Control Loop

```text
effective sensor input
  → PID calculation
  → clamp/rate-limit/sanity check
  → actuator write request
  → write verification/readback
  → runtime state update
  → fallback if write/readback invalid
```

### 4. Config Change Flow

```text
GUI/CLI mutation request
  → polkit authorization
  → D-Bus service validates DTO
  → config transaction manager stages change
  → inventory/capability cross-check
  → runtime apply
  → persistence commit
  → signals emitted to clients
```

### 5. Failure Flow

```text
hardware write fails / sensor disappears / daemon unhealthy / watchdog miss
  → control core marks fan/channel degraded
  → fail-safe manager forces safe-max for previously daemon-controlled fans
  → runtime safety state updated
  → signal + journal entry emitted
  → systemd may restart daemon
  → daemon re-discovers and re-validates before re-enrolling
```

## Safety-Critical Seams and Failure Boundaries

### Seam 1: Unprivileged clients ↔ root daemon
**Rule:** only intent crosses this boundary, never raw file paths or arbitrary sysfs write requests.  
**Why:** prevents the GUI/CLI from becoming privileged hardware editors.

### Seam 2: D-Bus API ↔ config transaction manager
**Rule:** all mutating API calls become validated domain commands.  
**Why:** prevents half-applied state and UI-driven invariants leakage.

### Seam 3: Control core ↔ sysfs adapter
**Rule:** control code asks for semantic operations (`set_manual_pwm(channel, value)`, `force_safe_max(fan)`), not raw file writes.  
**Why:** keeps hardware quirks isolated.

### Seam 4: Discovery ↔ stable domain identities
**Rule:** never use raw `hwmonN` names as persisted IDs.  
**Why:** hwmon numbering can change across boots.

### Seam 5: Runtime loop ↔ persistence
**Rule:** telemetry is never persisted as config; config is never inferred from transient state.  
**Why:** avoids split-brain and stale recovery behavior.

### Failure Boundary: Single fan enrollment
If one enrolled fan becomes invalid, degrade that fan first; do not crash the whole daemon unless the shared hardware path is corrupted.

### Failure Boundary: Shared chip adapter
If a chip-level adapter becomes inconsistent, drop all managed channels on that chip to safe-max and mark the chip unavailable.

### Failure Boundary: Whole daemon health
If the daemon loses event loop health, misses watchdog deadlines, or panics, systemd should kill/restart it; shutdown hooks should attempt safe-max first.

## systemd Integration Pattern

Recommend a **system service** supervised by systemd with:

- `Type=dbus` **or** `Type=notify`
- a stable `BusName=` if using `Type=dbus`
- `Restart=on-failure`
- `WatchdogSec=` with daemon keep-alives if using `notify`
- explicit startup timeout
- explicit stop timeout
- hardened execution settings where compatible with hwmon access

### Recommendation
Use **`Type=dbus` if D-Bus name acquisition is the true readiness signal** and the daemon API is central to consumers. Use **`Type=notify`** only if daemon readiness depends on more than bus ownership, such as completed inventory scan and successful safe config restoration.

For this project, **`Type=notify` is slightly better** because “daemon is ready” should mean more than “name acquired”; it should mean discovery completed and the initial safety state is known.

### Lifecycle Notes
- Startup should not report ready until discovery + config reconciliation finish.
- Stop/restart hooks should attempt a safe fallback for daemon-controlled fans.
- Watchdog failure should be treated as thermal-safety relevant, not just availability related.

## Qt6/QML Client Architecture

Use **QML for presentation only** and **C++/Rust-backed client models for state**.

### Recommended Client Layers

| Layer | Responsibility |
|------|----------------|
| QML views | Screens, dialogs, graphs, tray popups |
| View-model/model layer | Exposes typed models/properties to QML |
| D-Bus client adapter | Marshals D-Bus calls/signals to typed objects |
| Domain DTOs | Fan, sensor, config, runtime health, alerts |

### Rules
- No business logic in QML.
- No direct D-Bus calls from random QML components.
- Use `QAbstractListModel`/typed QObject models for inventory and telemetry.
- Batch updates from D-Bus signals before notifying QML when possible.

## Patterns to Follow

### Pattern 1: Inventory Snapshot + Delta Stream
**What:** full snapshot on connect, then signals for changes.  
**When:** GUI startup, tray resume, CLI watch mode.  
**Why:** simpler than reconstructing full state from signals alone.

### Pattern 2: Command/Query Separation
**What:** methods that mutate return operation result; reads come from properties/snapshots.  
**When:** config apply, enroll/unenroll, emergency override.  
**Why:** keeps D-Bus contract predictable.

### Pattern 3: Domain Commands, Not File Semantics
**What:** API exposes `EnrollFan`, `ApplyConfig`, `SetManualOverride`, not `WriteSysfs(path, value)`.  
**Why:** protects safety invariants.

### Pattern 4: Per-Fan State Machine
**What:** `bios`, `unmanaged-readable`, `managed-healthy`, `managed-degraded`, `fallback-max`, `error`.  
**Why:** safety behavior becomes explicit and UI-friendly.

### Pattern 5: Atomic Apply
**What:** config changes transition through staged → validated → active or rejected.  
**Why:** avoids partial application during hardware variability.

## Anti-Patterns to Avoid

### Anti-Pattern 1: GUI-owned config files
**Why bad:** creates split-brain with daemon runtime.

### Anti-Pattern 2: Persisting raw `hwmonN/pwm1` identifiers
**Why bad:** numbering can change across boots/kernel updates.

### Anti-Pattern 3: Letting each PID loop write sysfs independently
**Why bad:** races manual override, restore, fallback, and mode switching.

### Anti-Pattern 4: Treating “writable” as “safe to enroll”
**Why bad:** safety depends on reliable safe-max/fallback behavior, not merely write permission.

### Anti-Pattern 5: Making DBus readiness equal hardware readiness without reconciliation
**Why bad:** clients may assume active management before the daemon has validated the machine state.

## Suggested Build Order

1. **Sysfs/hwmon adapter + inventory model**
   - Needed first because every later layer depends on correct discovery and normalized capability data.

2. **Capability classifier + stable identity scheme**
   - Establish safe enrollment rules before any control logic exists.

3. **Read-only D-Bus inventory/telemetry API**
   - Lets CLI/GUI develop against real shapes early without exposing writes.

4. **Persistence + config transaction manager**
   - Needed before enabling long-lived managed state.

5. **Control core with simulated/test actuator backend**
   - Prove scheduling, PID semantics, and safety state machine without risking hardware.

6. **Real actuator writes + fail-safe manager**
   - Only after safety paths and rollback behavior are tested.

7. **systemd lifecycle + watchdog integration**
   - Turn runtime supervision into a first-class feature before shipping.

8. **Qt6/QML GUI and tray on top of stable D-Bus contracts**
   - Build after API/read models stabilize.

## Build Order Implications for Roadmap

- **Phase 1 should be inventory-first, not GUI-first.** The hardware model is the foundation.
- **Phase 2 should add safe read-only observability** through D-Bus + CLI before control writes.
- **Phase 3 should add config transactions and enrollment validation**.
- **Phase 4 should add live control and fallback behavior**.
- **Phase 5 should add polished GUI/tray UX and tuning workflows**.

## Testability Strategy

- Put sysfs access behind a trait/interface so fake hwmon trees can be mounted in tests.
- Test config reconciliation against disappearing/renumbered channels.
- Test fail-safe transitions with injected write failures and watchdog expiry.
- Test D-Bus API contract with read-only and privileged callers.
- Test GUI against a mock D-Bus service before connecting to real hardware.

## Scalability Considerations

| Concern | At 1 machine | At many supported boards | At distro/package scale |
|---------|--------------|--------------------------|-------------------------|
| Hardware quirks | Manual handling acceptable | Need quirk tables/capability rules | Need telemetry/logging for field diagnosis |
| API evolution | Simple versioning | Keep stable D-Bus namespace | Support additive interface changes only |
| Control concurrency | Single async runtime fine | Per-chip serialization becomes important | Regression suite becomes critical |
| UI complexity | Single window works | Need clearer state grouping | Need strong compatibility guarantees |

## Sources

- Linux kernel hwmon sysfs interface: https://docs.kernel.org/hwmon/sysfs-interface.html — HIGH
- D-Bus specification, standard interfaces/ObjectManager/path/interface naming: https://dbus.freedesktop.org/doc/dbus-specification.html — HIGH
- systemd service semantics (`Type=dbus`, `Type=notify`, restart/watchdog/ExecStopPost): https://man7.org/linux/man-pages/man5/systemd.service.5.html — HIGH
- systemd unit guidance on dependencies and load paths: https://man7.org/linux/man-pages/man5/systemd.unit.5.html — HIGH
- systemd D-Bus API and polkit usage in privileged operations: https://man7.org/linux/man-pages/man5/org.freedesktop.systemd1.5.html — HIGH
- polkit mechanism/subject/authorization architecture: https://polkit.pages.freedesktop.org/polkit/polkit.8.html — HIGH
- Qt 6 best practices for separating QML UI from C++ logic/models: https://doc.qt.io/qt-6.8/qtquick-bestpractices.html — HIGH
- Qt 6 model/view guidance for C++ models exposed to QML: https://doc.qt.io/qt-6.8/qtquick-modelviewsdata-cppmodels.html — HIGH
