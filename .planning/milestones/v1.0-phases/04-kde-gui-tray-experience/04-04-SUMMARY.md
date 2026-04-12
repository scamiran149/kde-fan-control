---
phase: 04-kde-gui-tray-experience
plan: 04
subsystem: ui
tags: [qt6, qml, kirigami, wizard, draft-model, fan-enrollment]

# Dependency graph
requires:
  - phase: 04-kde-gui-tray-experience
    plan: 02
    provides: C++ DraftModel with draft/validate/apply/discard cycle, FanListModel, SensorListModel, FanDetailPage
provides:
  - QML WizardDialog with 7-step guided fan enrollment wizard
  - Integration of wizard entry points into Overview empty CTA, toolbar, and Fan Detail unmanaged action
affects: [04-kde-gui-tray-experience, main-window]

# Tech tracking
tech-stack:
  added: []
  patterns: [Kirigami.Dialog multi-step wizard with StackLayout, preselected fan ID for contextual wizard entry, conditional step skipping for single-sensor aggregation]

key-files:
  created:
    - gui/qml/WizardDialog.qml
  modified:
    - gui/CMakeLists.txt
    - gui/qml/Main.qml
    - gui/qml/OverviewPage.qml
    - gui/qml/FanDetailPage.qml

key-decisions:
  - "Wizard uses Kirigami.Dialog with Controls.StackLayout for step navigation rather than Kirigami.PageStack to keep the wizard modal and self-contained"
  - "Aggregation step (step 4) is conditionally shown only when 2+ sensors are selected; single-sensor selection skips directly to target temperature"
  - "Wizard pre-selects fan ID when opened from Fan Detail unmanaged entry point, skipping step 1 and loading the fan into DraftModel immediately"
  - "Cancellation at any step discards the draft via draftModel.discardDraft() to prevent orphaned draft state"
  - "Apply success auto-closes the wizard after a 2-second display window so users see the confirmation"

patterns-established:
  - "Contract-first wizard pattern: every wizard data mutation goes through DraftModel DBus methods, mirroring direct editing flow"
  - "Conditional step navigation: goNext/goBack skip hidden steps based on data state (sensor count determines aggregation visibility)"

requirements-completed: [GUI-02]

# Metrics
duration: 23min
completed: 2026-04-11
---

# Phase 04 Plan 04: Wizard Configuration Dialog Summary

**Guided wizard dialog for fan enrollment using the same draft/apply contract, with conditional aggregation step and review-then-apply flow**

## Performance

- **Duration:** 23 min
- **Started:** 2026-04-12T00:17:07Z
- **Completed:** 2026-04-12T00:40:35Z
- **Tasks:** 1
- **Files modified:** 5

## Accomplishments
- WizardDialog.qml: Full 7-step guided wizard per UI-SPEC Wizard Contract
- Integration: Empty state CTA, Overview toolbar "Wizard configuration" action, Fan Detail unmanaged fan action
- Step progression works: fan selection → control mode → sensors → aggregation (conditional) → target temp → PID review → validate + apply
- Cancellation discards draft state via draftModel.discardDraft()

## Task Commits

Each task was committed atomically:

1. **Task 1: Guided wizard configuration dialog** - `fa216be` (feat)

## Files Created/Modified
- `gui/qml/WizardDialog.qml` - Kirigami.Dialog-based wizard with 7-step StackLayout, draft/apply contract, review+validate+apply step
- `gui/CMakeLists.txt` - Added WizardDialog.qml to QML_FILES
- `gui/qml/Main.qml` - Added WizardDialog component instance
- `gui/qml/OverviewPage.qml` - Added wizard action to toolbar and empty state CTA button
- `gui/qml/FanDetailPage.qml` - Added wizard action for unmanaged available fans

## Decisions Made
- Kirigami.Dialog used for wizard container (modal, self-contained) rather than page navigation
- Aggregation step is conditionally shown — skipped entirely when only 1 sensor is selected, jumping directly from sensor selection to target temperature
- Preselected fan ID property allows Fan Detail to open wizard with fan already chosen, skipping step 1
- Apply success closes wizard after 2-second display to confirm the change visually before dismissal
- No advanced controls (cadence, deadband, actuator policy, PID limits) shown in wizard per D-14

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed duplicate applySucceeded property in review step**
- **Found during:** Task 1 (WizardDialog implementation review)
- **Issue:** Step 6 ColumnLayout had a local `property bool applySucceeded: false` that shadowed the dialog-level property, which would have caused the apply result banner to never reflect successful state
- **Fix:** Removed the local property, ensuring the review step references `wizardDialog.applySucceeded`
- **Files modified:** gui/qml/WizardDialog.qml
- **Verification:** Build passes, property reference resolves correctly
- **Committed in:** fa216be (amended task commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Trivial fix — no scope creep.

## Known Stubs

None — all wizard data flows through DraftModel DBus methods and the wizard has no hardcoded empty values or placeholder text that flows to rendering.

## Threat Flags

None — wizard uses existing DraftModel/DBus contract with no new network endpoints, auth paths, or file access patterns. Threat model T-04-10 (Wizard validation bypass) is mitigated by step 7 always calling validateDraft before applyDraft.

## Self-Check: PASSED

- `gui/qml/WizardDialog.qml` exists: FOUND
- Commit `fa216be` in git log: FOUND
- Build exits with BUILD_EXIT=0: CONFIRMED

---
*Phase: 04-kde-gui-tray-experience*
*Completed: 2026-04-11*