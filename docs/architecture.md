# KDE Fan Control — Architecture

How the system's pieces fit together, why they're split this way, and what
invariants they maintain.

---

## 1. System Overview

```
 ┌─────────────────────────────────────────────────────────────┐
 │                      Unprivileged                            │
 │                                                             │
 │  ┌──────────┐    ┌──────────────────────────────────────┐   │
 │  │   CLI    │    │           GUI (C++/QML)              │   │
 │  │ (Rust)   │    │  ┌────────────┐  ┌────────────────┐  │   │
 │  │          │    │  │ C++ back-  │  │  QML pages /  │  │   │
 │  │ 16 cmds  │    │  │ end / DBus │  │  components    │  │   │
 │  │          │    │  │ proxy      │  │  (Kirigami)    │  │   │
 │  └────┬─────┘    │  └─────┬──────┘  └───────┬────────┘  │   │
 │       │          │        │                  │          │   │
 │       │  zbus    │     QDBusInterface      models       │   │
 │       │  proxy   └────────┼─────────────────┘          │   │
 └───────┼───────────────────┼──────────────────────────────┘
         │                   │
 ════════╪═══════════════════╪══════  system bus (DBus)
         │                   │
 ┌───────┼───────────────────┼──────────────────────────────┐
 │       ▼                   ▼           Privileged          │
 │  ┌─────────────────────────────────────────────────────┐ │
 │  │                 Daemon (Rust / root)                 │ │
 │  │                                                     │ │
 │  │  DBus server ──► config ◄──► control tasks          │ │
 │  │  (zbus)          (TOML)      (Tokio, per-fan PID)   │ │
 │  │                                                     │ │
 │  │  hwmon scanner ──► inventory                        │ │
 │  │  (sysfs /udev)                                      │ │
 │  └──────────────────────┬──────────────────────────────┘ │
 │                         │ writes                          │
 │                         ▼                                 │
 │              /sys/class/hwmon/hwmon*                      │
 └───────────────────────────────────────────────────────────┘

 Shared: core crate (inventory, config/validation, PID, lifecycle)
```

Three programs share one `core` crate and talk through the system bus:

| Component | Language | LOC | Runs as | Role |
|---|---|---|---|---|
| **Daemon** | Rust | ~2200 | root | Reads sysfs, runs PID loops, writes PWM, owns config, serves DBus |
| **CLI** | Rust | ~1500 | user | Thin DBus client; falls back to direct sysfs scan when daemon is down |
| **GUI** | C++/QML | ~5400 + ~5400 | user | KDE-native Kirigami app; models, pages, tray, notifications |
| **Core crate** | Rust | ~1400 | — | Shared types: inventory, config/validation, PID, lifecycle/ownership |

---

## 2. Privilege Boundary

```
  ┌─────────┐   DBus    ┌─────────┐   sysfs    ┌─────────┐
  │  CLI    │ ────────► │ Daemon  │ ────────► │  hwmon  │
  │  GUI    │  methods  │ (root)  │  writes    │  PWM    │
  └─────────┘           └─────────┘           └─────────┘
    unprivileged           root-only            kernel

  ✗ CLI / GUI never write sysfs directly
  ✗ No secondary control surface besides DBus
  ✓ Write methods currently require UID-0
     (polkit authorization planned)
```

The daemon is the only component with write access to fan-control sysfs
attributes. CLI and GUI are stateless clients that issue method calls over the
system bus. This keeps privilege escalation narrow and auditable: every PWM
write goes through one code path.

Current auth is UID-0 check. A polkit layer will replace it so that unprivileged
desktop users can manage fans without running the CLI as root.

---

## 3. State Ownership

The daemon is the sole source of truth. Clients render what the daemon reports.

| Asset | Owner | Location |
|---|---|---|
| Config file | Daemon | `$XDG_STATE_DIR/kde-fan-control/config.toml` |
| Owned-fan set | Daemon | in-memory, persisted in config |
| Control tasks | Daemon | per-fan Tokio tasks |
| Degraded-fan state | Daemon | in-memory + config |
| Event log | Daemon | in-memory ring buffer |

There is no split-brain risk. The CLI and GUI hold no persistent state of their
own — they call methods and display responses. A single config file means one
authoritative view and no merge conflicts.

