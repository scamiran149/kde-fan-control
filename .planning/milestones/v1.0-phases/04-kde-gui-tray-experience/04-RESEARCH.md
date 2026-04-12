# Phase 4: KDE GUI & Tray Experience — Research

**Researched:** 2026-04-11 (updated)
**Domain:** KDE Qt6/QML GUI application with DBus client, system tray, and notifications
**Confidence:** HIGH

## Summary

Phase 4 builds a KDE-native Qt6/QML GUI and system tray experience on top of the existing daemon-owned DBus contract. The GUI is a pure DBus client — it reads runtime state, edits staged draft configuration, triggers auto-tuning, and receives tray-visible fault or alert feedback, all without bypassing the daemon or inventing a second authority surface.

The core architecture is a C++/QML split: C++ QObject subclasses (DaemonInterface, models, TrayIcon, NotificationHandler) bind to the DBus system bus via QtDBus, parse JSON from the daemon, expose parsed data as Q_PROPERTY and QAbstractListModel subclasses, and invoke daemon methods via Q_INVOKABLE slots. The QML layer is pure declarative using Kirigami.ApplicationWindow, pageStack navigation, ScrollablePage, AbstractCard, FormLayout, InlineMessage, TabBar + StackLayout, and KStatusNotifierItem for tray integration.

A substantial implementation already exists in `gui/` with all the core C++ backend classes and QML pages, but the build has linker errors that must be resolved before the GUI can be considered functional. The most critical gap is the missing KF6 dev packages — the CMakeLists.txt works around this by linking directly to `.so.6` shared library files, but the KNotification API signature doesn't match the headers found under `/usr/include/KF5/`.

**Primary recommendation:** Fix the KF6 link-time issues first (install `libkf6notifications-dev` and `libkf6statusnotifieritem-dev`, or correct the API call signature to match the available KF5 compat headers). Then address the `StatusMonitor::onDaemonDisconnected()` missing slot. After the build links cleanly, focus on DBus signal wiring (the `connectDBusSignals()` method is currently a no-op comment), DBus thread safety, and live data refresh timing.

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
| GUI-01 | Overview-first dashboard with fan state and live metrics | Kirigami CardsListView + FanRowDelegate pattern, FanListModel merge architecture |
| GUI-02 | Per-fan detail page with draft editing and explicit apply | Kirigami ScrollablePage + FormLayout pattern, DraftModel DBus write contract |
| GUI-03 | Optional wizard configuration path | Kirigami.Dialog multi-step wizard pattern |
| GUI-04 | System tray with status-first inspection | KStatusNotifierItem + TrayPopover QML integration |
| GUI-05 | Desktop notifications on alert transitions | KNotification event() API with .notifyrc configuration |
</phase_requirements>

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

### Pattern 3: InlineMessage for Status Banners
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

### Pattern 4: Kirigami Page Stack Navigation
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

### Anti-Patterns to Avoid
- **QML XMLHttpRequest for DBus:** Don't make direct DBus calls from QML. All DBus communication goes through the C++ DaemonInterface layer. [VERIFIED: existing codebase enforces this]
- **QSystemTrayIcon:** Not native on KDE; always use KStatusNotifierItem. [CITED: STACK.md]
- **Synchronous DBus calls:** Never use `QDBusInterface::call()` — it blocks the event loop. Always use `asyncCall()`. [VERIFIED: existing daemon_interface.cpp uses asyncCall]
- **Direct sysfs writes from GUI:** Breaks privilege boundaries. All writes go through the daemon's DBus API. [CITED: PROJECT.md]
- **Model data as raw JSON in QML:** Don't pass JSON strings to QML. Parse in C++ and expose structured types via Q_PROPERTY and QAbstractListModel roles. [VERIFIED: existing codebase does this correctly]

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| DBus proxy classes | Custom DBus transport or manual `QDBusMessage` construction for every call | QtDBus `QDBusInterface::asyncCall()` with `QDBusPendingCallWatcher` | QtDBus handles marshalling, type conversion, and error propagation; the existing DaemonInterface wrapper is sufficient [VERIFIED: existing codebase] |
| State management in QML | Custom reactive framework or ViewModel pattern in QML | Qt's property system (Q_PROPERTY with NOTIFY) and QAbstractListModel | QML bindings react automatically to property change notifications; no need for a custom state manager [VERIFIED: existing codebase] |
| JSON parsing | nlohmann/json or custom parser | QJsonDocument / QJsonObject / QJsonArray | Qt's JSON types are sufficient and consistent with the stack; no extra dependency needed [VERIFIED: existing codebase] |
| Tray icon API | QSystemTrayIcon with platform-specific hacks | KStatusNotifierItem from KF6 | QSystemTrayIcon is not native on KDE Plasma; KStatusNotifierItem supports the StatusNotifierItem/Freedesktop spec [CITED: STACK.md] |
| Desktop notifications | Custom popup windows or QML overlay dialogs | KNotification::event() with .notifyrc configuration | KNotification integrates with KDE's notification system and respects D-Bus–activated notification daemons [VERIFIED: existing notification_handler.cpp + kdefancontrol.notifyrc] |
| DBus signal subscription | Polling on a timer | QDBusConnection::connect() with slot-based relay | The current codebase's `connectDBusSignals()` is a no-op comment — this must be implemented using `QDBusConnection::systemBus().connect()` [VERIFIED: status_monitor.cpp line 119-136 is empty] |

