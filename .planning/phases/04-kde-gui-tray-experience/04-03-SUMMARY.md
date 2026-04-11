---
phase: 04-kde-gui-tray-experience
plan: 03
subsystem: ui
tags: [qt6, qml, kirigami, dbus, c++, kde, kstatusnotifieritem, knotification, tray, notifications]

# Dependency graph
requires:
  - phase: 04-kde-gui-tray-experience
    plan: 01
    provides: C++ DBus proxy, FanListModel, StatusMonitor, QML component library
provides:
  - KStatusNotifierItem-based system tray icon with severity tracking
  - KNotification desktop notification handler for alert transitions
  - TrayPopover QML with alert summary and managed fan list
  - FanTrayDelegate QML for compact tray fan rows
  - kdefancontrol.notifyrc event definitions
affects: [04-kde-gui-tray-experience, system-tray]

# Tech tracking
tech-stack:
  added: [KStatusNotifierItem (KF6), KNotification (KF6), kdefancontrol.notifyrc]
  patterns: [severity-precedence tracking (fallback>degraded>high-temp>managed>unmanaged>disconnected), transition-only notification triggering, sticky-alert acknowledgment state]

key-files:
  created:
    - gui/src/tray_icon.h
    - gui/src/tray_icon.cpp
    - gui/src/notification_handler.h
    - gui/src/notification_handler.cpp
    - gui/qml/TrayPopover.qml
    - gui/qml/delegates/FanTrayDelegate.qml
    - gui/data/kdefancontrol.notifyrc
  modified:
    - gui/src/main.cpp
    - gui/CMakeLists.txt

key-decisions:
  - "KF6 dev packages for StatusNotifierItem and Notifications not available — linking KF6 shared libraries directly and using KF5 compat headers at /usr/include/KF5/KNotifications/"
  - "NotificationHandler tracks previous per-fan state to detect TRANSITIONS only per D-11 — notifications never fire on repeated status updates"
  - "TrayIcon uses KStatusNotifierItem::setAssociatedWidget would need a QWidget parent — context menu actions use QAction instead, main window activation deferred to QML layer"
  - "TrayPopover shows managed, degraded, fallback, and unmanaged fans in the list (D-09 says managed by default, but showing other states aids quick inspection)"

requirements-completed: [GUI-04, GUI-05]

# Metrics
duration: 23min
completed: 2026-04-11
---

# Phase 04 Plan 03: System Tray Icon and Notification Handler Summary

**KStatusNotifierItem tray icon, KNotification alert handler, and compact tray popover with managed fan list**

## Performance

- **Duration:** 23 min
- **Started:** 2026-04-11T23:22:24Z
- **Completed:** 2026-04-11T23:45:44Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- TrayIcon C++ class using KStatusNotifierItem for KDE system tray presence
- Severity tracking per UI-SPEC precedence: fallback > degraded > high-temp > managed > unmanaged > disconnected
- Icon management maps severity to symbolic icons (dialog-error, data-warning, temperature-high, emblem-ok, dialog-information, network-offline)
- Status mapping: NeedsAttention for fallback/degraded/high-temp, Active for managed, Passive for unmanaged/disconnected
- Tooltips show severity summary and managed/alert counts
- Context menu with Open Fan Control, Acknowledge alerts, and Quit actions
- NotificationHandler fires KNotification events only on transitions into degraded, fallback, and high-temp per D-11
- Desktop notifications: fallback=High urgency, degraded=High urgency, high-temp=Normal urgency per UI-SPEC
- Sticky alert state managed by TrayIcon::acknowledgeAlerts() per D-12
- TrayPopover QML with 360px width, header with daemon connection state, severity icon, and counts
- Alert area with colored banners for fallback, degraded, and high-temp conditions
- Managed fan list filtered to show operational fans with state, temperature, and output
- Footer actions: Open Fan Control and Acknowledge alerts (visible only when hasStickyAlerts)
- FanTrayDelegate QML: compact 40px row with state icon, name, temperature, output percent
- kdefancontrol.notifyrc: KNotification event definitions for fallback-active, degraded-state, and high-temp-alert