---

## 4. Control Loop Internals

Each managed fan runs as a dedicated Tokio task with three independent
intervals, all defaulting to 250 ms:

```
  ┌─────────────── sample (250 ms) ──────────────┐
  │ read temp sensors → aggregate → update status│
  └────────────────────┬─────────────────────────┘
                       ▼
  ┌─────────────── control (250 ms) ─────────────┐
  │ PID(aggregate, target) → output %            │
  └────────────────────┬─────────────────────────┘
                       ▼
  ┌─────────────── write (250 ms) ──────────────┐
  │ map % → PWM, write sysfs                     │
  └──────────────────────────────────────────────┘
```

All three intervals use `MissedTickBehavior::Skip` — if the system is
overloaded, ticks are dropped rather than queued, preventing compounding
delays.

### PID controller

```
 error_mdeg  = aggregated_temp_mdeg - target_mdeg
 error_deg   = error_mdeg / 1000.0
 integral   += error_deg × dt
 integral    = clamp(integral, PidLimits.integral_min..max)
 derivative  = clamp(-delta_measurement / dt, -PidLimits.derivative_limit, +limit)
 output      = Kp × error_deg + Ki × integral + Kd × derivative
 output      = clamp(output, 0..100)
```

Key design choices:

- **Derivative-on-measurement** — uses the negative delta of the measured
  temperature, not the delta of the error. This gives bumpless transfer when
  the setpoint changes (no derivative spike on target adjustment).
- **Deadband** — if `|error| ≤ deadband`, hold the previous output. Prevents
  oscillation around the target.
- **Integral clamping** — the integral term is clamped to configured limits
  (`PidLimits`), preventing windup after sustained periods when the fan cannot
  reach the target temperature.

### PWM mapping

Logical 0–100 % is mapped to the fan's `pwm_min..pwm_max` sysfs range:

```
  pwm_value = pwm_min + (output% / 100) × (pwm_max - pwm_min)
```

**Startup kick** — when output transitions from 0 % to >0 %, the controller
writes `kick_percent` for `kick_ms` before switching to the calculated value.
This prevents fan stall on low-PWM startup, where some fans need a brief
pulse above their minimum stable speed.

---

## 5. Auto-Tune Design

Auto-tune produces a PID proposal the user must explicitly accept. It does
not modify a running fan.

### Procedure

1. **Full-power observation** — set fan to 100 % for an observation window
   (default 30 s).
2. **Sample** — record temperature readings over time.
3. **Derive parameters** from the step response:
   - **Lag time** — time to first 0.5 °C temperature change after applying
     full power.
   - **Max cooling rate** — steepest observed temperature drop.
4. **Ziegler-Nichols-inspired formulas** convert lag and rate into tentative
   Kp, Ki, Kd.
5. **Softening factors** are applied for safety margin:
   - `Kp × 0.6`
   - `Ki × 0.5`
   - `Kd × 0.75`

The resulting proposal is **review-only**. The user must accept it into the
draft config, then apply the draft — it never bypasses the normal config
lifecycle.

---

## 6. Configuration Lifecycle

```
  ┌─────────┐     ┌──────────┐     ┌───────────┐
  │  Draft  │────►│ Validate │────►│   Apply   │
  │ (staged)│     │ (dry-run)│     │ (live)    │
  └─────────┘     └────┬─────┘     └─────┬─────┘
                       │                 │
                  pass / fail      claim + persist
                  (no state           + start tasks
                   change)           + report rejected
```

**Draft** — the user builds a desired configuration. Nothing is live.

**Validate** — check the draft against current hardware without any state
change. Returns per-fan pass/fail with reasons.

**Apply** — promote passing fans, report rejected fans, persist the config,
claim fan ownership, and start control tasks. This is the only step that
changes live state.

Key invariants:

- **Partial apply is normal.** Valid fans go live; invalid ones stay in the
  draft with rejection reasons. A single bad sensor doesn't block the rest.
- **Apply is additive.** Previously applied fans that are absent from the
  draft are preserved — apply only adds or updates, it doesn't remove.