**Key insight:** The largest gap in the existing implementation is that DBus signal subscription in `StatusMonitor::connectDBusSignals()` is currently a no-op comment. The GUI relies on polling via `refreshAll()` instead of reactive signal-driven updates. This means real-time state changes (degraded transitions, control status changes, draft updates) are only visible when the user triggers a refresh or the daemon comes online. The planner must add a signal subscription task.

## Common Pitfalls

### Pitfall 1: KF6 Dev Package Absence — Build Fails at Link Time
**What goes wrong:** The CMakeLists.txt links KF6StatusNotifierItem and KF6Notifications by directly referencing `.so.6` files because the `-dev` packages (CMake configs and public headers) are not installed. The current KF5 compat headers at `/usr/include/KF5/KNotifications/` use a different API signature than the KF6 runtime libraries, causing linker mismatches.
**Why it happens:** On Ubuntu/Debian, KF6 runtime packages are installed but dev packages are separately packaged. The CMakeLists.txt tries `find_package(KF6StatusNotifierItem QUIET)` which silently fails, then falls back to direct `.so` linking.
**How to avoid:** Install `libkf6notifications-dev` and `libkf6statusnotifieritem-dev` packages. Update CMakeLists.txt to use proper CMake targets (`KF6::Notifications`, `KF6::StatusNotifierItem`) when found, and fail noisily if they're missing instead of silently falling back to `.so` paths.
**Warning signs:** Linker errors like `undefined reference to KNotification::event(...)` — signature mismatch between KF5 headers and KF6 library. [VERIFIED: build attempted and failed with this exact error]

### Pitfall 2: DBus Signal Wiring Not Implemented
**What goes wrong:** `StatusMonitor::connectDBusSignals()` is an empty function with comments about SLOT-based connections. The GUI doesn't receive real-time updates when the daemon emits `draft_changed`, `control_status_changed`, `degraded_state_changed`, `lifecycle_event_appended`, or `AutoTuneCompleted`.
**Why it happens:** Qt6's `QDBusConnection::connect()` doesn't support lambda callbacks; it requires SLOT-compatible signatures or QObject receiver + slot string. The implementer left this as a TODO.
**How to avoid:** Create dedicated relay slots on StatusMonitor (e.g., `onDBusSignalDraftChanged()`) and connect them via `QDBusConnection::systemBus().connect(service, path, interface, signalName, receiver, slot)`. Each relay slot then calls `QMetaObject::invokeMethod()` with `Qt::QueuedConnection` to marshal updates to the main thread.
**Warning signs:** GUI doesn't update until user navigates away and back, or daemon state changes are invisible until a manual refresh. [VERIFIED: status_monitor.cpp lines 119-136]

