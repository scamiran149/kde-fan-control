---
phase: 04-kde-gui-tray-experience
reviewed: 2026-04-11T19:50:00Z
depth: standard
files_reviewed: 23
files_reviewed_list:
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
  - gui/src/models/draft_model.h
  - gui/src/models/draft_model.cpp
  - gui/src/models/lifecycle_event_model.h
  - gui/src/models/lifecycle_event_model.cpp
  - gui/src/tray_icon.h
  - gui/src/tray_icon.cpp
  - gui/src/notification_handler.h
  - gui/src/notification_handler.cpp
  - gui/qml/Main.qml
  - gui/qml/OverviewPage.qml
  - gui/qml/InventoryPage.qml
  - gui/qml/FanDetailPage.qml
  - gui/qml/TrayPopover.qml
  - gui/qml/WizardDialog.qml
  - gui/qml/delegates/FanRowDelegate.qml
  - gui/qml/delegates/FanTrayDelegate.qml
  - gui/qml/components/StateBadge.qml
  - gui/qml/components/OutputBar.qml
  - gui/qml/components/TemperatureDisplay.qml
  - gui/qml/components/PidField.qml
findings:
  critical: 2
  warning: 8
  info: 10
  total: 20
status: issues_found
---

# Phase 4: Code Review Report

**Reviewed:** 2026-04-11T19:50:00Z
**Depth:** standard
**Files Reviewed:** 23
**Status:** issues_found

## Summary

Reviewed the KDE/Qt6/QML GUI implementation for Phase 4 (kde-gui-tray-experience). The codebase includes a C++ DBus bridge to the Rust daemon, QML overview/inventory/detail pages, wizard configuration dialog, system tray integration, and notification handling.

The architecture is sound: the GUI correctly uses DBus as its sole IPC mechanism, never touches sysfs directly, and adheres to the project's privilege boundary constraints. The draft/apply contract is properly implemented through DraftModel.

However, there are **2 critical issues** involving DBus signal handling and KF6 library linking, **8 warnings** covering race conditions in data flow, resource management, and input validation, and **10 informational items** for code quality improvements.

---

## Critical Issues

### CR-01: Missing DBus NameOwnerChanged Handler — Daemon Reconnection Broken

**File:** `gui/src/daemon_interface.cpp:39-45`
**Issue:** The constructor connects to `NameOwnerChanged` on the system bus with `SLOT(handleNameOwnerChanged(...))`, but `handleNameOwnerChanged` is **never declared or defined** in the `DaemonInterface` class. The `SLOT()` macro with a string literal won't cause a compile error — it silently fails at runtime. This means when the daemon crashes and restarts, or when the daemon registers after the GUI starts, the `m_connected` property will never update via name owner tracking. The GUI will remain showing "disconnected" permanently after any daemon restart unless the user manually triggers `checkDaemonConnected()`.

Additionally, line 23-28 connects a generic signal on `s_inventoryIface` with an empty signal name (`QString()`), which won't match any DBus signal — this connection silently does nothing.

**Fix:**
```cpp
// In daemon_interface.h, add:
private slots:
    void handleNameOwnerChanged(const QString &name, const QString &oldOwner, const QString &newOwner);

// In daemon_interface.cpp, add:
void DaemonInterface::handleNameOwnerChanged(const QString &name, const QString &oldOwner, const QString &newOwner)
{
    if (name == QLatin1String(s_service)) {
        setConnected(!newOwner.isEmpty());
    }
}

// Remove the broken empty-signal connection (lines 23-28) since NameOwnerChanged
// already handles both connect and disconnect scenarios.
```

### CR-02: Hardcoded Library Paths Create Brittle/Non-Portable Build

**File:** `gui/CMakeLists.txt:94-99`
**Issue:** The build links KF6 libraries by absolute filesystem paths to `.so.6` versioned shared objects:
```cmake
set(KF6_SNI_LIB "/usr/lib/x86_64-linux-gnu/libKF6StatusNotifierItem.so.6")
set(KF6_NOTIF_LIB "/usr/lib/x86_64-linux-gnu/libKF6Notifications.so.6")
```

