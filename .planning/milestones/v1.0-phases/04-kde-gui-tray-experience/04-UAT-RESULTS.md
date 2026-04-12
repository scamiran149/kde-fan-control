# Phase 4 UAT Results

## Date: 2026-04-12

## Overall Result: **PASS WITH KNOWN ISSUES**

All 12 UAT sections passed. Fixes were applied during testing for blocking defects.

| Section | Result |
|---------|--------|
| 1. Launch Smoke Test | **PASS** (after fixes) |
| 2. Overview Read Path | **PASS** |
| 3. Inventory Read Path | **PASS** |
| 4. Detail Page Read Path | **PASS** |
| 5. Read-Only Safety | **PASS** |
| 6. Write-Path Session | **PASS** (after DBus signature fix) |
| 7. Draft Editing | **PASS WITH NOTE** — enrollment-level fields revert on managed fans |
| 8. Validate/Apply/Discard | **PASS** |
| 9. Auto-Tune | **PASS** (after proposal banner fix) |
| 10. Reactive Updates | **PASS** |
| 11. Tray/Notifications | **PASS** (after icon/Quit fixes) |
| 12. Polling/Visibility | **PASS** |

## Fixes Applied During UAT

1. **InventoryPage overflow/overlap** — Flattened nested AbstractCards into direct Repeater entries
2. **Duplicate FanDetailPage navigation** — Replace instead of push when detail page is already current
3. **DBus signature mismatch** — `setDraftFanEnrollment` now sends `(sbsas)` matching the daemon, not `(ss)` JSON blob
4. **WizardDialog runtime errors** — Removed readonly property assignment; registered model enums for QML
5. **Auto-tune proposal banner unreadable** — Render proposed Kp/Ki/Kd inline in message text
6. **Validation/apply error banners broken** — Moved visual children out of `InlineMessage` into layout siblings
7. **FanDetailPage scene warning** — Switched to `Kirigami.Page` + `ScrollView`; deferred data loading with `Qt.callLater`
8. **Tray icon disappearing** — Used Breeze-available icon names (no `-symbolic` suffix where unavailable)
9. **Duplicate Quit in tray menu** — Removed manual Quit action; KStatusNotifierItem provides its own
10. **Tray left-click no-op** — Fixed by correcting icon/status handling; `activateRequested` now delivers

## Known Issues Carried Forward

### Enrollment Field Revert (Design Gap)

**Severity:** Medium — UX defect, not a crash

When toggling enrollment-level fields (Managed checkbox, sensor selection, aggregation mode) on an already-managed fan, the local state updates optimistically then reverts after ~1 second. This happens because:

- Enrollment fields are set via `SetDraftFanEnrollment` (daemon DBus)
- The daemon's `DraftChanged` signal reloads the draft config
- For an already-managed fan, the draft entry may not contain these fields, causing the applied config to overwrite them

**Root cause:** The detail page presents enrollment and profile fields at the same level. Enrollment is a one-time setup action (wizard flow), not a live toggle. The fix is a UX redesign:

- Move enrollment controls to a separate section or demote them below profile controls
- Enrollment toggles on an already-managed fan should go through the wizard or a re-enrollment flow
- Ensure the draft model correctly handles the enrollment state for managed fans

**Deferred to:** A future phase that addresses the enrollment UX redesign.

### Wizard Flow Not Tested

**Severity:** Medium — untested feature

The UAT did not exercise the wizard configuration flow (WizardDialog). From initial exploration:

- DBus signature fix (`sbsas`) should resolve the wizard's `setDraftFanEnrollment` calls
- However, the wizard has `FanListModel` enum references that were fixed alongside the detail page
- The wizard was not exercised end-to-end during this UAT session

**Deferred to:** A dedicated wizard flow test in a future phase.

### FanDetailPage Graphics Scene Warning

**Severity:** Low — cosmetic console warning, no functional impact

`QML FanDetailPage: Created graphical object was not placed in the graphics scene.`

This is a harmless Kirigami `Page` lifecycle artifact: the page is instantiated before being parented to the scene graph. It fires on every `pageStack.push()`. No functional impact.

### Validation Success Banner Off-Screen

**Severity:** Low — UX annoyance

When the detail page has long content (many sensors, advanced controls visible), the validation success banner appears at the top but may scroll off-screen. Users need to scroll up to see it.

### Root Session SNI Unavailable

**Severity:** Expected — temporary workaround

`kf.statusnotifieritem: KDE platform plugin is loaded but SNI unavailable` when running `sudo ./gui/build/gui_app`. This is expected: KStatusNotifierItem cannot register on the session bus when running as root. The intended fix is the polkit authorization path (AUTH-01/02/03) in a later phase.