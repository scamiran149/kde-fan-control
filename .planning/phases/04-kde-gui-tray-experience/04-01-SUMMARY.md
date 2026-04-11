---
phase: 04-kde-gui-tray-experience
plan: 01
subsystem: ui
tags: [qt6, qml, kirigami, dbus, c++, kde, qabstractlistmodel]

# Dependency graph
requires:
  - phase: 03-temperature-control-runtime-operations
    provides: daemon DBus interfaces (Inventory, Lifecycle, Control) and runtime state data model
  - phase: 02-safe-enrollment-lifecycle-recovery
    provides: fan enrollment, lifecycle, and degraded-state data model
provides:
  - C++ DBus proxy classes for all three daemon interfaces
  - QAbstractListModel subclasses for fan overview and sensor inventory
  - QML Application Shell with Kirigami GlobalDrawer navigation
  - Overview dashboard with severity banners and fan card rows
  - Inventory read-only page for sensor and fan discovery
  - Reusable QML components (StateBadge, OutputBar, TemperatureDisplay)
affects: [04-kde-gui-tray-experience, system-tray, fan-detail-wizard]

# Tech tracking
tech-stack:
  added: [Qt6 Quick/Qml/DBus/Widgets, Kirigami 6 QML module, qt_add_qml_module build pattern]
  patterns: [DBus proxy pattern (DaemonInterface wraps QDBusInterface), reactive model refresh (StatusMonitor polls on daemon signals), QObject value types (FanStateInfo/SensorInfo), severity-sorted list model]

key-files:
  created:
    - gui/CMakeLists.txt
    - gui/src/main.cpp
    - gui/src/daemon_interface.h
    - gui/src/daemon_interface.cpp
    - gui/src/status_monitor.h
    - gui/src/status_monitor.cpp
    - gui/src/types.h
    - gui/src/types.cpp
    - gui/src/models/fan_list_model.h
    - gui/src/models/fan_list_model.cpp
    - gui/src/models/sensor_list_model.h
    - gui/src/models/sensor_list_model.cpp
    - gui/qml/Main.qml
    - gui/qml/OverviewPage.qml
    - gui/qml/InventoryPage.qml
    - gui/qml/delegates/FanRowDelegate.qml
    - gui/qml/components/StateBadge.qml
    - gui/qml/components/OutputBar.qml
    - gui/qml/components/TemperatureDisplay.qml
  modified: []

key-decisions:
  - "StatusMonitor uses refreshAll() polling instead of direct DBus signal subscriptions due to Qt6 QDBusConnection::connect() not supporting lambda callbacks"
  - "KF6Kirigami, KF6IconThemes, KF6StatusNotifierItem are available as QML modules at runtime but lack CMake dev packages — CMakeLists.txt uses find_package QUIET with conditional linking"
  - "CMakeLists.txt uses qt_add_qml_module() with URI org.kde.fancontrol and single shared library output"

patterns-established:
  - "DBus proxy pattern: DaemonInterface class wraps QDBusInterface instances for each daemon interface, exposes Q_INVOKABLE methods and Q_PROPERTY for connection state"
  - "Model refresh pattern: StatusMonitor triggers refreshAll() which calls DaemonInterface read methods and passes JSON results to FanListModel/SensorListModel refresh()"
  - "Severity sort: FanListModel provides severityOrder role and sorts by fallback→degraded→managed+highTemp→managed→unmanaged→partial→unavailable"
  - "Value type pattern: QObject-derived FanStateInfo/SensorInfo with Q_PROPERTY fields converted from daemon JSON strings"

requirements-completed: [GUI-01, GUI-05]

# Metrics
duration: 45min
completed: 2026-04-11
---

# Phase 04 Plan 01: KDE GUI Foundation Summary

**C++ DBus bridge and QML application shell with overview dashboard and inventory page for KDE fan control**

## Performance

- **Duration:** 45 min
- **Started:** 2026-04-11T22:16:00Z
- **Completed:** 2026-04-11T23:01:47Z
- **Tasks:** 2
- **Files modified:** 19

## Accomplishments
- C++ DBus proxy layer connecting to all three daemon interfaces (Inventory, Lifecycle, Control) with async call pattern and error surfacing
- QAbstractListModel subclasses with JSON parsing and severity-based sort order for fan rows and sensor listing
- KDE-native QML application shell with Kirigami GlobalDrawer navigation, overview dashboard with severity banners, and inventory read views
- Reusable QML components (StateBadge, OutputBar, TemperatureDisplay) for fan row rendering

## Task Commits

Each task was committed atomically:

