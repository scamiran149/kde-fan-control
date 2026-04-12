# Milestones

## v1.0 MVP (Shipped: 2026-04-12)

**Phases completed:** 4 phases, 15 plans, 29 tasks

**Key accomplishments:**

- Versioned draft/applied lifecycle config with validation helpers, degraded-state tracking, and preserved friendly-name persistence
- DBus lifecycle surface with staged edits, explicit apply, read-open/write-privileged access, and change signals for draft, applied, degraded, and event state
- Hwmon inventory now distinguishes PWM-only fans from channels with writable direct-current mode switching, and serialized snapshots preserve those discovered control choices for daemon and CLI consumers.
- Crash and shutdown fallback now share one durable recorder that persists owned-fan incidents, reloads fallback visibility after restart, and keeps lifecycle history distinct from boot reconciliation state.
- Temperature-target PID contracts with validated per-fan control profiles and managed-fan runtime control snapshots in core.
- Live daemon-owned fan PID loops with read-open DBus control status and applied-config task reconciliation.
- Privileged daemon auto-tune with reviewable proposals, read-open inspection, and draft-only staging for tuned or edited PID control profiles.
- CLI runtime status with optional PID detail plus staged control-profile and auto-tune review flows over the control DBus interface.
- Serde(default) on AppliedFanEntry enables Phase 2 config files to deserialize with safe defaults; dead_code warnings suppressed on test-only daemon helpers
- C++ DBus bridge and QML application shell with overview dashboard and inventory page for KDE fan control
- Draft editing model, lifecycle event model, and fan detail page with core controls, auto-tune flow, and advanced tabs
- KStatusNotifierItem tray icon, KNotification alert handler, and compact tray popover with managed fan list
- Guided wizard dialog for fan enrollment using the same draft/apply contract, with conditional aggregation step and review-then-apply flow

---