- **Backward-compatible deserialization.** All fields introduced after Phase 1
  use `serde(default)` so that older configs load cleanly.

---

## 7. Boot Reconciliation

When the daemon starts (or restarts), it must reconcile its persisted config
with the hardware that's actually present:

```
  1. Discover hardware     ──►  /sys/class/hwmon/hwmon* scan
  2. Load persisted config ──►  config.toml
  3. For each applied fan:
       ├─ fan still exists?       ──►  no  → degraded
       ├─ fan still available?    ──►  no  → degraded
       ├─ control mode supported?──►  no  → degraded
       └─ temp sources present?  ──►  no  → degraded
  4. Claim passing fans, mark failing ones as degraded
  5. Persist reconciled config (valid subset only)
  6. Start control loops for restored fans
```

Degraded fans are reported over DBus so the GUI can surface them. The daemon
never automatically removes a fan from the config — it marks it degraded and
lets the user decide.

---

## 8. DBus Interface Structure

Three interfaces on the system bus under a shared object path namespace:

```
 org.kde.FanControl
 ├── /org/kde/FanControl
 │   ├── org.kde.FanControl.Inventory
 │   │   ├── DiscoverHardware()     → full hw tree
 │   │   ├── GetFans() / GetSensors()
 │   │   └── SetName() / ClearName()
 │   │
 │   ├── org.kde.FanControl.Lifecycle
 │   │   ├── GetDraft() / SetDraft() / ClearDraft()
 │   │   ├── ValidateDraft()        → per-fan check
 │   │   ├── ApplyDraft()           → promote + persist + claim
 │   │   ├── GetAppliedFans() / GetDegradedFans()
 │   │   ├── GetEvents()           → event log
 │   │   └── GetRuntimeState()
 │   │
 │   └── org.kde.FanControl.Control
 │       ├── GetControlStatus()     → per-fan PID state
 │       ├── StartAutoTune() / StopAutoTune() / GetAutoTuneResult()
 │       └── EditProfile()
```

- **Inventory** — read hardware, manage human-readable names.
- **Lifecycle** — draft/apply flow, degraded state, events, runtime state.
- **Control** — live control status, auto-tune, profile editing.

All methods are on the system bus. Clients never need to know about the
daemon's internals — the DBus contract is the stable API.

---

## 9. GUI Architecture

```
  ┌─────────────────────────────────────────────────┐
  │                  QML layer                       │
  │                                                 │
  │  ┌──────────┐ ┌───────────┐ ┌───────────────┐ │
  │  │ Overview  │ │ Inventory │ │  FanDetail    │ │
  │  │  Page    │ │   Page   │ │  Page (tabs)  │ │
  │  │ (fast    │ │ (legacy   │ │ (legacy       │ │
  │  │  path)   │ │  path)   │ │  path)        │ │
  │  └──────────┘ └───────────┘ └───────────────┘ │
  │  ┌──────────┐ ┌──────────────────────────────┐ │
  │  │  Wizard  │ │  Tray Popover / Notification │ │
  │  │ Dialog   │ │  (structural path only)      │ │
  │  │ (7-step) │ │                              │ │
  │  └──────────┘ └──────────────────────────────┘ │
  │                                                 │
  │  Components: StateBadge, OutputBar,             │
  │  TemperatureDisplay, PidField, RenameDialog    │
  └──────────┬──────────────────────────────────────┘
             │ reads / writes
  ┌──────────┴──────────────────────────────────────┐
  │              C++ backend                         │
  │                                                 │
  │  ┌──────────────┐  ┌──────────────────────┐    │
  │  │ DaemonInter- │  │    StatusMonitor      │    │
  │  │ face (QDBus) │  │  Overview path:       │    │
  │  │              │  │   telemetry 100 ms    │    │
  │  └──────┬───────┘  │   structure 2000 ms   │    │
  │         │          │  Legacy path:          │    │
  │         │          │   coalesced 250 ms    │    │
  │         │          └──────────┬───────────┘    │
  │         │                     │                 │
  │  ┌──────┴─────────────────────┴──────────────┐ │
  │  │              Model classes                 │ │
  │  │  OverviewModel    (fast telemetry + rare   │ │
  │  │                     structural split)      │ │
  │  │  OverviewFanRow   (25-property stable row) │ │
  │  │  FanListModel      (severity-sorted,       │ │
  │  │                     diff-updated, legacy)   │ │
  │  │  SensorListModel   (legacy)               │ │
  │  │  DraftModel        (edit buffer)           │ │
  │  │  LifecycleEventModel                      │ │
  │  └───────────────────────────────────────────┘ │
  └─────────────────────────────────────────────────┘
```