### Pitfall 3: DBus Signal Threading
**What goes wrong:** QtDBus signals arrive on the DBus thread, not the main thread. Updating QML-visible properties from the DBus thread without marshaling causes crashes or undefined behavior.
**Why it happens:** `QDBusConnection::connect()` delivers signals on the connection's internal thread.
**How to avoid:** Always use `QMetaObject::invokeMethod(this, [this]() { /* update QML properties */ }, Qt::QueuedConnection)` in signal handlers. The existing code already uses this pattern in the auto-tune result handler but it must be applied consistently to all signal-driven updates.
**Warning signs:** Random crashes, property bindings not updating, or Model/View corruption during rapid state changes. [VERIFIED: existing codebase uses QMetaObject::invokeMethod in some places]

### Pitfall 4: Missing `onDaemonDisconnected()` Slot
**What goes wrong:** The current build fails with `undefined reference to StatusMonitor::onDaemonDisconnected()` because the slot is declared in the header but not defined in the .cpp file.
**Why it happens:** Implementation was incomplete.
**How to avoid:** Add the missing implementation to `status_monitor.cpp`. It should clear models and set daemon connection state to false.
**Warning signs:** Linker error: `undefined reference to StatusMonitor::onDaemonDisconnected()` [VERIFIED: build output shows this exact error]

### Pitfall 5: JSON Parsing Overhead on Every Refresh
**What goes wrong:** The `FanListModel::refresh()` method parses three JSON strings (inventory + runtime + draft) and does a full `beginResetModel()`/`endResetModel()` cycle on every call, even when only one source changed.
**Why it happens:** The current architecture takes the simplest correct approach — merge everything from scratch when any source updates. This avoids partial-update bugs but is wasteful.
**How to avoid:** For v1, the full-reset approach is acceptable. If performance becomes an issue at scale (many fans, high refresh rate), switch to incremental updates using `beginInsertRows`/`beginDataChanged` instead of `beginResetModel`. Don't optimize prematurely.
**Warning signs:** UI lag when refreshing with more than ~20 fans; visible flicker in the overview list. [ASSUMED — no evidence of actual performance issues yet]

### Pitfall 6: QML Module Registration Mismatch
**What goes wrong:** `qt_add_qml_module()` generates QML type registration. If the module URI in CMakeLists.txt doesn't match the QML `import` statement, types are invisible at runtime.
**Why it happens:** The URI must be exactly `org.kde.fancontrol` in both CMake and QML.
**How to avoid:** The existing code uses `URI org.kde.fancontrol` in CMakeLists.txt and `import org.kde.fancontrol` in QML files — this is correct and consistent.
**Warning signs:** QML runtime error: "module org.kde.fancontrol is not installed" or QML types not found. [VERIFIED: URI is consistent across codebase]

### Pitfall 7: Temperature Units
**What goes wrong:** The daemon stores temperatures in millidegrees Celsius. The GUI must display as °C with one decimal and convert back on input.
**Why it happens:** sysfs convention uses millidegrees; human display uses Celsius.
**How to avoid:** Use the helper functions in `types.cpp` (`millidegreesToCelsius()`, `formatTemperature()`) for display, and multiply by 1000 when sending set-point values back to the daemon. The DraftModel already handles this with `targetTempCelsius` and `targetTempMillidegrees` properties.
**Warning signs:** Temperature showing as "65000 °C" instead of "65.0 °C". [VERIFIED: existing codebase handles this correctly]

### Pitfall 8: Kirigami Page Lifecycle
**What goes wrong:** Pages pushed onto `pageStack` are not destroyed on pop by default. Detail pages accumulate in memory if the user navigates back and forth repeatedly.
**Why it happens:** Kirigami's pageStack keeps pages alive for smooth transitions.
**How to avoid:** For the overview page, this is fine — it should persist. For detail pages that are created with `Qt.resolvedUrl()` (inline creation), they are re-created on each push. The current code uses this pattern correctly. Don't use singleton detail pages with persistent state — create new instances per drill-in.
**Warning signs:** Memory growth after repeated navigation; stale data in detail pages. [VERIFIED: existing code creates detail pages inline per push]

