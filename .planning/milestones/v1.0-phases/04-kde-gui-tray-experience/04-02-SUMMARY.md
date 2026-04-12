---
phase: 04-kde-gui-tray-experience
plan: 02
subsystem: ui
tags: [qt6, qml, kirigami, c++, dbus, draft-model, pid-controls, auto-tune, lifecycle-events]

# Dependency graph
requires:
  - phase: 04-kde-gui-tray-experience
    plan: 01
    provides: C++ DBus proxy classes, FanListModel, SensorListModel, Overview page, reusable QML components, DaemonInterface async call pattern
  - phase: 03-temperature-control-runtime-operations
    provides: daemon DBus Control and Lifecycle interfaces, auto-tune methods, draft/apply contract
  - phase: 02-safe-enrollment-lifecycle-recovery
    provides: draft config types, validation/apply DBus methods, lifecycle event model, degraded-state tracking
provides:
  - C++ DraftModel with full draft/validate/apply/discard cycle and auto-tune proposal tracking
  - C++ LifecycleEventModel for lifecycle event history display
  - QML FanDetailPage with core controls, draft editing, auto-tune flow, and advanced tabs
  - QML PidField reusable component with hover help text
affects: [04-kde-gui-tray-experience, system-tray, fan-detail-wizard]

# Tech tracking
tech-stack:
  added: []
  patterns: [DraftModel reactive state pattern (caches draft/applied JSON, merges for display, flushes local edits through DBus writes), QML-form SpinBox for temp display in °C with millidegree internal storage, multi-select sensor source via Repeater with CheckBox, TabBar+StackLayout pattern for advanced detail content]

key-files:
  created:
    - gui/src/models/draft_model.h
    - gui/src/models/draft_model.cpp
    - gui/src/models/lifecycle_event_model.h
    - gui/src/models/lifecycle_event_model.cpp
    - gui/qml/FanDetailPage.qml
    - gui/qml/components/PidField.qml
  modified:
    - gui/CMakeLists.txt
    - gui/src/main.cpp
    - gui/qml/OverviewPage.qml

key-decisions:
  - "DraftModel uses two-path setters: local property setters (setEnrolled, setControlMode) that update Q_PROPERTY state immediately, and DBus flushers (setEnrolledViaDBus, setControlModeViaDBus) that also send the change to the daemon — QML calls the DBus flushers to ensure draft state is persisted"
  - "DraftModel caches both draft and applied JSON responses and merges them: draft entries take priority over applied entries, falling back to applied config values when no draft entry exists for the selected fan"
  - "DraftModel auto-tune proposal state is local: when AutoTuneCompleted signal arrives for the current fan, the model fetches the result via DaemonInterface.autoTuneResult(), populates proposedKp/Ki/Kd, and sets autoTuneProposalAvailable — the user must explicitly accept which stages gains into draft only"
  - "FanDetailPage uses Kirigami.ScrollablePage with FormLayout for core controls and TabBar+StackLayout for advanced tabs per D-16"
  - "LifecycleEventModel parses daemon lifecycle event JSON including structured DegradedReason objects, extracting kind and fan_id fields for human-readable display"
  - "Sensor multi-select uses Repeater+CheckBox over SensorListModel instead of a multi-select ComboBox, matching the UI-SPEC contract"

patterns-established:
  - "Draft editing pattern: QML calls DraftModel.xxxViaDBus() which updates local Q_PROPERTY and sends JSON to daemon — local form state stays in sync with daemon draft state"
  - "Auto-tune flow: startAutoTune() → autoTuneRunning=true → AutoTuneCompleted signal → autoTuneResult() fetch → autoTuneProposalAvailable=true → user reviews → acceptAutoTuneProposal() stages into draft → user must still Apply"
  - "Tab content pattern: Runtime/Advanced/Events tabs use Controls.TabBar + Controls.StackLayout with currentIndex binding per D-16"
  - "Per-field validation: validation and apply errors are stored as QStringList on DraftModel and displayed via Kirigami.InlineMessage with per-field Repeater"

