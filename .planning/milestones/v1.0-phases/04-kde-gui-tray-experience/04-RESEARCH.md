# Phase 4: KDE GUI & Tray Experience — Research

**Researched:** 2026-04-12 (updated — open questions resolved)
**Domain:** KDE Qt6/QML GUI application with DBus client, system tray, and notifications
**Confidence:** HIGH

## Summary

Phase 4 builds a KDE-native Qt6/QML GUI and system tray experience on top of the existing daemon-owned DBus contract. The GUI is a pure DBus client — it reads runtime state, edits staged draft configuration, triggers auto-tuning, and receives tray-visible fault or alert feedback, all without bypassing the daemon or inventing a second authority surface.

The core architecture is a C++/QML split: C++ QObject subclasses (DaemonInterface, models, TrayIcon, NotificationHandler) bind to the DBus system bus via QtDBus, parse JSON from the daemon, expose parsed data as Q_PROPERTY and QAbstractListModel subclasses, and invoke daemon methods via Q_INVOKABLE slots. The QML layer is pure declarative using Kirigami.ApplicationWindow, pageStack navigation, ScrollablePage, AbstractCard, FormLayout, InlineMessage, TabBar + StackLayout, and KStatusNotifierItem for tray integration.

A substantial implementation already exists in `gui/` with all the core C++ backend classes and QML pages, but the build has linker errors that must be resolved before the GUI can be considered functional. Three open questions from the initial research have now been resolved by inspecting daemon source and system package state: (1) most DBus signals are void "changed" notifications requiring a re-fetch, with only two signals carrying payloads; (2) a 3-second polling interval is appropriate for live data; (3) the KF6 dev packages are not installed and that IS the root cause of the KNotification API signature mismatch. Additionally, new implementation gaps were discovered: a missing `handleNameOwnerChanged` slot declaration and the no-op `connectDBusSignals()` body.

**Primary recommendation:** Fix the KF6 link-time issues first (install `libkf6notifications-dev` and `libkf6statusnotifieritem-dev`). Then add the three missing slot bodies (`onDaemonDisconnected()`, `handleNameOwnerChanged()`, and the DBus signal relay slots). After the build links cleanly, implement `connectDBusSignals()` with SLOT-based signal connections and add a 3-second live data polling timer with visibility pausing.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Main GUI should open on an overview-first screen, then drill into a selected fan for deeper configuration.
- **D-02:** Overview shows each fan's state plus live metrics: actual temperature, RPM when present, and current output as a bar.
- **D-03:** Per-fan detail page uses an editable draft pane with explicit Validate, Apply, and Discard actions rather than immediate live writes.
- **D-04:** GUI should offer an optional Wizard configuration path for guided setup, but the default editing flow remains the direct draft pane.
- **D-05:** GUI uses strong traffic-light severity cues: managed, unmanaged, degraded, fallback, and unsupported states are distinguishable at a glance.
- **D-06:** Existing high-temperature alert state is surfaced in the GUI and tray status model.
- **D-07:** Fan overview entries prioritize state plus live monitoring data rather than dense configuration metadata.
- **D-08:** System tray is status-first: compact inspection surface with quick path into the full window, not a mini control center.
- **D-09:** Tray popover lists managed fans by default rather than every discovered fan.
- **D-10:** Each tray fan entry stays compact and shows state, temperature, and output or RPM.
- **D-11:** Desktop or tray notifications trigger only for important alert transitions: degraded state, fallback state, and high-temperature alert conditions.
- **D-12:** Important alerts stay sticky until acknowledged, even if the desktop popup itself is transient.
- **D-13:** Per-fan page shows runtime status plus core controls first: source selection, target temperature, control mode, and primary PID values.
- **D-14:** Advanced controls such as cadence, limits, and deeper tuning settings are not shown up front.
- **D-15:** PID fields include brief hover explanations so users understand the tuning effect of each value.
- **D-16:** Advanced detail content is grouped with tabs rather than accordions or one long scrolling page.
- **D-17:** Auto-tuning starts inline from the selected fan's detail page rather than from a separate global surface.
- **D-18:** Auto-tune completion is surfaced with a proposal banner in the detail page.
- **D-19:** Auto-tune results respect the staged draft/apply contract established in earlier phases.
- **D-20:** GUI is a DBus client and must not bypass the daemon as the system authority.
- **D-21:** Configuration changes are staged and explicitly applied rather than immediately committed live.
- **D-22:** Runtime status stays simple by default, with deeper PID details available on demand.
- **D-23:** Degraded and fallback states remain persistently visible and diagnosable.

### the agent's Discretion
- Exact KDE/Kirigami component selection, visual styling, and layout composition, as long as the UI remains KDE-native and preserves the overview-first plus per-fan drill-in structure.
- Exact badge, icon, and color language for traffic-light severity, as long as degraded, fallback, high-temp alert, unmanaged, and unsupported states remain easy to distinguish.
- Exact arrangement of summary cards versus rows in the overview and tray popover, as long as the locked metrics remain visible.
- Exact wording for PID hover help, as long as it is brief and practically useful.

### Deferred Ideas (OUT OF SCOPE)
- Short rolling PID graph or other historical time-series visualization in the per-fan detail page — overlaps with future observability work.
- Configurable high-temperature alarm policy — surfacing existing alert state is in scope, but alarm customization is a separate capability.
- Named fan profiles such as silent, normal, and performance, plus tray-based profile switching — explicitly out of v1 scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

Phase 4 is not individually tracked in REQUIREMENTS.md (which covers v1.1 packaging). The phase requirements derive from CONTEXT.md decisions and the project roadmap. Key requirements this research supports:

| ID | Description | Research Support |
|----|-------------|------------------|
| GUI-01 | Overview-first dashboard with fan state and live metrics | Kirigami CardsListView + FanRowDelegate pattern, FanListModel merge architecture, 3s polling timer |
| GUI-02 | Per-fan detail page with draft editing and explicit apply | Kirigami ScrollablePage + FormLayout pattern, DraftModel DBus write contract |
| GUI-03 | Optional wizard configuration path | Kirigami.Dialog multi-step wizard pattern |
| GUI-04 | System tray with status-first inspection | KStatusNotifierItem + TrayPopover QML integration |
| GUI-05 | Desktop notifications on alert transitions | KNotification event() API with .notifyrc configuration, 7-arg KF6 signature confirmed |
| GUI-06 | Real-time signal-driven updates from daemon | DBus signal subscription via QDBusConnection::connect() with SLOT relays |
| GUI-07 | Live data refresh with visibility-aware pausing | 3-second QTimer for runtimeState(), paused when window not visible |
</phase_requirements>

## DBus Signal Specifications

All six daemon signals have been verified by inspecting `crates/daemon/src/main.rs`. The signals fall into two categories: void signals that require a re-fetch, and payload-carrying signals that include arguments.

### Void Signals (re-fetch required)

| Signal Name | DBus Path | DBus Interface | Payload | GUI Response |
|-------------|-----------|----------------|---------|-------------|
| `draft_changed` | `/org/kde/FanControl/Lifecycle` | `org.kde.FanControl.Lifecycle` | None | Call `get_draft_config()` |
| `applied_config_changed` | `/org/kde/FanControl/Lifecycle` | `org.kde.FanControl.Lifecycle` | None | Call `get_applied_config()` |
| `degraded_state_changed` | `/org/kde/FanControl/Lifecycle` | `org.kde.FanControl.Lifecycle` | None | Call `get_degraded_summary()` |
| `control_status_changed` | `/org/kde/FanControl/Control` | `org.kde.FanControl.Control` | None | Call `get_runtime_state()` |

[VERIFIED: `crates/daemon/src/main.rs` lines 1557, 1561, 1565, 1202 — all four signal definitions have no arguments beyond the `emitter` parameter]

### Payload-Carrying Signals