### Pitfall 9: System Bus Access Policy
**What goes wrong:** The GUI runs as a normal user. The daemon runs as root on the system bus. Write methods require UID 0. If the DBus policy file (`org.kde.FanControl.conf`) doesn't allow the user to send to the daemon, all method calls fail.
**Why it happens:** DBus system bus policies default-deny for write methods on privileged services.
**How to avoid:** The existing `org.kde.FanControl.conf` must allow local users to send messages to the daemon. Read methods should be accessible to all local users; write methods are restricted to root. The GUI's authorization pattern is: try the call, surface the authorization error if it fails. Don't pre-check permissions.
**Warning signs:** All write operations fail silently or with "AccessDenied" errors. [VERIFIED: packing/dbus/org.kde.FanControl.conf exists]

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

### KNotification Event with .notifyrc
```cpp
// Source: existing gui/src/notification_handler.cpp
KNotification *n = KNotification::event(
    QStringLiteral("fallback-active"),     // event ID from .notifyrc
    QStringLiteral("Fallback active"),      // title
    QStringLiteral("Managed fans..."),     // body text
    QStringLiteral("dialog-error-symbolic"), // icon
    nullptr,                                 // widget
    KNotification::CloseOnTimeout,          // flags
    QStringLiteral("kdefancontrol.notifyrc") // config file
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

### DBus Signal Subscription (Pattern to Implement)
```cpp
// This must be added to StatusMonitor::connectDBusSignals()
// Source: pattern derived from Qt6 QDBusConnection docs
void StatusMonitor::connectDBusSignals()
{
    QDBusConnection bus = QDBusConnection::systemBus();

    // Draft changed signal
    bus.connect(s_service,
                s_lifecyclePath,
                s_lifecycleIface,
                QStringLiteral("draft_changed"),
                this,
                SLOT(onDBusDraftChanged()));

    // Control status changed signal
    bus.connect(s_service,
                s_controlPath,
                s_controlIface,
                QStringLiteral("control_status_changed"),
                this,
                SLOT(onDBusControlStatusChanged()));

    // Degraded state changed signal
    bus.connect(s_service,
                s_lifecyclePath,
                s_lifecycleIface,
                QStringLiteral("degraded_state_changed"),
                this,
                SLOT(onDBusDegradedStateChanged()));

    // Lifecycle event appended signal
    bus.connect(s_service,
                s_lifecyclePath,
                s_lifecycleIface,
                QStringLiteral("lifecycle_event_appended"),
                this,
                SLOT(onDBusLifecycleEventAppended()));

    // Auto-tune completed signal
    bus.connect(s_service,
                s_controlPath,
                s_controlIface,
                QStringLiteral("AutoTuneCompleted"),
                this,
                SLOT(onDBusAutoTuneCompleted()));
}