This is fragile and non-portable: it hardcodes architecture (`x86_64`), library version (`.6`), and the absolute path. It will fail on different architectures, different distros, or when KF6 version changes. It also links against the runtime `.so` rather than the development symlink (`.so`), which may not work with all linkers. The comment says "KF6 runtime shared libraries are available but dev packages are not installed" — this should be solved by installing the dev packages, not by linking runtime versioned libraries directly.

**Fix:**
```cmake
# Install KF6 dev packages, then use proper CMake targets:
find_package(KF6StatusNotifierItem REQUIRED)
find_package(KF6Notifications REQUIRED)

target_link_libraries(gui_app PRIVATE
    Qt6::Core Qt6::Quick Qt6::Qml Qt6::DBus Qt6::Widgets
    KF6::StatusNotifierItem
    KF6::Notifications
)
```
If dev packages truly cannot be installed, use `find_library()` with appropriate hints instead of hardcoding absolute paths, and at minimum link against unversioned `.so` symlinks.

---

## Warnings

### WR-01: FanListModel Refresh Has Race-Window When Multiple Async Results Arrive

**File:** `gui/src/status_monitor.cpp:81-110`
**Issue:** `onSnapshotResult()`, `onRuntimeStateResult()`, and `onDraftConfigResult()` each update cached JSON and then conditionally call `m_fanModel->refresh()`. However, when `refreshAll()` fires all three requests simultaneously, the three async responses may arrive in any order. If snapshot arrives first but runtime hasn't arrived yet, `onSnapshotResult` refreshes the sensor model but skips fan model refresh (since `m_cachedRuntimeState` is empty). If runtime arrives second, `onRuntimeResult` then calls `m_fanModel->refresh()` with the fresh runtime but potentially stale draft config. This creates a transient state where the fan list shows data from mixed time points. While not a crash, it can produce flickering or inconsistent display.

**Fix:** Consider batching: track arrival of all three responses and only refresh the fan model once all three are available. Alternatively, add a `refreshIfComplete()` check after each cache update that only triggers when all three caches are non-empty.

### WR-02: DraftModel setPidGains Always Sends DBus Call Even When Values Haven't Changed

**File:** `gui/src/models/draft_model.cpp:163-180`
**Issue:** `setPidGains()` checks for changed values with `qFuzzyCompare` for the purpose of emitting `pidGainsChanged()`, but then **unconditionally** sends the DBus call `m_daemon->setDraftFanControlProfile()` even if no values changed. This means every keystroke in the PID field spinboxes triggers a DBus round-trip to the privileged daemon. For a system service managing hardware, this creates unnecessary DBus traffic and polkit authorization prompts on every character change.

**Fix:**
```cpp
void DraftModel::setPidGains(double kp, double ki, double kd)
{
    bool changed = !qFuzzyCompare(m_kp, kp) || !qFuzzyCompare(m_ki, ki) || !qFuzzyCompare(m_kd, kd);
    if (changed) {
        m_kp = kp;
        m_ki = ki;
        m_kd = kd;
        Q_EMIT pidGainsChanged();
        // Only send DBus call when values actually changed
        QJsonObject gainsObj;
        gainsObj[QStringLiteral("kp")] = kp;
        gainsObj[QStringLiteral("ki")] = ki;
        gainsObj[QStringLiteral("kd")] = kd;
        QJsonObject profileObj;
        profileObj[QStringLiteral("pid_gains")] = gainsObj;
        QJsonDocument doc(profileObj);
        m_daemon->setDraftFanControlProfile(m_fanId, QString::fromUtf8(doc.toJson(QJsonDocument::Compact)));
    }
}
```

### WR-03: QML SpinBox valueFromText Uses parseFloat Without Validation

**File:** `gui/qml/FanDetailPage.qml:373-376`, `gui/qml/WizardDialog.qml:491-494`
**Issue:** The `valueFromText` functions use `parseFloat(text)` on user-editable SpinBox input without guarding against `NaN`. If a user clears the field or types a non-numeric string, `parseFloat` returns `NaN`, and `Math.round(NaN * 10)` = `NaN` which becomes `0` via `Math.round`. This silently converts invalid input to 0°C, which is then sent to the daemon via `setTargetTempCelsiusViaDBus(0)`. A 0°C target temperature for a fan is dangerous (requests maximum fan speed at all times on boot).

