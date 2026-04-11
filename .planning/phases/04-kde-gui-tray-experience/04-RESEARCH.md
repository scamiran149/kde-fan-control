# Phase 4: KDE GUI & Tray Experience — Research

**Gathered:** 2026-04-11
**Status:** Complete

## Stack

### GUI Layer
| Technology | Version | Purpose |
|---|---|---|
| Qt | 6.8+ (floor) | GUI foundation, DBus client, system tray integration |
| Qt Quick / QML | 6 | Scene and UI framework |
| Qt Quick Controls 2 | 6 | Standard controls (TextField, ComboBox, TabBar, Slider, CheckBox) |
| Kirigami | 6.x | KDE app shell, page navigation, AbstractCard, FormLayout, InlineMessage, Actions, GlobalDrawer, ScrollablePage |
| KStatusNotifierItem | KF6 current | System tray icon per KDE/Freedesktop spec |
| CMake + Extra CMake Modules (ECM) | Current distro | Build system |
| C++20 | Compiler floor | Backend glue layer (QObject subclasses exposed to QML) |

### Existing Backend (DBus Server)
| Interface | Key Methods | Key Signals |
|---|---|---|
| `org.kde.FanControl.Inventory` | `snapshot()`, `set_sensor_name()`, `set_fan_name()`, `remove_sensor_name()`, `remove_fan_name()`, `apply_names_to_snapshot()` | — |
| `org.kde.FanControl.Lifecycle` | `get_draft_config()`, `get_applied_config()`, `get_degraded_summary()`, `get_lifecycle_events()`, `get_runtime_state()`, `set_draft_fan_enrollment()`, `remove_draft_fan()`, `discard_draft()`, `validate_draft()`, `apply_draft()` | `draft_changed`, `applied_config_changed`, `degraded_state_changed`, `lifecycle_event_appended` |
| `org.kde.FanControl.Control` | `get_control_status()`, `get_auto_tune_result()`, `start_auto_tune()`, `accept_auto_tune()`, `set_draft_fan_control_profile()` | `control_status_changed`, `AutoTuneCompleted` |

### Core types the GUI must consume (from `kde-fan-control-core`)
| Type | Location | Purpose |
|---|---|---|
| `InventorySnapshot`, `HwmonDevice`, `TemperatureSensor`, `FanChannel` | `core/src/inventory.rs` | Hardware listing and capabilities |
| `ControlMode` (PWM,Voltage), `SupportState` (Available,Partial,Unavailable) | `core/src/inventory.rs` | Fan mode and support enumeration |
| `AppConfig`, `DraftConfig`, `DraftFanEntry`, `AppliedConfig`, `AppliedFanEntry`, `FriendlyNames` | `core/src/config.rs` | Configuration model, draft/apply, friendly names |
| `AggregationFn`, `PidGains`, `ControlCadence`, `ActuatorPolicy`, `PidLimits` | `core/src/control.rs` | Control profile types |
| `RuntimeState`, `FanRuntimeStatus`, `ControlRuntimeSnapshot`, `DegradedReason` | `core/src/lifecycle.rs` | Live runtime state |
| `FallbackResult`, `FallbackIncident`, `LifecycleEvent` | `core/src/lifecycle.rs` | Fallback and event tracking |
| `ReconcileOutcome`, `ReconcileResult` | `core/src/lifecycle.rs` | Boot reconciliation results |

## Architecture

### Recommended architecture: C++/QML split
- **C++ layer**: QObject subclasses that bind to the DBus system bus via QtDBus, parse JSON from the daemon, expose parsed data as Q_PROPERTY and QAbstractListModel subclasses, and invoke daemon methods via Q_INVOKABLE slots. This layer owns the DBus connection, signal subscriptions, and data transformation.
- **QML layer**: Pure declarative UI using Kirigami.ApplicationWindow, pageStack navigation, ScrollablePage, AbstractCard/FormLayout, InlineMessage for banners, and TabBar + StackLayout for advanced detail tabs. All data comes from C++ context properties or registered types — no QML XMLHttpRequest or direct DBus from QML.
- **Rationale**: The daemon exposes all methods returning `String` (JSON-serialized). The C++ layer parses JSON into structured QML-friendly types. Qt6 QML has no native zbus; QtDBus (C++) is the natural DBus bridge. This matches the STACK.md recommendation of "Qt/Kirigami + small C++ glue layer."

### App structure
```
gui/
  CMakeLists.txt
  src/
    main.cpp                  — Application entry, DBus connection, context property registration
    daemon_interface.h/.cpp   — QtDBus abstraction: Inventory, Lifecycle, Control interfaces
    models/
      fan_list_model.h/.cpp   — QAbstractListModel for fan overview
      sensor_list_model.h/.cpp — QAbstractListModel for sensor listing
      lifecycle_event_model.h/.cpp — QAbstractListModel for lifecycle events
    types.h/.cpp              — QObject value types for FanState, ControlProfile, etc.
    status_monitor.h/.cpp     — Signal subscription, polling, reactive state updates
  qml/
    Main.qml                  — Kirigami.ApplicationWindow
    OverviewPage.qml          — Fan overview dashboard
    InventoryPage.qml         — Sensor/fan discovery list
    FanDetailPage.qml         — Editing, runtime, advanced tabs
    WizardDialog.qml          — Guided setup wizard
    TrayIcon.qml              — (system tray managed from C++)
    delegates/
      FanRowDelegate.qml      — Compact fan row for overview
      FanTrayDelegate.qml     — Tray popover fan row
    components/
      StateBadge.qml          — Status badge (managed, unmanaged, degraded, fallback, etc.)
      OutputBar.qml           — PWM/output percentage bar
      TemperatureDisplay.qml  — Temperature with °C suffix
```