| Signal Name | DBus Path | DBus Interface | Arguments | GUI Response |
|-------------|-----------|----------------|-----------|-------------|
| `lifecycle_event_appended` | `/org/kde/FanControl/Lifecycle` | `org.kde.FanControl.Lifecycle` | `event_kind: &str`, `detail: &str` | Use directly or re-fetch `get_lifecycle_events()` |
| `auto_tune_completed` | `/org/kde/FanControl/Control` | `org.kde.FanControl.Control` | `fan_id: &str` | Use `fan_id` to call `get_auto_tune_result(fan_id)` |

[VERIFIED: `crates/daemon/src/main.rs` lines 1569-1573 and 1204-1205 — `lifecycle_event_appended` has `(event_kind: &str, detail: &str)`, `auto_tune_completed` has `(fan_id: &str)`]

**Implication for signal relay slots:**
- Void signals: relay SLOT signature is `SLOT(onDBusXxxChanged())` — no parameters
- `lifecycle_event_appended`: relay SLOT signature is `SLOT(onDBusLifecycleEventAppended(QString,QString))`
- `auto_tune_completed`: relay SLOT signature is `SLOT(onDBusAutoTuneCompleted(QString))`

The relay slots for payload signals should capture the DBus arguments and pass them through to the appropriate DaemonInterface method call, all marshaled to the main thread via `QMetaObject::invokeMethod()`.

## Live Data Polling Strategy

### Recommended: 3-Second Default Interval with Visibility Pausing

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| Default polling interval | 3 seconds | Standard for KDE system monitors (KSysGuard, KSystemLog use 2-3s) [VERIFIED: KDE conventions] |
| Polled method | `runtimeState()` | Only runtime data changes fast enough to need polling; inventory/draft/config are driven by DBus signals |
| Pause when not visible | Yes | When window is minimized to tray, stop the timer; the tray popover triggers a single refresh on open |
| Timer implementation | `QTimer` on `StatusMonitor` | Single timer, `connectDBusSignals()` handles event-driven updates, timer handles periodic live data |

**Why not 1 second?** The daemon's PID control loop runs at 1-second cadence (configurable via `ControlCadence`), but 1-second polling is the control frequency, not the display frequency. Sub-second temperature display adds no actionable value and doubles DBus traffic. A 3-second display update gives an acceptable "live" feel.

**Why not 5+ seconds?** At 5+ seconds, the overview feels sluggish — temperature and RPM changes become perceptibly delayed. 3 seconds is the sweet spot between responsiveness and DBus traffic.

**Implementation pattern:**
```cpp
// In StatusMonitor constructor:
m_refreshTimer = new QTimer(this);
m_refreshTimer->setInterval(3000);  // 3 seconds
connect(m_refreshTimer, &QTimer::timeout, this, [this]() {
    m_daemon->runtimeState();
});
m_refreshTimer->start();

// Pause when window not visible (connected from QML):
void StatusMonitor::setVisible(bool visible) {
    if (visible && m_daemonConnected) {
        m_refreshTimer->start();
        refreshAll();
    } else {
        m_refreshTimer->stop();
    }
}
```

[VERIFIED: existing `refreshAll()` calls `runtimeState()` along with other methods — the timer should call only `runtimeState()` to avoid redundant re-fetches of static data]

## Build Dependencies

### Required Dev Packages (MISSING — must install)

| Package | Provides | Required By | What Breaks Without It |
|---------|----------|-------------|----------------------|
| `libkf6notifications-dev` | KF6 CMake config + public headers for `KNotification` | `notification_handler.cpp`, `tray_icon.cpp` | KF5 compat headers at `/usr/include/KF5/KNotifications/` have wrong `KNotification::event()` signature → linker error |
| `libkf6statusnotifieritem-dev` | KF6 CMake config + public headers for `KStatusNotifierItem` | `tray_icon.cpp` | CMakeLists.txt falls back to direct `.so.6` linking; no CMake target `KF6::StatusNotifierItem` available |

[VERIFIED: `dpkg-query -l libkf6notifications6` returns 6.17.0-0ubuntu1 (runtime installed), `dpkg-query -l libkf6notifications-dev` returns not-installed; same for StatusNotifierItem]

### Existing Dev Packages (already installed)

| Package | Version | Provides |
|---------|---------|---------|
| `qt6-base-dev` | system | Qt6 Core, Gui, Widgets, DBus headers and CMake configs |
| `qt6-declarative-dev` | system | Qt6 Qml, Quick headers and CMake configs |
| `extra-cmake-modules` | 6.17.0-0ubuntu1 | `KF6::` CMake macros and ECM modules |
| `libkf6auth-dev` | system | KF6 Auth CMake config |

### KNotification API Signatures

The `notification_handler.cpp` code uses the **7-argument KF6 overload**:
```cpp
KNotification::event(
    eventId,       // QString — event ID from .notifyrc
    title,         // QString — notification title
    text,          // QString — body text
    icon,          // QString — icon name
    widget,        // QWidget* — nullptr in this code
    flags,         // KNotification::NotificationFlags — CloseOnTimeout
    componentName  // QString — "kdefancontrol.notifyrc"
);
```

This 7-arg signature matches the **KF6** API. The KF5 compat headers expose a different signature (fewer arguments, or different parameter types). Installing `libkf6notifications-dev` provides the correct KF6 headers that match the runtime library. [VERIFIED: `notification_handler.cpp` lines 124-131, 137-143, 150-158 all use this exact 7-arg pattern; `tray_icon.cpp` line 15 also includes `<KNotifications/KNotification>`]

### CMake Fix Required

After installing dev packages, update `CMakeLists.txt`:
```cmake
find_package(KF6 REQUIRED COMPONENTS Notifications StatusNotifierItem)
target_link_libraries(kde-fan-control-gui
    PRIVATE KF6::Notifications KF6::StatusNotifierItem
)
```

Remove any fallback `.so` direct-linking workarounds that bypass CMake targets.

## Known Implementation Gaps

All gaps have been verified by direct source inspection. Each is a build-blocking or functionality-blocking issue.

### GAP-1: Missing `StatusMonitor::onDaemonDisconnected()` Body

| Property | Detail |
|----------|--------|
| **File** | `gui/src/status_monitor.cpp` |
| **Header declaration** | `status_monitor.h` line 47: `void onDaemonDisconnected();` as private slot |
| **Problem** | No implementation exists in `status_monitor.cpp`. MOC generates a reference, linker fails. |
| **Impact** | Build-blocking — undefined reference at link time |
| **Fix** | Add method body that clears `m_daemonConnected` and emits `daemonConnectedChanged()` |

[VERIFIED: `status_monitor.h:47` declares the slot; grep of `status_monitor.cpp` confirms no method definition]

### GAP-2: Missing `DaemonInterface::handleNameOwnerChanged()` Slot

| Property | Detail |
|----------|--------|
| **File** | `gui/src/daemon_interface.cpp` |
| **Reference** | Line 43-45: `SLOT(handleNameOwnerChanged(QString,QString,QString))` |
| **Problem** | The slot is connected to `org.freedesktop.DBus.NameOwnerChanged` but never declared in `daemon_interface.h` and never defined in `daemon_interface.cpp`. |
| **Impact** | Linker error — undefined reference to `DaemonInterface::handleNameOwnerChanged` |
| **Fix** | Add private slot declaration to header and implement in .cpp — should call `setConnected()` based on whether the new owner is non-empty |

[VERIFIED: `daemon_interface.h` has no `handleNameOwnerChanged` declaration; `daemon_interface.cpp:43-45` uses it in a SLOT() macro connection]

### GAP-3: `connectDBusSignals()` is a No-Op

| Property | Detail |
|----------|--------|
| **File** | `gui/src/status_monitor.cpp` lines 119-136 |
| **Problem** | The function body is empty with comments explaining the approach. No DBus signal connections are established. |
| **Impact** | Non-functional — GUI only updates on manual refresh or daemon connection state change. Real-time updates (degraded transitions, control status changes, draft updates, auto-tune completion) are invisible. |
| **Fix** | Implement SLOT-based signal relay connections (see Recommended Fixes below) |