**Fix:**
```javascript
valueFromText: function(text) {
    var num = parseFloat(text)
    if (isNaN(num)) return value  // keep current value on invalid input
    return Math.round(num * 10)
}
```

### WR-04: TrayPopover "Open Fan Control" Button Has No Handler

**File:** `gui/qml/TrayPopover.qml:247-253`
**Issue:** The "Open Fan Control" button's `onClicked` handler is empty (only contains a comment). Clicking it does nothing — the user has no way to open the main window from the tray popover.

**Fix:** Connect to a signal that raises/activates the main window. The TrayIcon C++ class should expose a signal that the QML layer can use, or use `Qt.callLater` to call the application window's `show()` and `raise()`:
```qml
onClicked: {
    trayIcon.activateMainWindow()
}
```
And in C++ TrayIcon, add:
```cpp
Q_INVOKABLE void activateMainWindow() { Q_EMIT activateRequested(); }
signals: void activateRequested();
```

### WR-05: FanListModel::refresh Never Clears Model on Empty Input

**File:** `gui/src/models/fan_list_model.cpp:70-90`
**Issue:** When `inventoryJson` is empty, `QJsonDocument::fromJson("", &err)` will produce an empty `QJsonObject` (the `err.error` check skips the early return only for non-empty strings). If all three JSON strings are empty (e.g., daemon just disconnected), `refresh("", "", "")` is called from `StatusMonitor::onDaemonConnectedChanged` (line 76-78). The model then creates fans from an empty `devices` array — which works correctly (yields zero fans). However, the same is not true for `SensorListModel::refresh("")` — it also handles empty correctly. But there's a subtle issue: when the daemon is disconnected, `StatusMonitor::onDaemonConnectedChanged` calls `m_fanModel->refresh(QString(), QString(), QString())` — but this sends three null strings through JSON parsing (not empty strings), which produces different behavior than empty strings. `QJsonDocument::fromJson(QByteArray())` returns a null document, and `.object()` on a null document returns an empty object, so the model correctly clears. This is fine but fragile.

**Fix:** Consider explicitly passing empty strings rather than null strings for clarity:
```cpp
m_fanModel->refresh(QString(""), QString(""), QString(""));
```

### WR-06: KNotification Memory Leak — Notifications Never Deleted

**File:** `gui/src/notification_handler.cpp:124-160`
**Issue:** `KNotification::event()` returns a `KNotification*` pointer. In all three notification paths (fallback, degraded, high-temp), the returned pointer is stored in a local variable `n`, `sendEvent()` is called, and then the pointer is dropped. While `KNotification` has a `CloseOnTimeout` flag set and will be deleted when closed, if multiple notifications fire in rapid succession (e.g., multiple fans transitioning to fallback simultaneously), each creates a new `KNotification` object. If the user doesn't dismiss them, they accumulate. The `CloseOnTimeout` flag helps, but during rapid state transitions, multiple notifications could stack without explicit cleanup.

**Fix:** Connect the notification's `closed()` signal to `deleteLater()` for guaranteed cleanup, or use `KNotification::event()` without storing the pointer (it self-deletes via `CloseOnTimeout` after emitting). Alternatively, coalesce multiple transitions into a single notification to reduce spam.

### WR-07: WizardDialog Step Navigation Doesn't Flush All Data Changes

**File:** `gui/qml/WizardDialog.qml:763-776`
**Issue:** The `onClicked` handler for "Next" button only checks `stepFan` and `stepSensor` for data flushes. When transitioning from `stepTargetTemp` (step 4) to `stepPid` (step 5), the target temperature may not have been flushed to the daemon yet (the SpinBox `onValueModified` handler in WizardDialog only fires on user interaction, but the default 65.0°C value was set via `selectedTargetTempCelsius` property without calling `setTargetTempCelsiusViaDBus`). If the user navigates quickly through the wizard without changing the default target temp, the daemon never receives the target temperature, leaving it at 0 millidegrees.