1. **Task 1: CMake project bootstrap, C++ DBus proxy, and core model layer** - `34b49ec` (feat)
2. **Task 2: QML Application Shell, Overview Dashboard, and Inventory Page** - `3d996fd` (feat)

## Files Created/Modified
- `gui/CMakeLists.txt` - CMake build config with qt_add_qml_module, Qt6/KF6 dependency resolution
- `gui/src/main.cpp` - Application entry point wiring DaemonInterface, StatusMonitor, models as QML context properties
- `gui/src/daemon_interface.h/.cpp` - DBus proxy for Inventory, Lifecycle, Control interfaces with Q_INVOKABLE methods
- `gui/src/status_monitor.h/.cpp` - Daemon connection tracking and model refresh coordination
- `gui/src/types.h/.cpp` - FanStateInfo/SensorInfo QObject value types and format helpers
- `gui/src/models/fan_list_model.h/.cpp` - Severity-sorted QAbstractListModel for fan overview
- `gui/src/models/sensor_list_model.h/.cpp` - QAbstractListModel for sensor listing
- `gui/qml/Main.qml` - Kirigami.ApplicationWindow with GlobalDrawer navigation
- `gui/qml/OverviewPage.qml` - Fan dashboard with severity banners and CardsListView
- `gui/qml/InventoryPage.qml` - Sensor/fan discovery read views
- `gui/qml/delegates/FanRowDelegate.qml` - Compact fan row with state badge, temp, RPM, output bar
- `gui/qml/components/StateBadge.qml` - Traffic-light severity badge with high-temp alert overlay
- `gui/qml/components/OutputBar.qml` - PWM/output percentage bar with active/disabled states
- `gui/qml/components/TemperatureDisplay.qml` - Millidegrees-to-Celsius display

## Decisions Made
- StatusMonitor uses `refreshAll()` polling triggered by daemon-connected state rather than direct DBus signal subscriptions — Qt6's `QDBusConnection::connect()` doesn't support lambda callbacks, so a signal-based polling approach was used instead
- KF6Kirigami, KF6IconThemes, KF6StatusNotifierItem are available as runtime QML modules but have no CMake dev packages on this system — CMakeLists.txt uses `find_package(... QUIET)` with conditional linking and runtime QML imports handle the rest
- CMakeLists.txt produces a shared library (`libgui_app.so` + `libgui_appplugin.so`) via `qt_add_qml_module()` rather than a standalone executable — this is the standard Qt6 QML module pattern

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Added missing QtQuick.Controls import in OutputBar.qml**
- **Found during:** Task 2 (QML UI layer)
- **Issue:** OutputBar.qml used `Controls.Label` but was missing `import QtQuick.Controls as Controls`
- **Fix:** Added `import QtQuick.Controls as Controls` to OutputBar.qml
- **Files modified:** gui/qml/components/OutputBar.qml
- **Verification:** Build succeeds, QML cache compilation passes
- **Committed in:** 3d996fd (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Trivial import fix. No scope creep.

## Issues Encountered
- KF6 CMake packages for Kirigami, IconThemes, and StatusNotifierItem are not available as dev packages on this system, so they're handled via conditional linking and runtime QML resolution. This is expected for Kirigami which ships as a QML module.
- StatusMonitor's DBus signal subscription pattern was simplified to use `refreshAll()` polling instead of `QDBusConnection::connect()` with lambda callbacks due to Qt6 API limitations. The current approach works but should be revisited for true reactive DBus signal subscriptions in a future plan.

## Known Stubs
- StatusMonitor uses `refreshAll()` polling instead of true reactive DBus signal subscriptions — the QDBusConnection::connect() + lambda pattern doesn't work in Qt6. The `refreshAll()` approach is functional but less efficient than reacting to individual DBus signals. This should be enhanced with a dedicated signal-forwarding mechanism in a future plan.
- Fan detail page (navigated from FanRowDelegate) is stub — will be implemented in Plan 04-02
- Wizard configuration (toolbar action on OverviewPage) is stub — will be implemented in a future plan

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- C++ DBus bridge and model layer are ready for fan detail page and wizard configuration
- QML component library (StateBadge, OutputBar, TemperatureDisplay) is reusable for future pages
- StatusMonitor and DaemonInterface need true DBus signal subscriptions when a better pattern is established
- KF6StatusNotifierItem and KF6IconThemes will need CMake dev packages or alternative linking for system tray support (Plan 04-03+)

## Self-Check: PASSED

All 19 created files verified present. Both task commits (34b49ec, 3d996fd) verified in git log.

---
*Phase: 04-kde-gui-tray-experience*
*Completed: 2026-04-11*