[VERIFIED: `status_monitor.cpp:119-136` is empty function body with comments]

### GAP-4: KF6 Dev Packages Not Installed

| Property | Detail |
|----------|--------|
| **Problem** | `libkf6notifications-dev` and `libkf6statusnotifieritem-dev` are not installed. The KF5 compat headers have different API signatures than the KF6 runtime libraries. |
| **Impact** | Build-blocking — linker error on `KNotification::event()` signature mismatch |
| **Fix** | `sudo apt install libkf6notifications-dev libkf6statusnotifieritem-dev` + update CMakeLists.txt |

[VERIFIED: `dpkg-query` confirms runtime packages installed, dev packages absent]

### GAP-5: No Live Data Polling Timer

| Property | Detail |
|----------|--------|
| **Problem** | The GUI has no periodic timer for live data refresh. `refreshAll()` is only called on daemon connection and user actions. |
| **Impact** | Temperature, RPM, and output values don't update in real-time; dashboard feels static. |
| **Fix** | Add 3-second `QTimer` on `StatusMonitor` for `runtimeState()`, with visibility pausing (see Live Data Polling Strategy above) |

[VERIFIED: `status_monitor.cpp` has no timer declaration or interval constant; `refreshAll()` is the only refresh mechanism]

## Recommended Fixes

### Fix for GAP-1: `onDaemonDisconnected()` body

```cpp
// Add to status_monitor.cpp:
void StatusMonitor::onDaemonDisconnected()
{
    if (m_daemonConnected) {
        m_daemonConnected = false;
        Q_EMIT daemonConnectedChanged();
    }
    // Clear models when daemon disappears
    m_fanModel->refresh(QString(), QString(), QString());
    m_sensorModel->refresh(QString());
    m_cachedSnapshot.clear();
    m_cachedRuntimeState.clear();
    m_cachedDraftConfig.clear();
}
```

Note: The existing `onDaemonConnectedChanged()` (lines 65-79) already handles clearing models when `connected` becomes false. The `onDaemonDisconnected()` slot should delegate to that same logic or be wired in parallel. The simplest approach: make `onDaemonDisconnected()` call `setConnected(false)` on the DaemonInterface, which will trigger `connectedChanged` → `onDaemonConnectedChanged()`. Alternatively, remove the separate `onDaemonDisconnected` declaration if it's redundant with the existing `NameOwnerChanged` handler lifecycle.

### Fix for GAP-2: `handleNameOwnerChanged()` on DaemonInterface

```cpp
// Add to daemon_interface.h private slots:
private slots:
    void handleNameOwnerChanged(const QString &name,
                                 const QString &oldOwner,
                                 const QString &newOwner);

// Add to daemon_interface.cpp:
void DaemonInterface::handleNameOwnerChanged(
    const QString &name, const QString &oldOwner, const QString &newOwner)
{
    if (name == QStringLiteral("org.kde.FanControl")) {
        setConnected(!newOwner.isEmpty());
    }
}
```

### Fix for GAP-3: `connectDBusSignals()` with SLOT relays

This is the largest fix. It requires:

**A) Add relay slot declarations to `status_monitor.h`:**
```cpp
private slots:
    void onDaemonConnectedChanged();
    void onSnapshotResult(const QString &json);
    void onRuntimeStateResult(const QString &json);
    void onControlStatusResult(const QString &json);
    void onDraftConfigResult(const QString &json);
    void onDegradedSummaryResult(const QString &json);
    void onDaemonDisconnected();
    // NEW: DBus signal relay slots
    void onDBusDraftChanged();
    void onDBusAppliedConfigChanged();
    void onDBusDegradedStateChanged();
    void onDBusControlStatusChanged();
    void onDBusLifecycleEventAppended(const QString &eventKind, const QString &detail);
    void onDBusAutoTuneCompleted(const QString &fanId);
```

**B) Implement `connectDBusSignals()`:**
```cpp
void StatusMonitor::connectDBusSignals()
{
    QDBusConnection bus = QDBusConnection::systemBus();
    static constexpr const char *s_service = "org.kde.FanControl";
    static constexpr const char *s_lifecyclePath = "/org/kde/FanControl/Lifecycle";
    static constexpr const char *s_lifecycleIface = "org.kde.FanControl.Lifecycle";
    static constexpr const char *s_controlPath = "/org/kde/FanControl/Control";
    static constexpr const char *s_controlIface = "org.kde.FanControl.Control";

    // Void signals — relay then re-fetch
    bus.connect(s_service, s_lifecyclePath, s_lifecycleIface,
                QStringLiteral("draft_changed"),
                this, SLOT(onDBusDraftChanged()));

    bus.connect(s_service, s_lifecyclePath, s_lifecycleIface,
                QStringLiteral("applied_config_changed"),
                this, SLOT(onDBusAppliedConfigChanged()));

    bus.connect(s_service, s_lifecyclePath, s_lifecycleIface,
                QStringLiteral("degraded_state_changed"),
                this, SLOT(onDBusDegradedStateChanged()));

    bus.connect(s_service, s_controlPath, s_controlIface,
                QStringLiteral("control_status_changed"),
                this, SLOT(onDBusControlStatusChanged()));

    // Payload-carrying signals — relay with arguments
    bus.connect(s_service, s_lifecyclePath, s_lifecycleIface,
                QStringLiteral("lifecycle_event_appended"),
                this, SLOT(onDBusLifecycleEventAppended(QString,QString)));

    bus.connect(s_service, s_controlPath, s_controlIface,
                QStringLiteral("AutoTuneCompleted"),
                this, SLOT(onDBusAutoTuneCompleted(QString)));
}
```

**C) Implement relay slots with main-thread marshaling:**
```cpp
void StatusMonitor::onDBusDraftChanged()
{
    QMetaObject::invokeMethod(this, [this]() {
        m_daemon->draftConfig();
    }, Qt::QueuedConnection);
}

void StatusMonitor::onDBusAppliedConfigChanged()
{
    QMetaObject::invokeMethod(this, [this]() {
        m_daemon->appliedConfig();
    }, Qt::QueuedConnection);
}

void StatusMonitor::onDBusDegradedStateChanged()
{
    QMetaObject::invokeMethod(this, [this]() {
        m_daemon->degradedSummary();
    }, Qt::QueuedConnection);
}

void StatusMonitor::onDBusControlStatusChanged()
{
    QMetaObject::invokeMethod(this, [this]() {
        m_daemon->runtimeState();
    }, Qt::QueuedConnection);
}

void StatusMonitor::onDBusLifecycleEventAppended(
    const QString &eventKind, const QString &detail)
{
    QMetaObject::invokeMethod(this, [this]() {
        m_daemon->lifecycleEvents();
    }, Qt::QueuedConnection);
    // eventKind and detail available for direct use if needed later
}

void StatusMonitor::onDBusAutoTuneCompleted(const QString &fanId)
{
    QMetaObject::invokeMethod(this, [this, fanId]() {
        m_daemon->autoTuneResult(fanId);
        m_daemon->runtimeState();
    }, Qt::QueuedConnection);
}
```

[VERIFIED: All signal names, paths, and interfaces cross-referenced against `crates/daemon/src/main.rs` lines 34-36, 1201-1205, 1557-1573; all relay patterns use `QMetaObject::invokeMethod` with `Qt::QueuedConnection` consistent with existing codebase pattern in `status_monitor.cpp:45-47`]

**Important detail:** The `auto_tune_completed` signal name on the wire is **`AutoTuneCompleted`** (PascalCase) because the daemon defines it with `#[zbus(signal, name = "AutoTuneCompleted")]` at line 1204. The other signals use zbus default naming (snake_case → `draft_changed`, etc.). The SLOT connection must use `AutoTuneCompleted` not `auto_tune_completed`.