**Fix:** Call `draftModel.setTargetTempCelsiusViaDBus(selectedTargetTempCelsius)` when entering the review step (step 6), or when transitioning from step 4 to step 5, to ensure the daemon's draft always matches what the wizard displays.

### WR-08: DraftModel JSON Parsing Error on Second Call Reuses Stale Error Variable

**File:** `gui/src/models/fan_list_model.cpp:75-90` and similar patterns in `sensor_list_model.cpp:59-63` and `draft_model.cpp:314-318`
**Issue:** The pattern `QJsonParseError err; QJsonObject x = QJsonDocument::fromJson(json.toUtf8(), &err).object();` parses JSON and checks `err.error`. However, when multiple JSON strings are parsed sequentially (lines 75-89 in `fan_list_model.cpp`), the `err` variable is reused. If the first parse succeeds and the second fails, the check on line 82 (`err.error != QJsonParseError::NoError`) correctly catches it. But if the first parse fails and the code returns early, the second and third `err` checks are skipped — which is correct. However, if any empty JSON string is passed, `fromJson("")` sets `err.error = NoError` (since an empty document is valid JSON per RFC 8259 §4), but `.object()` returns an empty object. This is the intended behavior for "no data" but is not explicitly documented in the code, making maintenance risky.

**Fix:** Add a comment explaining the empty-string semantics:
```cpp
// Empty JSON strings produce valid empty objects — this is intentional
// and represents "no data available" (e.g., daemon disconnected).
```

---

## Info

### IN-01: QML Property Type Mismatch for temperatureMillidegrees

**File:** `gui/qml/OverviewPage.qml:22-28`, `gui/qml/FanDetailPage.qml:25`
**Issue:** `FanListModel::temperatureMillidegrees` is declared as `qint64` in C++ (`Q_PROPERTY(qint64 temperatureMillidegrees ...)`), but QML `property int temperatureMillidegrees` in the delegate/page may lose precision for values exceeding JavaScript's safe integer range (~2^53). While fan temperatures in millidegrees won't realistically exceed this, the type should be documented as JavaScript `number` (which is 64-bit double) or the C++ type should be explicitly registered. Similarly, `FanTrayDelegate.qml:28` uses `property int temperatureMillidegrees: 0`.

**Fix:** In QML property declarations, use `property real` or `property var` for qint64 values rather than `property int`. Or document that the values are always < 150000 (150°C in millidegrees) and fit in int.

### IN-02: Unused KF5 Include Paths in CMakeLists.txt

**File:** `gui/CMakeLists.txt:72-73`
**Issue:** The include directories include `/usr/include/KF5` and `/usr/include/KF5/KNotifications`, but the project uses KF6. These KF5 paths are likely remnants of an earlier build configuration and are unnecessary for KF6 headers.

**Fix:** Remove these hardcoded KF5 include paths:
```cmake
# Remove:
/usr/include/KF5
/usr/include/KF5/KNotifications
```

### IN-03: QML Components Use Context Properties Instead of Registered Types

**File:** `gui/qml/Main.qml:52-59`
**Issue:** All models and interfaces are injected as QML context properties (`setContextProperty`). This works but doesn't provide any type safety or auto-completion in tooling. For a project this size, it's acceptable, but registering types with `qmlRegisterType` would be more maintainable.

**Fix:** Consider registering C++ types with `qmlRegisterType` / `qmlRegisterUncreatableType` for better tooling support and type safety. Low priority.

### IN-04: WizardDialog applySucceeded Logic Race

**File:** `gui/qml/WizardDialog.qml:809-819`
**Issue:** The `onApplyStateChanged` connection assumes that `hasApplyError` being false means apply succeeded, but this also triggers on `discardDraft()` which clears `hasApplyError`. The closeTimer could fire after an unintended state clear. The `applySucceeded` flag is properly set only in the success branch, but the general logic could be tighter.

**Fix:** Add an explicit check: only set `applySucceeded = true` and start the timer when `draftModel.hasApplyError === false && draftModel.applyErrors.length === 0 && applyWasAttempted` (with a local flag tracking whether an apply was attempted).

### IN-05: TemperatureDisplay Shows "No control source" for Zero-Degree Readings

