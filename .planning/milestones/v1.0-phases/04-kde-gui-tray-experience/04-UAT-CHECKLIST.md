# Phase 4 UAT Checklist

## Purpose

This checklist is the practical UAT entry document for Phase 4.

It exists because the current implementation is functionally close to spec, but the final authorization model is not complete yet:

- Phase 4 GUI behavior is being tested now.
- Final privileged-write UX is **not** final yet.
- `sudo` root GUI usage is a **temporary development/UAT workaround only**.
- The intended shipped behavior remains the v1.1 path in `AUTH-01`, `AUTH-02`, and `AUTH-03`: daemon-side polkit authorization with unprivileged GUI usage.

## Scope

This checklist covers:

- Overview and inventory read paths
- Fan detail navigation and live updates
- Draft edit / validate / apply behavior
- Auto-tune flow
- Tray and notification behavior
- Reactive updates and polling behavior

This checklist does **not** redefine product authorization.

## Current Known Issues

These are known at UAT start and should be logged, but they do not block testing unless they break a scenario below.

1. Root-launched GUI may log `kf.statusnotifieritem: ... SNI unavailable`.
2. `FanDetailPage` may log `Created graphical object was not placed in the graphics scene.`
3. Fan RPM values appear to come from daemon inventory/snapshot and may be stale relative to live sysfs.
4. Final polkit-authenticated write UX is not implemented yet; root GUI is only a temporary test path.

## Session Modes

### Mode A: Normal User Session

Use this for read-path UAT:

```bash
./gui/build/gui_app
```

Expected:

- GUI launches as a normal desktop app
- Runtime values, overview, inventory, and detail pages work
- Write controls are disabled or clearly read-only

### Mode B: Temporary Root Session

Use this only for write-path UAT until polkit is implemented:

```bash
sudo ./gui/build/gui_app
```

Optional debug mode:

```bash
sudo --preserve-env=KFC_GUI_DEBUG KFC_GUI_DEBUG=1 ./gui/build/gui_app
```

Expected:

- Write controls are enabled
- Draft edits, validate, apply, discard, and auto-tune can be exercised
- Tray/session integration may be degraded because the GUI is running as root

## Preconditions

Before starting either session:

1. Build the GUI:

```bash
cmake --build gui/build
```

2. Ensure the daemon is running:

```bash
systemctl status kde-fan-control
```

3. Confirm the CLI can see live state:

```bash
./target/debug/kde-fan-control-cli state
./target/debug/kde-fan-control-cli inventory
```

4. Have at least:

- one managed fan
- one unmanaged fan
- one readable temperature source

## Pass Criteria

Phase 4 UAT is considered successful when all of the following are true:

1. Overview shows correct managed/unmanaged states and live temperature/output.
2. Inventory page is populated and usable.
3. Fan detail pages open and close cleanly.
4. Managed-fan live values update reactively in the GUI.
5. Draft editing works in the temporary root session.
6. Validate/apply/discard work in the temporary root session.
7. Auto-tune flow works if hardware and daemon state allow it.
8. Normal-user session remains safely read-only.
9. Any remaining warnings are non-blocking and documented.

## Checklist

### 1. Launch Smoke Test

Run a normal-user session first:

```bash
./gui/build/gui_app
```

Verify:

- [ ] App window opens
- [ ] Overview page renders
- [ ] No tray panel is embedded in the main window
- [ ] Inventory navigation works
- [ ] Clicking a fan opens the detail page
- [ ] Back navigation returns to the overview

Record any console warnings.

### 2. Overview Read Path

Compare GUI against CLI `state`:

```bash
./target/debug/kde-fan-control-cli state
```

Verify:

- [ ] Managed fans in GUI match CLI managed fans
- [ ] Unmanaged fans in GUI match CLI unmanaged fans
- [ ] Managed fan temperature matches CLI closely
- [ ] Managed fan output percentage matches CLI closely
- [ ] High-temp status, if present, is surfaced in the GUI

Note:

- RPM may differ from live sysfs because the daemon snapshot path currently appears stale.

### 3. Inventory Read Path

Compare GUI against CLI `inventory`:

```bash
./target/debug/kde-fan-control-cli inventory
```