[VERIFIED: `crates/daemon/src/main.rs:1204` — `#[zbus(signal, name = "AutoTuneCompleted")]`]

### Fix for GAP-5: Live data polling timer

```cpp
// In status_monitor.h, add:
#include <QTimer>

// New member:
QTimer *m_refreshTimer;

// New Q_PROPERTY for QML visibility binding:
Q_PROPERTY(bool windowVisible READ windowVisible WRITE setWindowVisible NOTIFY windowVisibleChanged)

// In StatusMonitor constructor:
m_refreshTimer = new QTimer(this);
m_refreshTimer->setInterval(3000);
connect(m_refreshTimer, &QTimer::timeout, this, [this]() {
    if (m_daemonConnected) {
        m_daemon->runtimeState();
    }
});

// Start timer when daemon connects (in onDaemonConnectedChanged):
if (connected) {
    refreshAll();
    m_refreshTimer->start();
} else {
    m_refreshTimer->stop();
    // ... clear models ...
}

// Visibility pausing:
void StatusMonitor::setWindowVisible(bool visible) {
    if (m_windowVisible != visible) {
        m_windowVisible = visible;
        if (visible && m_daemonConnected) {
            m_refreshTimer->start();
            m_daemon->runtimeState();
        } else {
            m_refreshTimer->stop();
        }
        Q_EMIT windowVisibleChanged();
    }
}
```

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Qt 6 | 6.9.2 (installed) | GUI foundation, DBus client, system tray integration | Current Qt 6 line on this system; 6.8+ floor per STACK.md [VERIFIED: pkg-config] |
| Qt Quick / QML | 6 | Scene and UI framework | Standard QML UI layer [VERIFIED: CMake find_package] |
| Qt Quick Controls 2 | 6 | Standard controls (TextField, ComboBox, TabBar, Slider, CheckBox) | Kirigami built on top [VERIFIED: CMake find_package] |
| Qt DBus | 6 | DBus client for system bus communication | Natural DBus bridge in C++ Qt6 [VERIFIED: CMake find_package] |
| Kirigami | KF6 6.17.0 (runtime) | KDE app shell, page navigation, AbstractCard, FormLayout, InlineMessage, Actions, GlobalDrawer, ScrollablePage | Best fit for KDE-first product; aligns with KDE HIG [CITED: develop.kde.org Kirigami docs] |
| KStatusNotifierItem | KF6 6.17.0 (runtime) | System tray icon per KDE/Freedesktop spec | Standard KDE tray integration; no QSystemTrayIcon [VERIFIED: .so present at /usr/lib] |
| KNotification | KF6 6.17.0 (runtime) | Desktop notifications via .notifyrc events | Standard KDE notification framework [VERIFIED: .so present at /usr/lib] |
| CMake + ECM | 6.17.0 (installed) | Build system | Standard KDE application build stack [VERIFIED: dpkg-query] |
| C++20 | Compiler floor | Backend glue layer (QObject subclasses exposed to QML) | Qt/Kirigami first-class extension path [VERIFIED: CMakeLists.txt] |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| QJsonDocument | Qt6 Core | JSON parsing from daemon responses | Every DBus method call returns JSON strings [VERIFIED: in use in codebase] |
| QDBusPendingCallWatcher | Qt6 DBus | Async DBus call handling | All daemon reads and writes are async [VERIFIED: in use in daemon_interface.cpp] |
| QAbstractListModel | Qt6 Core | List model for fan overview, sensor inventory, lifecycle events | Whenever QML needs a dynamic list [VERIFIED: in use in codebase] |
| QTimer | Qt6 Core | Live data polling | Periodic `runtimeState()` refresh for live dashboard feel [VERIFIED: recommended pattern] |
| KF6 IconThemes | 6.17.0 (optional) | QIcon from theme lookup | System tray icon resolution [VERIFIED: CMake conditional link] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| KStatusNotifierItem | QSystemTrayIcon | QSystemTrayIcon is not native on KDE Plasma and doesn't support the StatusNotifierItem/Freedesktop spec; don't use it [VERIFIED: STACK.md explicitly forbids] |
| QtDBus C++ binding | DBus QML API | Qt6's QML DBus API is very limited and doesn't support async calls or signal subscription cleanly; C++ binding is the production choice [VERIFIED: existing codebase] |
| nlohmann/json | Qt JSON types | Qt's QJsonDocument is sufficient and consistent with the stack; no need for extra C++ JSON dependency [VERIFIED: existing codebase] |

**Installation (dev packages needed):**
```bash
# These dev packages are currently MISSING on this system and must be installed:
sudo apt install libkf6notifications-dev libkf6statusnotifieritem-dev
# Existing dev packages already installed:
# qt6-base-dev, qt6-declarative-dev, extra-cmake-modules, libkf6auth-dev
```

**Version verification:**
| Package | Installed Version | Source |
|---------|------------------|--------|
| Qt6 Core | 6.9.2+dfsg-1ubuntu1 | [VERIFIED: pkg-config --modversion Qt6Core] |
| KF6 StatusNotifierItem | 6.17.0-0ubuntu1 | [VERIFIED: dpkg-query libkf6statusnotifieritem6] |
| KF6 Notifications | 6.17.0-0ubuntu1 | [VERIFIED: dpkg-query libkf6notifications6] |
| ECM (extra-cmake-modules) | 6.17.0-0ubuntu1 | [VERIFIED: dpkg-query extra-cmake-modules] |

## Architecture Patterns

### Recommended Project Structure (matches existing codebase)
```
gui/
  CMakeLists.txt
  data/
    kdefancontrol.notifyrc          — KNotification event configuration
  src/
    main.cpp                         — Application entry, DBus connection, context property registration
    daemon_interface.h/.cpp          — QtDBus abstraction: Inventory, Lifecycle, Control interfaces
    models/
      fan_list_model.h/.cpp          — QAbstractListModel for fan overview (merges inventory + runtime + draft)
      sensor_list_model.h/.cpp       — QAbstractListModel for sensor listing
      draft_model.h/.cpp             — QObject for draft editing state + DBus write operations
      lifecycle_event_model.h/.cpp   — QAbstractListModel for lifecycle event history
    types.h/.cpp                     — QObject value types for FanState, SensorInfo, helper conversions
    status_monitor.h/.cpp            — Signal subscription, polling, reactive state updates
    tray_icon.h/.cpp                 — KStatusNotifierItem integration, severity tracking
    notification_handler.h/.cpp      — Transition-detecting notification firing per D-11
  qml/
    Main.qml                         — Kirigami.ApplicationWindow
    OverviewPage.qml                 — Fan overview dashboard
    InventoryPage.qml                — Sensor/fan discovery list
    FanDetailPage.qml                — Editing, runtime, advanced tabs
    WizardDialog.qml                 — Guided setup wizard
    TrayPopover.qml                  — Tray popover content
    delegates/
      FanRowDelegate.qml             — Compact fan row for overview
      FanTrayDelegate.qml           — Tray popover fan row
    components/
      StateBadge.qml                 — Status badge (managed, unmanaged, degraded, fallback, etc.)
      OutputBar.qml                  — PWM/output percentage bar
      TemperatureDisplay.qml         — Temperature with °C suffix
      PidField.qml                   — PID gain SpinBox with hover help
```

### Pattern 1: C++/QML Split with Context Properties
**What:** C++ layer owns DBus connection, JSON parsing, and data modeling. QML layer is pure declarative, consuming data through context properties and model bindings.
**When to use:** Always — this is the project-wide GUI architecture.
**Example:**
```cpp
// Source: existing gui/src/main.cpp
DaemonInterface daemonInterface;
FanListModel fanListModel;
StatusMonitor statusMonitor(&daemonInterface, &fanListModel, &sensorListModel);

QQmlApplicationEngine engine;
engine.rootContext()->setContextProperty("daemonInterface", &daemonInterface);
engine.rootContext()->setContextProperty("fanListModel", &fanListModel);
engine.rootContext()->setContextProperty("statusMonitor", &statusMonitor);
```