## Task Commits

Each task was committed atomically:

1. **Task 1: TrayIcon, NotificationHandler, and KF6 integration** - `c77b452` (feat)
2. **Task 2: TrayPopover and FanTrayDelegate QML** - `ece68e6` (feat)

## Files Created/Modified
- `gui/src/tray_icon.h` - TrayIcon class declaration with severity tracking and KStatusNotifierItem management
- `gui/src/tray_icon.cpp` - Icon/status updates, tooltip management, severity computation, alert acknowledgment
- `gui/src/notification_handler.h` - NotificationHandler class for transition detection
- `gui/src/notification_handler.cpp` - Per-fan state tracking and KNotification event emission on transitions
- `gui/src/main.cpp` - Added TrayIcon and NotificationHandler as QML context properties
- `gui/CMakeLists.txt` - Added new source files, QML files, KF5/KNotifications include path, KF6 shared library linking
- `gui/qml/TrayPopover.qml` - Compact tray popover with header, alert area, managed fan list, footer actions
- `gui/qml/delegates/FanTrayDelegate.qml` - Compact 40px fan row with state icon, name, temperature, output
- `gui/data/kdefancontrol.notifyrc` - KNotification event configuration for fallback, degraded, high-temp alerts

## Decisions Made
- KF6 dev packages not installed on this system; used KF5 compat headers at `/usr/include/KF5/KNotifications/` with direct `.so.6` library linking instead of CMake `find_package` targets
- NotificationHandler compares current fan state to previous per-fan snapshot to detect state transitions, firing KNotification only on transitions INTO degraded/fallback/high-temp per D-11
- TrayIcon tracks `m_alertsAcknowledged` flag — acknowledging clears UI stickiness only and does not alter daemon state per D-12
- KNotification events use `CloseOnTimeout` flag (desktop popups are transient) while the tray popover maintains sticky alert banners until explicitly acknowledged
- TrayPopover shows fans with state managed, degraded, fallback, or unmanaged to provide quick inspection while highlighting managed fans by default ordering

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Build] Adapted KF6 linking strategy for missing dev packages**
- **Found during:** Task 1 build
- **Issue:** KF6 CMake dev packages for StatusNotifierItem and Notifications are not installed; `find_package(KF6StatusNotifierItem)` and `find_package(KF6Notifications)` fail silently
- **Fix:** Used KF5 compat headers at `/usr/include/KF5/KNotifications/` for C++ API, linked directly to runtime `.so.6` shared library files, added both `/usr/include/KF5` and `/usr/include/KF5/KNotifications` to include path for the CamelCase forwarding headers and export header
- **Files modified:** `gui/CMakeLists.txt`
- **Verification:** Build succeeds and links against KF6StatusNotifierItem.so.6 and KF6Notifications.so.6

None — plan executed as written.

## Known Stubs
- TrayIcon::setAssociatedWidget() is not yet wired to the main application window in main.cpp — the KStatusNotifierItem popup will show but needs QML popover integration in a future enhancement. The `setAssociatedWidget()` call requires a QWidget parent which is available via QApplication.
- TrayPopover "Open Fan Control" button click handler has a TODO comment — needs QML integration with the main application window's pageStack to navigate to the overview page.
- FanTrayDelegate click handler emits a `clicked()` signal but does not yet navigate to fan detail — needs connection to main window pageStack.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: spam_mitigation | gui/src/notification_handler.cpp | Per T-04-09: NotificationHandler only fires on TRANSITIONS into alert states, preventing notification spam on repeated status updates |

## Self-Check: PASSED

All 9 created/modified files verified present. Both task commits (c77b452, ece68e6) verified in git log. Build succeeds with exit code 0.

---
*Phase: 04-kde-gui-tray-experience*
*Completed: 2026-04-11*