### Build integration
- Add `gui/` subdirectory to the workspace `Cargo.toml` is NOT needed — the GUI builds via CMake independently.
- Root `CMakeLists.txt` or top-level Makefile orchestrates: `cargo build --release` for daemon/CLI, then `cmake --build` for GUI.
- The GUI build must `find_package(Qt6 REQUIRED COMPONENTS Core Quick Qml QuickControls2 DBus Widgets)` and `find_package(KF6 REQUIRED COMPONENTS Kirigami IconThemes)`.
- KStatusNotifierItem is typically found via `find_package(KF6 REQUIRED COMPONENTS StatusNotifierItem)` or the framework's `KNotification` which includes it.

## DBus Integration Pattern

### Method call pattern
All daemon methods return JSON strings. The C++ bridge:
1. Calls `QDBusInterface::asyncCall()` on the system bus to `org.kde.FanControl` at `/org/kde/FanControl`.
2. Connects `QDBusPendingCallWatcher::finished` to parse the JSON reply with `QJsonDocument::fromJson()`.
3. Updates Q_PROPERTY values or QAbstractListModel entries from the parsed data.
4. QML bindings react automatically through property bindings.

### Signal subscription pattern
DBus signals (`draft_changed`, `control_status_changed`, `degraded_state_changed`, `lifecycle_event_appended`, `AutoTuneCompleted`) are connected via `QDBusConnection::connect()` on the system bus. Each signal handler updates the relevant C++ model, which propagates to QML through property change notifications.

### Authorization pattern
Write methods (`set_draft_fan_enrollment`, `set_draft_fan_control_profile`, `discard_draft`, `validate_draft`, `apply_draft`, `start_auto_tune`, `accept_auto_tune`) require UID 0 (root). The GUI must:
1. Try the call.
2. If authorization fails, display the inline error message per UI-SPEC.
3. Never pre-check or silently skip writes.

## Common Pitfalls

1. **JSON parsing overhead**: The daemon returns opaque JSON strings. The C++ bridge must parse every response. Use `QJsonDocument::fromJson()` and cache parsed results. Don't re-parse on every property read.

2. **DBus signal threading**: QtDBus signals arrive on the DBus thread. Update QML-visible properties on the main thread using `QMetaObject::invokeMethod()` with `Qt::QueuedConnection`.

3. **QAbstractListModel role names**: Role names must be exact string matches for QML property bindings. Use `QHash<int, QByteArray> roleNames() const override` consistently.

4. **System bus access**: The daemon runs as root on the system bus. The GUI runs as normal user. Read methods are accessible to all local users per `org.kde.FanControl.conf`. Write methods require UID 0. The DBus policy in the `.conf` file allows `send_destination` for all users.

5. **Kirigami page lifecycle**: Pages pushed onto `pageStack` are not destroyed on pop by default. Use `pageStack.clear()` or `StackView.onDestruction` for cleanup. The overview page should persist; detail pages are pushed/popped.

6. **QML module registration**: Qt6 uses `QML_ELEMENT` and `QML_SINGLETON` macros in C++ headers. The CMake `qt_add_qml_module()` call generates the QML type registration. Module URI must match (e.g., `org.kde.fancontrol`).

7. **Temperature units**: The daemon stores temperatures in millidegrees Celsius. The GUI must convert to °C with one decimal place for display and convert back on input. Output is a percentage (0–100%). RPM is an integer.

8. **Draft vs Applied state**: The GUI must track draft state locally and display it alongside applied state. The `draft_changed` signal invalidates local draft cache. The `apply_draft` method is async and may partially succeed.

9. **System tray lifecycle**: KStatusNotifierItem requires a QGuiApplication (or QApplication). The tray icon is set from C++ and the context menu/popover is also defined in C++ or QML. The popover is a QML component loaded by the tray.

10. **CMake vs Cargo**: The GUI is a CMake project. Don't try to build it with Cargo. Keep it as a separate build directory. The workspace `Cargo.toml` only lists Rust crates.

## Don't Hand-Roll

1. **DBus proxy generation**: Use Qt's `qdbusxml2cpp` to generate C++ adaptor/proxy classes from the daemon's introspection XML. Alternatively, write the proxy classes manually but model them on the QtDBus pattern — don't invent a custom DBus transport.

2. **State management**: Use Qt's property system (Q_PROPERTY with NOTIFY signals) and QAbstractListModel. Don't build a custom reactive framework in QML.

3. **JSON schema**: Parse daemon JSON with Qt's JSON types. Don't import nlohmann/json or another C++ JSON library — Qt's is sufficient and consistent with the stack.

4. **Tray icon API**: Use KStatusNotifierItem from KF6. Don't use QSystemTrayIcon — it's not native on KDE Plasma and doesn't support the StatusNotifierItem/Freedesktop spec.

## Validation Architecture

### Dimension coverage for Phase 4

| Dimension | Validation approach |
|---|---|
| D1 Contract Adherence | Verify GUI calls match daemon DBus method names and parameter types exactly |
| D2 Edge Cases | Test disconnected daemon, authorization failure, empty inventory, all fan states |
| D3 Error Propagation | Verify error banners appear for each DBus error type, stale data is labeled |
| D4 State Round-Trips | Verify draft → validate → apply → applied_config reflects in GUI |
| D5 Reactive Updates | Verify GUI updates on `draft_changed`, `control_status_changed`, `degraded_state_changed`, `AutoTuneCompleted` |
| D6 Authorization | Verify read methods succeed as normal user, write methods show authorization error |
| D7 Accessibility | Verify keyboard navigation, state text+icon+color, focus rings |
| D8 Build Verification | Verify CMake configure succeeds, QML loads, DBus connection establishes |