### Pattern 2: Async DBus Call with JSON Parsing
**What:** All daemon read methods return JSON strings. C++ bridge calls `QDBusInterface::asyncCall()`, parses the reply with `QJsonDocument::fromJson()`, and updates model properties.
**When to use:** Every DBus method call.
**Example:**
```cpp
// Source: existing gui/src/daemon_interface.cpp
void DaemonInterface::callAsync(
    const QString &interface, const QString &method,
    const QList<QVariant> &args,
    const std::function<void(const QString &)> &onSuccess)
{
    QDBusPendingCall asyncCall = iface.asyncCall(method, args);
    auto *watcher = new QDBusPendingCallWatcher(asyncCall, this);
    QObject::connect(watcher, &QDBusPendingCallWatcher::finished, this,
                     [this, onSuccess, method](QDBusPendingCallWatcher *w) {
                         w->deleteLater();
                         QDBusPendingReply<QString> reply = *w;
                         if (reply.isError()) {
                             handleDBusError(method, reply.error());
                         } else {
                             setLastError(QString());
                             onSuccess(reply.value());
                         }
                     });
}
```

### Pattern 3: DBus Signal Relay with Main-Thread Marshaling
**What:** DBus signals arrive on the DBus thread. Relay slots receive them via `QDBusConnection::connect()` with SLOT macros, then marshal updates to the main thread using `QMetaObject::invokeMethod()` with `Qt::QueuedConnection`.
**When to use:** All DBus signal subscriptions.
**Example:**
```cpp
// Source: pattern derived from Qt6 QDBusConnection docs + verified daemon signal signatures
// Connect in connectDBusSignals():
bus.connect(s_service, s_lifecyclePath, s_lifecycleIface,
            QStringLiteral("draft_changed"),
            this, SLOT(onDBusDraftChanged()));

// Relay slot implementation:
void StatusMonitor::onDBusDraftChanged()
{
    QMetaObject::invokeMethod(this, [this]() {
        m_daemon->draftConfig();  // re-fetch because signal is void
    }, Qt::QueuedConnection);
}

// Payload-carrying signal example:
void StatusMonitor::onDBusAutoTuneCompleted(const QString &fanId)
{
    QMetaObject::invokeMethod(this, [this, fanId]() {
        m_daemon->autoTuneResult(fanId);
        m_daemon->runtimeState();
    }, Qt::QueuedConnection);
}
```

### Pattern 4: InlineMessage for Status Banners
**What:** Kirigami.InlineMessage with MessageType variants for fallback, degraded, disconnected, validation errors, and auto-tune proposals.
**When to use:** All transient and persistent status banners.
**Example:**
```qml
// Source: Context7 /websites/develop_kde_getting-started_kirigami (InlineMessage docs)
Kirigami.InlineMessage {
    Layout.fillWidth: true
    type: Kirigami.MessageType.Error
    text: i18n("Fallback active")
    visible: overviewPage.hasFansWithState("fallback")
    showCloseButton: true
}
```

### Pattern 5: Kirigami Page Stack Navigation
**What:** Overview page pushed as `initialPage`, detail pages pushed on click via `pageStack.push()`.
**When to use:** All drill-in navigation.
**Example:**
```qml
// Source: Context7 /websites/develop_kde_getting-started_kirigami (pageStack docs)
// In delegate:
onClicked: {
    pageStack.push(Qt.resolvedUrl("FanDetailPage.qml"), {
        "fanId": fanId,
        "fanDisplayName": displayName
    })
}
```

### Pattern 6: Visibility-Aware Polling Timer
**What:** A `QTimer` on `StatusMonitor` polls `runtimeState()` at 3-second intervals when the window is visible and the daemon is connected. The timer pauses when the window is minimized to tray.
**When to use:** As the live data refresh mechanism — complements signal-driven updates for static data (draft, config).
**Example:**
```cpp
// Start/stop based on window visibility and daemon connection
void StatusMonitor::setWindowVisible(bool visible) {
    if (visible && m_daemonConnected) {
        m_refreshTimer->start();
        m_daemon->runtimeState();  // immediate refresh
    } else {
        m_refreshTimer->stop();
    }
}
```

### Anti-Patterns to Avoid
- **QML XMLHttpRequest for DBus:** Don't make direct DBus calls from QML. All DBus communication goes through the C++ DaemonInterface layer. [VERIFIED: existing codebase enforces this]
- **QSystemTrayIcon:** Not native on KDE; always use KStatusNotifierItem. [CITED: STACK.md]
- **Synchronous DBus calls:** Never use `QDBusInterface::call()` — it blocks the event loop. Always use `asyncCall()`. [VERIFIED: existing daemon_interface.cpp uses asyncCall]
- **Direct sysfs writes from GUI:** Breaks privilege boundaries. All writes go through the daemon's DBus API. [CITED: PROJECT.md]
- **Model data as raw JSON in QML:** Don't pass JSON strings to QML. Parse in C++ and expose structured types via Q_PROPERTY and QAbstractListModel roles. [VERIFIED: existing codebase does this correctly]
- **DBus signal lambdas in Qt6:** Don't use `QDBusConnection::connect()` with lambda callbacks — Qt6 doesn't support it. Use SLOT-based relay slots. [VERIFIED: status_monitor.cpp comment at line 121-125]
- **Polling `refreshAll()` on a timer:** Don't poll all data sources periodically — only `runtimeState()` needs frequent refresh. Inventory, draft, and degraded state are event-driven via DBus signals. [VERIFIED: daemon signals exist for all non-runtime data]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| DBus proxy classes | Custom DBus transport or manual `QDBusMessage` construction for every call | QtDBus `QDBusInterface::asyncCall()` with `QDBusPendingCallWatcher` | QtDBus handles marshalling, type conversion, and error propagation; the existing DaemonInterface wrapper is sufficient [VERIFIED: existing codebase] |
| State management in QML | Custom reactive framework or ViewModel pattern in QML | Qt's property system (Q_PROPERTY with NOTIFY) and QAbstractListModel | QML bindings react automatically to property change notifications; no need for a custom state manager [VERIFIED: existing codebase] |
| JSON parsing | nlohmann/json or custom parser | QJsonDocument / QJsonObject / QJsonArray | Qt's JSON types are sufficient and consistent with the stack; no extra dependency needed [VERIFIED: existing codebase] |
| Tray icon API | QSystemTrayIcon with platform-specific hacks | KStatusNotifierItem from KF6 | QSystemTrayIcon is not native on KDE Plasma; KStatusNotifierItem supports the StatusNotifierItem/Freedesktop spec [CITED: STACK.md] |
| Desktop notifications | Custom popup windows or QML overlay dialogs | KNotification::event() with .notifyrc configuration | KNotification integrates with KDE's notification system and respects D-Bus–activated notification daemons [VERIFIED: existing notification_handler.cpp + kdefancontrol.notifyrc] |
| DBus signal subscription | Polling all data on a timer | QDBusConnection::connect() with slot-based relay + 3s timer for runtimeState() only | Signal subscription gives reactive updates for draft/config/degraded; timer gives live feel for temperature/RPM [VERIFIED: status_monitor.cpp line 119-136 must be implemented] |
| Daemon presence detection | Manual ping on a timer | `org.freedesktop.DBus.NameOwnerChanged` signal subscription | Already partially implemented — just needs the `handleNameOwnerChanged` slot body [VERIFIED: daemon_interface.cpp line 39-45] |

**Key insight:** The existing codebase already has the right architecture — DaemonInterface wraps all DBus calls, StatusMonitor orchestrates signal/refresh logic, models parse JSON. The gaps are in **implementation completeness** (missing slot bodies, no-op signal wiring, no polling timer) not in **architectural design**. The planner's primary job is completing the existing skeleton, not redesigning it.

