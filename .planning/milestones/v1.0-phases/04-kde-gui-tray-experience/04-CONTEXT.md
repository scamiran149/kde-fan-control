# Phase 4: KDE GUI & Tray Experience - Context

**Gathered:** 2026-04-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 4 adds a KDE-native Qt6/QML GUI and system tray experience on top of the existing daemon-owned DBus contract. This phase covers how users inspect runtime health, edit staged fan-control configuration, trigger auto-tuning, and receive tray-visible fault or alert feedback without bypassing the daemon or inventing a second authority surface.

</domain>

<decisions>
## Implementation Decisions

### Main Window Structure
- **D-01:** The main GUI should open on an overview-first screen, then drill into a selected fan for deeper configuration.
- **D-02:** The overview should show each fan's state plus live metrics: actual temperature, RPM when present, and current output as a bar.
- **D-03:** The per-fan detail page should use an editable draft pane with explicit `Validate`, `Apply`, and `Discard` actions rather than immediate live writes.
- **D-04:** The GUI should offer an optional `Wizard configuration` path for guided setup, but the default editing flow remains the direct draft pane.

### Status And Severity Surfacing
- **D-05:** The GUI should use strong traffic-light severity cues so managed, unmanaged, degraded, fallback, and unsupported states are distinguishable at a glance.
- **D-06:** Existing high-temperature alert state should be surfaced in the GUI and tray status model.
- **D-07:** Fan overview entries should prioritize state plus live monitoring data rather than dense configuration metadata.

### Tray Experience
- **D-08:** The system tray should be status-first: a compact inspection surface with a quick path into the full window, not a mini control center.
- **D-09:** The tray popover should list managed fans by default rather than every discovered fan.
- **D-10:** Each tray fan entry should stay compact and show state, temperature, and output or RPM.

### Notifications
- **D-11:** Desktop or tray notifications should trigger only for important alert transitions: degraded state, fallback state, and high-temperature alert conditions.
- **D-12:** Important alerts should stay sticky until acknowledged, even if the desktop popup itself is transient.

### Per-Fan Detail Depth
- **D-13:** The per-fan page should show runtime status plus core controls first: source selection, target temperature, control mode, and primary PID values.
- **D-14:** Advanced controls such as cadence, limits, and deeper tuning settings should not be shown up front.
- **D-15:** PID fields should include brief hover explanations so users understand the tuning effect of each value.
- **D-16:** Advanced detail content should be grouped with tabs rather than accordions or one long scrolling page.

### Auto-Tune Flow
- **D-17:** Auto-tuning should start inline from the selected fan's detail page rather than from a separate global surface.
- **D-18:** Auto-tune completion should be surfaced with a proposal banner in the detail page.
- **D-19:** Auto-tune results should still respect the staged draft/apply contract established in earlier phases.

### Already Locked From Prior Context
- **D-20:** The GUI remains a DBus client and must not bypass the daemon as the system authority.
- **D-21:** Configuration changes remain staged and explicitly applied rather than immediately committed live.
- **D-22:** Runtime status should stay simple by default, with deeper PID details available on demand.
- **D-23:** Degraded and fallback states should remain persistently visible and diagnosable.

### the agent's Discretion
- Exact KDE/Kirigami component selection, visual styling, and layout composition, as long as the UI remains KDE-native and preserves the overview-first plus per-fan drill-in structure.
- Exact badge, icon, and color language for traffic-light severity, as long as degraded, fallback, high-temp alert, unmanaged, and unsupported states remain easy to distinguish.
- Exact arrangement of summary cards versus rows in the overview and tray popover, as long as the locked metrics remain visible.
- Exact wording for PID hover help, as long as it is brief and practically useful.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product And Scope
- `.planning/PROJECT.md` — Product constraints, KDE-first UI direction, daemon-owned authority, and safety boundaries.
- `.planning/REQUIREMENTS.md` — Phase 4 requirements `GUI-01` through `GUI-05` and the surrounding v1 scope limits.
- `.planning/ROADMAP.md` — Phase 4 goal, success criteria, dependency on Phase 3, and tray-focused success expectations.
- `.planning/STATE.md` — Current phase progression and active project notes.