Verify:

- [ ] Inventory page lists sensors
- [ ] Inventory page lists fans
- [ ] Support state is shown for each fan
- [ ] Control mode is shown where available
- [ ] Friendly names appear if configured

### 4. Detail Page Read Path

Open one managed fan and one unmanaged fan.

Verify:

- [ ] Managed fan detail page opens without blocking errors
- [ ] Unmanaged fan detail page opens without blocking errors
- [ ] Managed fan temperature updates over time
- [ ] Managed fan output percentage updates over time
- [ ] Detail page can be exited via Back

### 5. Read-Only Safety Check

In a normal-user session, verify the GUI behaves as read-only.

Verify:

- [ ] Write controls are disabled or clearly non-editable
- [ ] GUI explains read-only state
- [ ] No repeated `AccessDenied` spam appears from casual interaction

### 6. Write-Path Session

Close the normal-user GUI and open the temporary root session:

```bash
sudo ./gui/build/gui_app
```

Verify:

- [ ] Write controls are enabled
- [ ] Fan detail settings can be changed
- [ ] No DBus signature mismatch errors occur

### 7. Draft Editing

Use an unmanaged supported fan.

Verify:

- [ ] Enroll toggle works
- [ ] Control mode can be changed
- [ ] Temperature source selection works
- [ ] Aggregation can be changed when multiple sources are selected
- [ ] Target temperature can be changed
- [ ] PID values can be changed
- [ ] Advanced cadence/output fields can be changed

### 8. Validate / Apply / Discard

Verify:

- [ ] Validate shows success or a useful error
- [ ] Apply succeeds for a valid draft
- [ ] Fan state reflects the applied result in overview and detail
- [ ] Discard removes staged changes without breaking the applied config

### 9. Auto-Tune

Use a managed fan with a valid source configuration.

Verify:

- [ ] Start Auto-Tune can be triggered
- [ ] Running state is visible
- [ ] Completion produces a proposal or result state
- [ ] Accept applies proposed gains to the draft
- [ ] Dismiss leaves the system in a sane state

### 10. Reactive Updates

Keep the GUI open and compare against CLI changes.

Examples:

```bash
./target/debug/kde-fan-control-cli state
./target/debug/kde-fan-control-cli inventory
```

Verify:

- [ ] Overview updates when daemon state changes
- [ ] Detail page updates when daemon state changes
- [ ] Output percentage updates over time on managed fans
- [ ] Temperature updates over time on managed fans
- [ ] Friendly-name changes propagate after refresh/signal flow

### 11. Tray / Notifications

Treat this as conditional in the temporary root session because tray integration may be degraded there.

Verify where possible:

- [ ] Tray icon appears in normal-user session
- [ ] Severity icon reflects overall state
- [ ] Main window can be reopened from tray
- [ ] Notifications appear on important state transitions only

If root-session tray behavior is degraded, mark it as expected temporary behavior, not a Phase 4 product pass/fail signal.

### 12. Polling / Visibility

Verify:

- [ ] Live values update while the main window is visible
- [ ] Minimizing or hiding the window pauses polling
- [ ] Restoring the window resumes polling

## Logging Template

For each scenario, record:

- Result: `PASS`, `FAIL`, or `PASS WITH KNOWN ISSUE`
- Session mode: `normal-user` or `temporary-root`
- Fan used: `<fan-id>`
- CLI comparison used: `state`, `inventory`, or both
- Notes: short factual summary

Example:

```text
Scenario: 4. Detail Page Read Path
Result: PASS WITH KNOWN ISSUE
Session: normal-user
Fan: hwmon-it8686-9e4661f298d91b68-fan1
CLI comparison: state
Notes: Temperature and output updated correctly; console still logs FanDetailPage graphics-scene warning.
```

## Out-of-Spec Temporary Workaround

For this UAT only:

- root GUI sessions are allowed as a temporary test path

This must **not** be interpreted as final product acceptance for authorization UX.

Final acceptance still requires:

- `GUI-02`
- `AUTH-01`
- `AUTH-02`
- `AUTH-03`

Those belong to the later polkit/system-integration work in Phases 5 and 6.