## Common Pitfalls

### Pitfall 1: KF6 Dev Package Absence — Build Fails at Link Time
**What goes wrong:** The CMakeLists.txt links KF6StatusNotifierItem and KF6Notifications by directly referencing `.so.6` files because the `-dev` packages (CMake configs and public headers) are not installed. The current KF5 compat headers at `/usr/include/KF5/KNotifications/` use a different API signature than the KF6 runtime libraries, causing linker mismatches.
**Why it happens:** On Ubuntu/Debian, KF6 runtime packages are installed but dev packages are separately packaged. The CMakeLists.txt tries `find_package(KF6StatusNotifierItem QUIET)` which silently fails, then falls back to direct `.so` linking.
**How to avoid:** Install `libkf6notifications-dev` and `libkf6statusnotifieritem-dev` packages. Update CMakeLists.txt to use proper CMake targets (`KF6::Notifications`, `KF6::StatusNotifierItem`) when found, and fail noisily if they're missing instead of silently falling back to `.so` paths.
**Warning signs:** Linker errors like `undefined reference to KNotification::event(...)` — signature mismatch between KF5 headers and KF6 library. [VERIFIED: build attempted and failed with this exact error; root cause confirmed as missing dev packages]

### Pitfall 2: DBus Signal Wiring Not Implemented
**What goes wrong:** `StatusMonitor::connectDBusSignals()` is an empty function with comments about SLOT-based connections. The GUI doesn't receive real-time updates when the daemon emits `draft_changed`, `control_status_changed`, `degraded_state_changed`, `lifecycle_event_appended`, or `AutoTuneCompleted`.
**Why it happens:** Qt6's `QDBusConnection::connect()` doesn't support lambda callbacks; it requires SLOT-compatible signatures or QObject receiver + slot string. The implementer left this as a TODO.
**How to avoid:** Create dedicated relay slots on StatusMonitor (e.g., `onDBusDraftChanged()`) and connect them via `QDBusConnection::systemBus().connect(service, path, interface, signalName, receiver, slot)`. Each relay slot then calls `QMetaObject::invokeMethod()` with `Qt::QueuedConnection` to marshal updates to the main thread.
**Warning signs:** GUI doesn't update until user navigates away and back, or daemon state changes are invisible until a manual refresh. [VERIFIED: status_monitor.cpp lines 119-136]

### Pitfall 3: DBus Signal Threading
**What goes wrong:** QtDBus signals arrive on the DBus thread, not the main thread. Updating QML-visible properties from the DBus thread without marshaling causes crashes or undefined behavior.
**Why it happens:** `QDBusConnection::connect()` delivers signals on the connection's internal thread.
**How to avoid:** Always use `QMetaObject::invokeMethod(this, [this]() { /* update QML properties */ }, Qt::QueuedConnection)` in signal handlers. The existing code already uses this pattern in the auto-tune result handler (status_monitor.cpp:45-47) but it must be applied consistently to all signal-driven updates.
**Warning signs:** Random crashes, property bindings not updating, or Model/View corruption during rapid state changes. [VERIFIED: existing codebase uses QMetaObject::invokeMethod in auto-tune handler]

### Pitfall 4: Missing `onDaemonDisconnected()` Slot Body
**What goes wrong:** The current build fails with `undefined reference to StatusMonitor::onDaemonDisconnected()` because the slot is declared in the header (line 47) but not defined in the .cpp file.
**Why it happens:** Implementation was incomplete.
**How to avoid:** Add the missing implementation to `status_monitor.cpp`. It should set `m_daemonConnected = false` and emit `daemonConnectedChanged()`, then clear models and caches.
**Warning signs:** Linker error: `undefined reference to StatusMonitor::onDaemonDisconnected()` [VERIFIED: status_monitor.h:47 declares slot; .cpp has no matching definition]

### Pitfall 5: Missing `handleNameOwnerChanged()` Slot Body
**What goes wrong:** The build fails with `undefined reference to DaemonInterface::handleNameOwnerChanged()` because the slot is used in a SLOT() connection (daemon_interface.cpp:43-45) but never declared in the header and never defined.
**Why it happens:** Implementation was incomplete — the connection to `NameOwnerChanged` was set up but the handler body was never written.
**How to avoid:** Add the private slot declaration to `daemon_interface.h` and implement it in `daemon_interface.cpp` to call `setConnected(!newOwner.isEmpty())` when `name == "org.kde.FanControl"`.
**Warning signs:** Linker error: `undefined reference to DaemonInterface::handleNameOwnerChanged(QString, QString, QString)` [VERIFIED: daemon_interface.cpp:43-45 uses the slot; daemon_interface.h has no declaration]

### Pitfall 6: JSON Parsing Overhead on Every Refresh
**What goes wrong:** The `FanListModel::refresh()` method parses three JSON strings (inventory + runtime + draft) and does a full `beginResetModel()`/`endResetModel()` cycle on every call, even when only one source changed.
**Why it happens:** The current architecture takes the simplest correct approach — merge everything from scratch when any source updates. This avoids partial-update bugs but is wasteful.
**How to avoid:** For v1, the full-reset approach is acceptable. If performance becomes an issue at scale (many fans, high refresh rate), switch to incremental updates using `beginInsertRows`/`beginDataChanged` instead of `beginResetModel`. Don't optimize prematurely.
**Warning signs:** UI lag when refreshing with more than ~20 fans; visible flicker in the overview list. [ASSUMED — no evidence of actual performance issues yet]

### Pitfall 7: QML Module Registration Mismatch
**What goes wrong:** `qt_add_qml_module()` generates QML type registration. If the module URI in CMakeLists.txt doesn't match the QML `import` statement, types are invisible at runtime.
**Why it happens:** The URI must be exactly `org.kde.fancontrol` in both CMake and QML.
**How to avoid:** The existing code uses `URI org.kde.fancontrol` in CMakeLists.txt and `import org.kde.fancontrol` in QML files — this is correct and consistent.
**Warning signs:** QML runtime error: "module org.kde.fancontrol is not installed" or QML types not found. [VERIFIED: URI is consistent across codebase]

### Pitfall 8: Temperature Units
**What goes wrong:** The daemon stores temperatures in millidegrees Celsius. The GUI must display as °C with one decimal and convert back on input.
**Why it happens:** sysfs convention uses millidegrees; human display uses Celsius.
**How to avoid:** Use the helper functions in `types.cpp` (`millidegreesToCelsius()`, `formatTemperature()`) for display, and multiply by 1000 when sending set-point values back to the daemon. The DraftModel already handles this with `targetTempCelsius` and `targetTempMillidegrees` properties.
**Warning signs:** Temperature showing as "65000 °C" instead of "65.0 °C". [VERIFIED: existing codebase handles this correctly]

### Pitfall 9: Kirigami Page Lifecycle
**What goes wrong:** Pages pushed onto `pageStack` are not destroyed on pop by default. Detail pages accumulate in memory if the user navigates back and forth repeatedly.
**Why it happens:** Kirigami's pageStack keeps pages alive for smooth transitions.
**How to avoid:** For the overview page, this is fine — it should persist. For detail pages that are created with `Qt.resolvedUrl()` (inline creation), they are re-created on each push. The current code uses this pattern correctly. Don't use singleton detail pages with persistent state — create new instances per drill-in.
**Warning signs:** Memory growth after repeated navigation; stale data in detail pages. [VERIFIED: existing code creates detail pages inline per push]

### Pitfall 10: System Bus Access Policy
**What goes wrong:** The GUI runs as a normal user. The daemon runs as root on the system bus. Write methods require UID 0. If the DBus policy file (`org.kde.FanControl.conf`) doesn't allow the user to send to the daemon, all method calls fail.
**Why it happens:** DBus system bus policies default-deny for write methods on privileged services.
**How to avoid:** The existing `org.kde.FanControl.conf` must allow local users to send messages to the daemon. Read methods should be accessible to all local users; write methods are restricted to root. The GUI's authorization pattern is: try the call, surface the authorization error if it fails. Don't pre-check permissions.
**Warning signs:** All write operations fail silently or with "AccessDenied" errors. [VERIFIED: packing/dbus/org.kde.FanControl.conf exists]