### Prior Context That Carries Forward
- `.planning/phases/02-safe-enrollment-lifecycle-recovery/02-CONTEXT.md` — Staged apply model, persistent degraded or fallback visibility, and the earlier decision to leave tray UX for Phase 4.
- `.planning/phases/03-temperature-control-runtime-operations/03-CONTEXT.md` — Temperature-target PID framing, simple-by-default runtime presentation, and per-fan auto-tune flow expectations.

### Existing Backend Surfaces The GUI Must Use
- `crates/core/src/inventory.rs` — Hardware inventory model, support-state fields, sensor and fan labels, and control-mode visibility the GUI must render.
- `crates/core/src/config.rs` — Draft versus applied configuration model, degraded reasons, lifecycle events, and persisted state shape the GUI edits and presents.
- `crates/core/src/lifecycle.rs` — Runtime state model including `managed`, `unmanaged`, `degraded`, and `fallback` fan statuses plus control runtime snapshot fields.
- `crates/daemon/src/main.rs` — DBus interfaces, authorization boundary, and signal surface the GUI and tray must bind to.
- `crates/cli/src/main.rs` — Current concise-versus-detailed status rendering and control flows that establish the baseline operator language.
- `packaging/dbus/org.kde.FanControl.conf` — DBus service policy and local-user access expectations.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/daemon/src/main.rs`: Already exposes `org.kde.FanControl.Inventory`, `org.kde.FanControl.Lifecycle`, and `org.kde.FanControl.Control`, which are the natural GUI integration points.
- `crates/daemon/src/main.rs`: Already emits `draft_changed`, `degraded_state_changed`, `control_status_changed`, and `AutoTuneCompleted` signals that can drive reactive GUI and tray updates.
- `crates/core/src/lifecycle.rs`: Already provides `RuntimeState`, `FanRuntimeStatus`, and `ControlRuntimeSnapshot`, which map directly to overview cards, tray summaries, and fan-detail runtime panels.
- `crates/core/src/config.rs`: Already provides draft and applied configuration types plus lifecycle event and degraded-reason models the GUI can present without inventing a second schema.
- `crates/cli/src/main.rs`: Already demonstrates a simple-by-default runtime summary plus optional deeper detail, which is a strong pattern reference for the GUI information hierarchy.

### Established Patterns
- The daemon is the sole authority; clients observe and mutate state only through DBus.
- Read surfaces are broadly accessible while mutating operations stay behind authorization checks.
- Configuration is staged in draft form, then validated and applied explicitly.
- Runtime state distinguishes managed, unmanaged, degraded, and fallback conditions in a way the GUI can mirror directly.
- The existing operator language favors concise default status with optional deeper diagnostic detail.

### Integration Points
- The main window overview can bind to inventory plus runtime state from the existing DBus read methods.
- Per-fan detail editing should bind to draft and applied config reads plus the existing write methods for enrollment and control-profile updates.
- Tray status updates can subscribe to daemon signal emission for draft, degraded, control-status, and auto-tune changes.
- Notification triggers should derive from degraded, fallback, and high-temp alert transitions already represented in runtime and lifecycle state.

</code_context>

<specifics>
## Specific Ideas

- The user wants the overview to feel like an operational dashboard first, not just a settings form.
- Output should be visible as a bar in the overview so the current control response is easy to scan.
- The fan-detail page is the right place for numeric configuration, auto-tune settings, and deeper control inspection.
- The user likes having an optional guided `Wizard configuration` entry point for setup.
- PID controls should include short hover explanations such as how raising a value affects responsiveness or stability.
- The user expects the tray to remain useful for quick inspection and notification surfacing, while future profile switching can build on it later.

</specifics>

<deferred>
## Deferred Ideas

- Short rolling PID graph or other historical time-series visualization in the per-fan detail page — this overlaps with future observability work rather than the core Phase 4 operator surface.
- Configurable high-temperature alarm policy — surfacing existing alert state is in scope, but alarm customization is a separate capability.
- Named fan profiles such as `silent`, `normal`, and `performance`, plus tray-based profile switching — profiles are explicitly out of v1 scope today.

</deferred>

---

*Phase: 04-kde-gui-tray-experience*
*Context gathered: 2026-04-11*