void StatusMonitor::onDBusDraftChanged()
{
    QMetaObject::invokeMethod(this, [this]() {
        m_daemon->draftConfig();
    }, Qt::QueuedConnection);
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| QSystemTrayIcon | KStatusNotifierItem | KF5 era (2014+) | QSystemTrayIcon doesn't work on modern KDE Plasma; SNI is the standard [VERIFIED: STACK.md] |
| KF5 compat headers | KF6 proper dev packages | KF6 release (2023+) | KF5 headers at /usr/include/KF5/ are backward-compat stubs; KF6 CMake configs are required for proper linking [VERIFIED: /usr/include/KF5/KNotifications/ exists alongside KF6 .so] |
| Polling-based refresh | Signal-driven updates | Always preferred | DBus signals are the reactive approach; polling is the fallback the current code uses because connectDBusSignals() is not yet implemented [VERIFIED: status_monitor.cpp] |
| QML_INLINE_COMPONENTS | QML_ELEMENT + qt_add_qml_module | Qt 6.2+ | The standard Qt6 QML type registration approach; used in the existing codebase [VERIFIED: CMakeLists.txt uses qt_add_qml_module] |
| Kirigami.Dialog with custom buttons | Kirigami.Dialog with standardButtons | Kirigami 6.x | Kirigami.Dialog replaces Qt.Dialog for KDE apps; used in WizardDialog.qml [VERIFIED: existing codebase] |

**Deprecated/outdated:**
- `QSystemTrayIcon`: Does not support KDE's StatusNotifierItem protocol; always use `KStatusNotifierItem` instead.
- `dbus-rs`: The Rust daemon uses zbus, not dbus-rs. The GUI uses QtDBus. Both are correct for their respective stacks.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Installing `libkf6notifications-dev` and `libkf6statusnotifieritem-dev` will resolve the linker errors | Common Pitfalls 1 | If the KF6 API signature differs from KF5 compat headers, additional adaptation work is needed |
| A2 | DBus signal subscription via QDBusConnection::connect() with SLOT-based relays is sufficient for v1 | Common Pitfalls 2 | If signal arguments need parsing (e.g., draft_changed carries JSON), additional argument extraction is needed |
| A3 | The full-reset model approach (beginResetModel/endResetModel) is performant enough for typical fan counts (1-10 fans) | Common Pitfalls 5 | If systems have many fans (20+), incremental updates may be needed |
| A4 | The daemon emits DBus signals with the exact names documented (draft_changed, control_status_changed, degraded_state_changed, lifecycle_event_appended, AutoTuneCompleted) | Architecture | If signal names differ, the subscription code will silently fail to connect; verify against daemon source |

**If this table is empty:** All claims in this research were verified or cited — no user confirmation needed.

## Open Questions

1. **DBus signal signature format**
   - What we know: The daemon uses zbus to emit signals. zbus signals on the system bus follow DBus conventions. The signal names are listed in CONTEXT.md.
   - What's unclear: Whether the daemon's signals carry arguments (e.g., `draft_changed` might carry the changed JSON, or it might be a void signal that requires a follow-up method call). The current StatusMonitor assumes void signals and re-fetches data.
   - Recommendation: Inspect `crates/daemon/src/main.rs` signal emission code to verify exact signatures. If signals carry JSON, parse the argument directly instead of re-fetching. If void, the current re-fetch approach is correct.

2. **Polling interval for live temperature updates**
   - What we know: The overview and tray show live temperature, RPM, and output percentage. The daemon's control loop runs at configurable intervals (default: sample=1000ms, control=2000ms, write=2000ms).
   - What's unclear: How often the GUI should refresh runtime state for live-feeling updates. No timer-based polling is currently implemented — the GUI only refreshes when the user navigates or the daemon connection state changes.
   - Recommendation: Add a refresh timer (e.g., 2-5 seconds) on the StatusMonitor for runtime state polling, and rely on DBus signals for event-driven updates (draft changes, degraded transitions, auto-tune completion). This gives the dashboard a "live" feel without excessive DBus traffic.

3. **KF6 dev package API compatibility**
   - What we know: The KF5 compat headers at `/usr/include/KF5/KNotifications/` expose `KNotification::event()` with a signature that uses `QWidget*` and `QFlags<KNotification::NotificationFlag>`. The KF6 runtime library may have a different signature (e.g., using `QWindow*` instead of `QWidget*`).
   - What's unclear: Whether installing `libkf6notifications-dev` provides updated headers that match the KF6 runtime library, or whether the KF5 compat headers are the only option.
   - Recommendation: Install the dev packages and verify the API. If the signature differs, adapt the call site in `notification_handler.cpp`.

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
- `libkf6notifications-dev`: Required for proper KNotification C++ API. The KF5 compat headers cause linker errors. Must install.

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
- `pkg-config --modversion Qt6Core` — Qt 6.9.2 confirmed on system
- `dpkg-query` — KF6 6.17.0 runtime packages confirmed

### Secondary (MEDIUM confidence)
- Qt6 DBus documentation patterns (QDBusConnection::connect, SLOT-based signal relay) — from Qt6 official docs and existing codebase patterns
- KStatusNotifierItem README on invent.kde.org — confirms usage pattern
- Build error output — confirms linker failures for `KNotification::event()` and `StatusMonitor::onDaemonDisconnected()`

### Tertiary (LOW confidence)
- KF6 API signature compatibility between KF5 compat headers and KF6 runtime library — assumed based on build failure, needs verification after installing dev packages

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all versions verified on system, codebase exists and partially compiles
- Architecture: HIGH — existing implementation follows the correct C++/QML split pattern
- Pitfalls: HIGH — build errors confirmed, signal wiring gap confirmed by code inspection
- DBus signals: MEDIUM — signal names listed in CONTEXT.md but exact signatures need verification against daemon source

**Research date:** 2026-04-11
**Valid until:** 2026-05-11 (30 days — stable Qt6/KF6 stack, unlikely to break)