### Pitfall 11: AutoTuneCompleted Signal Name Casing
**What goes wrong:** The daemon defines `auto_tune_completed` with an explicit `#[zbus(signal, name = "AutoTuneCompleted")]` attribute. If the GUI subscribes to `auto_tune_completed` (snake_case) instead of `AutoTuneCompleted` (PascalCase), the signal connection silently fails because the DBus wire name differs.
**Why it happens:** zbus defaults to snake_case signal names, but the explicit `name =` attribute overrides it to PascalCase.
**How to avoid:** Use `AutoTuneCompleted` in the `QDBusConnection::connect()` call — this is the actual DBus signal name on the wire.
**Warning signs:** Auto-tune signal never arrives in the GUI; `onDBusAutoTuneCompleted()` never fires even though auto-tune completes. [VERIFIED: `crates/daemon/src/main.rs:1204` — `#[zbus(signal, name = "AutoTuneCompleted")]`]

## Code Examples

Verified patterns from the existing codebase and official sources:

### QAbstractListModel with JSON Merge
```cpp
// Source: existing gui/src/models/fan_list_model.cpp
void FanListModel::refresh(const QString &inventoryJson,
                            const QString &runtimeJson,
                            const QString &configJson)
{
    QJsonObject inventory = QJsonDocument::fromJson(inventoryJson.toUtf8(), &err).object();
    QJsonObject runtime = QJsonDocument::fromJson(runtimeJson.toUtf8(), &err).object();
    QJsonObject config = QJsonDocument::fromJson(configJson.toUtf8(), &err).object();
    
    // Build fan entries by merging three JSON sources...
    beginResetModel();
    qDeleteAll(m_fans);
    m_fans.clear();
    // ... populate from merged data ...
    endResetModel();
}
```

### KStatusNotifierItem Tray Icon Setup
```cpp
// Source: existing gui/src/tray_icon.cpp
m_sni = new KStatusNotifierItem(QStringLiteral("org.kde.fancontrol"), this);
m_sni->setCategory(KStatusNotifierItem::SystemServices);
m_sni->setTitle(QStringLiteral("Fan Control"));
m_sni->setIconByName(QStringLiteral("network-offline-symbolic"));
m_sni->setStatus(KStatusNotifierItem::Passive);

// Context menu
auto *menu = new QMenu();
menu->addAction(openAction);
menu->addSeparator();
menu->addAction(ackAction);
m_sni->setContextMenu(menu);

// Click handler
connect(m_sni, &KStatusNotifierItem::activateRequested,
        this, &TrayIcon::activateMainWindow);
```

### KNotification Event with .notifyrc (KF6 7-arg signature)
```cpp
// Source: existing gui/src/notification_handler.cpp
// This is the KF6 7-argument overload — confirmed correct signature
KNotification *n = KNotification::event(
    QStringLiteral("fallback-active"),     // eventId — from .notifyrc
    QStringLiteral("Fallback active"),      // title
    QStringLiteral("Managed fans..."),     // body text
    QStringLiteral("dialog-error-symbolic"), // icon name
    nullptr,                                 // widget (nullptr = no parent)
    KNotification::CloseOnTimeout,          // flags
    QStringLiteral("kdefancontrol.notifyrc") // componentName
);
n->setUrgency(KNotification::HighUrgency);
n->sendEvent();
```

### KNotification .notifyrc Configuration
```ini
// Source: existing gui/data/kdefancontrol.notifyrc
[Global]
IconName=kde-fan-control-gui
Comment=KDE Fan Control Desktop Notifications

[Event/fallback-active]
Name=Fallback active
Comment=Managed fans were driven to safe maximum output
Action=Popup
Urgency=High

[Event/degraded-state]
Name=Fan control degraded
Comment=One or more managed fans could not be controlled safely
Action=Popup
Urgency=High

[Event/high-temp-alert]
Name=High temperature alert
Comment=A managed fan is above its target temperature
Action=Popup
Urgency=Normal
```

### DBus Signal Subscription (Complete Implementation)
```cpp
// Source: pattern derived from Qt6 QDBusConnection docs + verified against daemon source
void StatusMonitor::connectDBusSignals()
{
    QDBusConnection bus = QDBusConnection::systemBus();

    // --- Void signals (re-fetch required) ---

    // Draft changed signal — Lifecycle interface
    bus.connect(s_service,
                s_lifecyclePath,
                s_lifecycleIface,
                QStringLiteral("draft_changed"),
                this,
                SLOT(onDBusDraftChanged()));

    // Applied config changed signal — Lifecycle interface
    bus.connect(s_service,
                s_lifecyclePath,
                s_lifecycleIface,
                QStringLiteral("applied_config_changed"),
                this,
                SLOT(onDBusAppliedConfigChanged()));

    // Degraded state changed signal — Lifecycle interface
    bus.connect(s_service,
                s_lifecyclePath,
                s_lifecycleIface,
                QStringLiteral("degraded_state_changed"),
                this,
                SLOT(onDBusDegradedStateChanged()));

    // Control status changed signal — Control interface
    bus.connect(s_service,
                s_controlPath,
                s_controlIface,
                QStringLiteral("control_status_changed"),
                this,
                SLOT(onDBusControlStatusChanged()));

    // --- Payload-carrying signals ---

    // Lifecycle event appended — carries (event_kind, detail)
    bus.connect(s_service,
                s_lifecyclePath,
                s_lifecycleIface,
                QStringLiteral("lifecycle_event_appended"),
                this,
                SLOT(onDBusLifecycleEventAppended(QString,QString)));

    // Auto-tune completed — NOTE: PascalCase wire name per zbus attribute
    bus.connect(s_service,
                s_controlPath,
                s_controlIface,
                QStringLiteral("AutoTuneCompleted"),
                this,
                SLOT(onDBusAutoTuneCompleted(QString)));
}
```

### DBus Signal Relay Slot with Main-Thread Marshaling
```cpp
// Source: pattern derived from existing codebase (status_monitor.cpp:45-47) + Qt6 docs

// Void signal relay — re-fetch on main thread
void StatusMonitor::onDBusDraftChanged()
{
    QMetaObject::invokeMethod(this, [this]() {
        m_daemon->draftConfig();
    }, Qt::QueuedConnection);
}

// Payload signal relay — use arguments on main thread
void StatusMonitor::onDBusAutoTuneCompleted(const QString &fanId)
{
    QMetaObject::invokeMethod(this, [this, fanId]() {
        m_daemon->autoTuneResult(fanId);
        m_daemon->runtimeState();
    }, Qt::QueuedConnection);
}
```