**File:** `gui/qml/components/TemperatureDisplay.qml:22`
**Issue:** The condition `millidegrees <= 0` means that if a sensor reports exactly 0°C (which is a valid temperature, though rare), it displays "No control source" instead of "0.0 °C". For fan control, 0°C isn't practical, but the logic conflates "no reading" with "zero reading".

**Fix:** Use a sentinel value approach. In the C++ model, use `-1` or `INT_MIN` as "no data" rather than `0`, then in QML check `millidegrees < 0` instead of `millidegrees <= 0`.

### IN-06: LifecycleEventModel Doesn't Handle Null JSON Gracefully

**File:** `gui/src/models/lifecycle_event_model.cpp:59-63`
**Issue:** When the daemon returns an empty string for lifecycle events, `QJsonDocument::fromJson("")` produces a valid but empty document, and `doc.array()` returns an empty `QJsonArray`. This is correct behavior but means the model silently clears all events on any empty response, including transient errors. If the daemon briefly returns empty on a timeout, all event history disappears.

**Fix:** Consider preserving existing events on empty/null responses, or differentiate between "no events yet" (empty array `[]`) and "request failed" (empty string).

### IN-07: FanRowDelegate Mouse Area Overlaps Card Content Without Click Feedback

**File:** `gui/qml/delegates/FanRowDelegate.qml:109-112`
**Issue:** The `MouseArea` with `onClicked: fanRow.clicked()` covers the entire card, but there's no visual feedback (press/hover state) for the click. The `Kirigami.AbstractCard` already provides highlighting; the `MouseArea` on top may interfere with card interaction.

**Fix:** Move the `clicked` signal handling to the card's built-in click handler if Kirigami supports it, or add visual feedback (opacity change) to the MouseArea.

### IN-08: outputPercent Property Type Inconsistency Between C++ and QML

**File:** `gui/qml/delegates/FanTrayDelegate.qml:28`, `gui/qml/FanDetailPage.qml:27`, `gui/src/types.h:26`
**Issue:** `FanStateInfo::outputPercent` is declared as `double` in C++, but `FanTrayDelegate` declares `property double outputPercent: -1.0` while `FanDetailPage` declares `property double fanOutputPercent: -1.0`. However, `FanListModel::data()` returns `fan->outputPercent()` as a `QVariant(double)`, and `OutputBar.qml` uses `property double percent: 0.0`. The `OutputBar` default of `0.0` differs from the "no control" sentinel `-1.0` used elsewhere. The `formatOutputPercent` in `types.cpp` treats `< 0` as "No control", but `OutputBar` doesn't check for negative values — it just shows the absolute value.

**Fix:** In `OutputBar.qml`, handle `percent < 0` as a special "no control" state:
```qml
text: outputBar.percent < 0 ? i18n("No control") : Math.round(outputBar.percent) + "%"
```

### IN-09: PidField SpinBox Precision Loss in Decimal Scaling

**File:** `gui/qml/components/PidField.qml:35-38`
**Issue:** The SpinBox uses integer arithmetic internally but represents decimal values (with `decimals` = 2 by default). The conversion uses `Math.pow(10, pidField.decimals)` for scaling. At 2 decimal places, the integer range is `from: 0` to `to: 10000`, representing 0.00 to 100.00. This works but is susceptible to floating-point accumulation errors if values are repeatedly set via the spin buttons.

**Fix:** The current approach is standard for Qt Quick SpinBox with decimals. No immediate fix needed, but document the scaling convention for future maintainers.

### IN-10: No User-Facing Error on DBus Connection Failure at Startup

**File:** `gui/src/main.cpp:46-47`
**Issue:** The `checkDaemonConnected` call is queued but there's no visible notification if the daemon isn't running at startup. The UI shows "Disconnected" in the tray and banners, but there's no explicit "First run" guidance or notification that tells the user to start the system service. This is a UX concern rather than a bug.

**Fix:** Consider adding a first-run check in `StatusMonitor::checkDaemonConnected()` that fires a desktop notification via `NotificationHandler` if the daemon is not found on first launch, directing the user to check `systemctl status org.kde.FanControl`.

---

_Reviewed: 2026-04-11T19:50:00Z_
_Reviewer: the agent (gsd-code-reviewer)_
_Depth: standard_