requirements-completed: [GUI-02, GUI-03, GUI-05]

# Metrics
duration: 25min
completed: 2026-04-11
---

# Phase 04 Plan 02: Fan Detail Page Summary

**Draft editing model, lifecycle event model, and fan detail page with core controls, auto-tune flow, and advanced tabs**

## Performance

- **Duration:** 25 min
- **Started:** 2026-04-11T23:05:41Z
- **Completed:** 2026-04-11T23:31:00Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- C++ DraftModel with full draft/validate/apply/discard cycle connecting to DaemonInterface DBus proxy
- C++ LifecycleEventModel parsing daemon lifecycle event JSON with structured DegradedReason display
- QML FanDetailPage with header block (state badge, temperature, RPM, output, high-temp alert), core controls (enrollment toggle, control mode, sensor multi-select, aggregation, target temp, PID gains, auto-tune), draft action buttons (Validate/Apply/Discard), auto-tune proposal banner, and advanced tabs (Runtime/Advanced/Events)
- QML PidField reusable SpinBox component with hover ToolTip help text per D-15
- Overview page updated to push FanDetailPage when fan row is selected

## Task Commits

Each task was committed atomically:

1. **Task 1: Draft editing model, lifecycle event model, and auto-tune state tracker** - `55eaaec` (feat)
2. **Task 2: Fan Detail page with core controls, draft editing, auto-tune, and advanced tabs** - `400d908` (feat)

## Files Created/Modified
- `gui/src/models/draft_model.h` - DraftModel header with Q_PROPERTY fields for draft editing and auto-tune state
- `gui/src/models/draft_model.cpp` - DraftModel implementation with JSON parsing, DBus write flushers, and signal handlers
- `gui/src/models/lifecycle_event_model.h` - LifecycleEventModel QAbstractListModel subclass header
- `gui/src/models/lifecycle_event_model.cpp` - LifecycleEventModel implementation parsing daemon event JSON
- `gui/qml/FanDetailPage.qml` - Per-fan detail page with core controls, draft editing, auto-tune proposal, and advanced tabs
- `gui/qml/components/PidField.qml` - Reusable PID gain input SpinBox with hover help text
- `gui/CMakeLists.txt` - Added new source files and QML files to build
- `gui/src/main.cpp` - Registered DraftModel and LifecycleEventModel as context properties
- `gui/qml/OverviewPage.qml` - Added FanDetailPage navigation on fan row click

## Decisions Made
- DraftModel uses separate ViaDBus setter methods that both update local Q_PROPERTY state and send JSON to the daemon, ensuring local form state and daemon draft state stay synchronized
- LifecycleEventModel parses the daemon's structured DegradedReason JSON, extracting kind and fan_id for human-readable display instead of raw JSON
- Auto-tune proposal gains are staged into draft only on accept — user must still click "Apply changes" per D-19
- Sensor multi-select uses Repeater+CheckBox over SensorListModel rows matching the UI-SPEC contract for multi-sensor selection
- Aggregation dropdown is hidden when only one sensor is selected, showing "Single sensor" read-only text per UI-SPEC

## Deviations from Plan

None - plan executed exactly as written.

## Known Stubs
- FanDetailPage advanced tab values (cadence intervals, deadband, actuator policy min/max) use hardcoded SpinBox defaults — these will be wired to DraftModel DBus writes in a future enhancement once setDraftFanControlProfile supports these fields individually
- FanDetailPage lifecycle event refresh is triggered on page load via Component.onCompleted calling daemonInterface.lifecycleEvents() — lifecycle events are not yet auto-refreshed on daemon lifecycle signal changes (would need StatusMonitor integration)

## Self-Check: PASSED

All 6 created files verified present. Both task commits (55eaaec, 400d908) verified in git log. Build exits with BUILD_EXIT=0.

---
*Phase: 04-kde-gui-tray-experience*
*Completed: 2026-04-11*