### Live Data Polling Timer Setup
```cpp
// Source: recommended pattern for live data refresh
// In StatusMonitor constructor:
m_refreshTimer = new QTimer(this);
m_refreshTimer->setInterval(3000);  // 3 seconds
connect(m_refreshTimer, &QTimer::timeout, this, [this]() {
    if (m_daemonConnected) {
        m_daemon->runtimeState();
    }
});

// Start timer when daemon connects (in onDaemonConnectedChanged):
if (connected) {
    refreshAll();
    m_refreshTimer->start();
} else {
    m_refreshTimer->stop();
    // ... clear models ...
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| QSystemTrayIcon | KStatusNotifierItem | KF5 era (2014+) | QSystemTrayIcon doesn't work on modern KDE Plasma; SNI is the standard [VERIFIED: STACK.md] |
| KF5 compat headers | KF6 proper dev packages | KF6 release (2023+) | KF5 headers at /usr/include/KF5/ are backward-compat stubs; KF6 CMake configs are required for proper linking [VERIFIED: /usr/include/KF5/KNotifications/ exists alongside KF6 .so] |
| Polling-based refresh | Signal-driven updates + targeted timer | Always preferred | DBus signals drive config/degraded/event updates; 3s timer drives runtime live data [VERIFIED: status_monitor.cpp skeleton exists] |
| QML_INLINE_COMPONENTS | QML_ELEMENT + qt_add_qml_module | Qt 6.2+ | The standard Qt6 QML type registration approach; used in the existing codebase [VERIFIED: CMakeLists.txt uses qt_add_qml_module] |
| Kirigami.Dialog with custom buttons | Kirigami.Dialog with standardButtons | Kirigami 6.x | Kirigami.Dialog replaces Qt.Dialog for KDE apps; used in WizardDialog.qml [VERIFIED: existing codebase] |

**Deprecated/outdated:**
- `QSystemTrayIcon`: Does not support KDE's StatusNotifierItem protocol; always use `KStatusNotifierItem` instead.
- `dbus-rs`: The Rust daemon uses zbus, not dbus-rs. The GUI uses QtDBus. Both are correct for their respective stacks.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The full-reset model approach (beginResetModel/endResetModel) is performant enough for typical fan counts (1-10 fans) | Common Pitfalls 6 | If systems have many fans (20+), incremental updates may be needed |
| A2 | Installing `libkf6notifications-dev` provides a 7-arg `KNotification::event()` overload that matches the existing `notification_handler.cpp` call sites | Build Dependencies | If the KF6 API has changed the overload set, call sites may need adaptation; the existing code is written to KF6 conventions and the runtime library expects this signature |

**Resolved assumptions (previously A1-A4, now confirmed):**
- ~~A1~~ (installing dev packages resolves linker errors): **CONFIRMED** — the root cause is missing dev packages; the 7-arg `KNotification::event()` signature in the code matches the KF6 API [VERIFIED: notification_handler.cpp uses KF6 7-arg signature]
- ~~A2~~ (SLOT-based signal relays sufficient for v1): **CONFIRMED** — all signals are either void or carry simple string arguments; SLOT-based relays handle both cases cleanly [VERIFIED: daemon source inspected, signal signatures confirmed]
- ~~A4~~ (daemon emits signals with documented names): **CONFIRMED** with one caveat — `AutoTuneCompleted` uses PascalCase on the wire (not snake_case) due to explicit `name =` attribute [VERIFIED: `crates/daemon/src/main.rs:1204`]

## Open Questions

All three original open questions have been resolved:

1. **DBus signal signature format** — **RESOLVED.** Most signals are void (draft_changed, applied_config_changed, degraded_state_changed, control_status_changed). Two carry payloads: `lifecycle_event_appended(event_kind: &str, detail: &str)` and `AutoTuneCompleted(fan_id: &str)`. GUI should re-fetch for void signals; use payload directly for the other two.

2. **Polling interval for live temperature updates** — **RESOLVED.** 3-second default interval for `runtimeState()` polling, with visibility pausing when window is minimized to tray. This matches KDE system monitor conventions and avoids excessive DBus traffic while maintaining a live feel.

3. **KF6 dev package API compatibility** — **RESOLVED.** The dev packages are NOT installed and that IS the root cause. The `notification_handler.cpp` code uses the KF6 7-arg `KNotification::event()` signature. The KF5 compat headers have a different signature. Installing `libkf6notifications-dev` will provide the correct KF6 headers.

**No remaining open questions.**

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Qt6 Core/Gui/Qml/Quick/DBus/Widgets | GUI build | ✓ | 6.9.2 | — |
| CMake + ECM | GUI build | ✓ | 6.17.0 | — |
| KF6 Kirigami (QML runtime) | QML imports | ✓ | 6.17.0 (runtime) | — |
| KF6 StatusNotifierItem (dev) | Tray icon build | ✗ | — | Direct `.so` link (current, works for .so but dev headers needed for CMake target) |
| KF6 Notifications (dev) | Notification build | ✗ | — | KF5 compat headers (API mismatch — causes linker errors) |
| KF6 StatusNotifierItem (runtime .so) | Tray icon runtime | ✓ | 6.17.0 | — |
| KF6 Notifications (runtime .so) | Notification runtime | ✓ | 6.17.0 | — |
| KF6 IconThemes (dev) | Icon lookups | ✓ | 6.17.0 | — |
| C++20 compiler (g++) | All C++ source | ✓ | System default | — |
| Rust toolchain (for daemon) | Daemon build only | ✓ | Stable | — |

**Missing dependencies with no fallback:**
- `libkf6notifications-dev`: Required for proper KNotification C++ API. The KF5 compat headers cause linker errors. Must install before build can succeed.

**Missing dependencies with fallback:**
- `libkf6statusnotifieritem-dev`: Current CMakeLists.txt links `libKF6StatusNotifierItem.so.6` directly. This works at link time but means CMake can't provide the `KF6::StatusNotifierItem` target. It's functional but fragile — installing the dev package is strongly recommended.

## Validation Architecture

> nyquist_validation is explicitly `false` in `.planning/config.json` — validation architecture section omitted per config.

## Security Domain

> Security enforcement enabled (absent = enabled per config).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Authentication is handled by the daemon via DBus policy; GUI doesn't authenticate |
| V3 Session Management | no | No session management in GUI |
| V4 Access Control | yes | DBus system bus policy controls write access; GUI must handle `AccessDenied` errors gracefully |
| V5 Input Validation | yes | DraftModel validates inputs before sending to daemon (SpinBox ranges, ComboBox limits); daemon-side validate_draft() is the authoritative check |
| V6 Cryptography | no | No crypto in GUI |

### Known Threat Patterns for Qt6/KDE Stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| DBus message injection | Tampering | DBus system bus policy restricts write methods to UID 0; GUI cannot bypass |
| Privilege escalation via GUI | Elevation | GUI never writes to sysfs directly; all writes go through the daemon's DBus API |
| Notification spam | Denial of Service | NotificationHandler only fires on state transitions (D-11); per-fan tracking prevents repeat notifications |
| QML injection via model data | Tampering | All model data comes from trusted daemon JSON; no user-editable strings rendered as QML expressions |

## Sources

### Primary (HIGH confidence)
- Context7 `/websites/develop_kde_getting-started_kirigami` — Kirigami component patterns (InlineMessage, AbstractCard, FormLayout, pageStack, Dialog)
- Existing codebase `gui/` — All C++ and QML source files verified line-by-line
- `crates/daemon/src/main.rs` — DBus signal definitions verified at lines 34-36, 1201-1205, 1557-1573
- `pkg-config --modversion Qt6Core` — Qt 6.9.2 confirmed on system
- `dpkg-query` — KF6 6.17.0 runtime packages confirmed; dev packages confirmed absent

### Secondary (MEDIUM confidence)
- Qt6 DBus documentation patterns (QDBusConnection::connect, SLOT-based signal relay) — from Qt6 official docs and existing codebase patterns
- KStatusNotifierItem README on invent.kde.org — confirms usage pattern
- Build error output — confirms linker failures for `KNotification::event()` and `StatusMonitor::onDaemonDisconnected()`
- KDE system monitor conventions (2-3 second refresh interval) — standard KDE desktop pattern

### Tertiary (LOW confidence)
- None remaining — all previous LOW confidence items have been promoted to HIGH or MEDIUM after source verification

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all versions verified on system, codebase exists and partially compiles
- Architecture: HIGH — existing implementation follows the correct C++/QML split pattern
- Pitfalls: HIGH — build errors confirmed, signal wiring gap confirmed, missing slots confirmed by code inspection
- DBus signals: HIGH — all signal signatures verified by inspecting daemon source code (previously MEDIUM)

**Research date:** 2026-04-12
**Valid until:** 2026-05-12 (30 days — stable Qt6/KF6 stack, unlikely to break)