**DaemonInterface** — QDBusInterface proxy wrapping the three DBus interfaces.
Calls are async; signals are forwarded to the QML layer via Qt signal/slot.

**StatusMonitor** — dual-path refresh scheduler:

- **Overview telemetry path**: 100 ms timer calls `GetOverviewTelemetry()`.
  Results go to `OverviewModel::applyTelemetry()` which sets per-property
  values on `OverviewFanRow` objects. No model-level `dataChanged` is emitted
  unless `visual_state` or `high_temp_alert` transitions occur (those signals
  are needed by `TrayIcon` and `NotificationHandler`).

- **Overview structural path**: 2000 ms cooldown-gated timer calls
  `GetOverviewStructure()`. Results go to `OverviewModel::applyStructure()`
  which may add/remove/reorder rows. Force-refresh triggers (bypassing
  cooldown) fire on: daemon reconnect, write mutations, auto-tune completion,
  and QML page-visibility changes.

- **Legacy path**: 250 ms timer calls `Snapshot()`, `GetRuntimeState()`,
  `GetControlStatus()`, `GetDraftConfig()`, `GetDegradedSummary()`.
  Responses are coalesced into `FanListModel::refresh()` and
  `SensorListModel::refresh()`. Used by `FanDetailPage`, `InventoryPage`,
  and `WizardDialog`.

**Models** — six model/object classes exposed to QML:
- `OverviewModel` — purpose-built overview list with split structure/telemetry
  paths. Exposes `RowObjectRole` returning `OverviewFanRow*` for direct QML
  property binding without model-level `dataChanged` cascades.
- `OverviewFanRow` — 25-property QObject per fan row, split into structural
  (13) and telemetry (12) groups with per-property NOTIFY signals.
- `FanListModel` — severity-sorted, diff-updated. Legacy path for detail
  pages, inventory, and wizard.
- `SensorListModel` — hardware sensors from inventory.
- `DraftModel` — edit buffer for the draft/apply flow.
- `LifecycleEventModel` — event log entries.

**TrayIcon and NotificationHandler** read from `OverviewModel` (not
`FanListModel`) and connect only to structural-model signals
(`modelReset`, `rowsInserted`, `rowsRemoved`, `dataChanged`), decoupling
them from the 100 ms telemetry churn.

**Pages and components**:
- `OverviewPage` — dashboard with live fan status. Reads from `OverviewModel`
  via `rowObject` binding for surgical per-property QML updates. Fixed-width
  monospace layout for rapidly changing numeric fields (temperature, RPM,
  output).
- `InventoryPage` — hardware browser. Reads from `FanListModel` (legacy path).
- `FanDetailPage` — tabbed view (config / auto-tune / advanced). Reads from
  `FanListModel` via `fanById()` (legacy path).
- `WizardDialog` — 7-step fan enrollment flow. Reads from `FanListModel` (legacy path).
- `TrayIcon` (KStatusNotifierItem), `NotificationHandler` (KNotification),
  `TrayPopover`.

---

## 10. Known Technical Debt

| Area | Issue | Impact |
|---|---|---|
| OverviewModel | `applyStructure()` uses `beginResetModel()` instead of `beginMoveRows` | Full model rebuild on structural changes; acceptable because structural refreshes are rare (~2s cooldown-gated) |
| Build system | KF6 dev packages need proper CMake `find_package` support | Fragile builds on some distros |
| GUI navigation | Some tray→main-window and popover integration stubs | Incomplete shell interaction |
| FanDetailPage | Advanced tab values are hardcoded | Not reflecting live state |
| Lifecycle events | Events refresh only on page load | Stale event list until user navigates away and back |
| Authorization | UID-0 check instead of polkit | Desktop users must run CLI as root; GUI writes blocked for non-root |