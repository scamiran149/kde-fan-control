# Phase 2 Research: Safe Enrollment & Lifecycle Recovery

**Phase:** 02-safe-enrollment-lifecycle-recovery
**Date:** 2026-04-11
**Status:** Complete

## Objective

Research how to implement safe fan enrollment, daemon-owned lifecycle control, single-config persistence, best-effort boot recovery, and crash-path fallback on top of the Phase 1 inventory and DBus surfaces.

## Locked Inputs

- Staged edits with explicit apply for lifecycle configuration
- Future live setpoint changes should remain possible without redesigning the config model
- Best-effort partial boot apply
- Persistent degraded and fallback state plus small event history
- Read-open and write-privileged DBus access
- Near-term privileged writes without designing around production `sudo`
- Long-term package-installed service with room for `polkit` later

## Research Findings

### 1. Daemon Service Readiness

- `systemd.service(5)` recommends `Type=notify`, `Type=notify-reload`, or `Type=dbus` for long-running services that need precise startup semantics.
- For this daemon, `Type=notify` is the better long-term fit than `Type=dbus` alone because readiness should include more than acquiring a bus name.
- Phase 2 readiness should mean all of the following succeeded:
  - live inventory loaded
  - persisted config loaded
  - boot reconciliation completed
  - DBus object tree registered
  - initial degraded-state summary computed

**Planning implication:** design the daemon state machine so startup reconciliation is a first-class readiness step. Avoid treating DBus registration alone as successful startup.

### 2. DBus Contract Shape

- `zbus` supports async methods, properties, and signals cleanly enough to expose both configuration state and lifecycle notifications without inventing a polling-only API.
- The project should use DBus for authoritative daemon-owned lifecycle operations rather than client-side file mutation.
- Signals are the right mechanism for lifecycle changes like:
  - draft changed
  - applied config changed
  - degraded state entered or cleared
  - fallback entered or cleared
  - lifecycle event appended

**Planning implication:** Phase 2 should expose a draft/apply DBus contract with signals, not only snapshot retrieval methods.

### 3. Tokio Runtime Coordination

- `tokio::select!` is a good fit for daemon lifecycle loops that need to react to shutdown, apply requests, and hardware/state changes.
- Tokio `watch` or similar snapshot-oriented coordination is a good fit for current applied config state.
- Tokio broadcast-style event fanout is a good fit for small lifecycle event streams or change notifications.

**Planning implication:** separate long-lived daemon state from one-shot DBus request handling. Use coordinated async state, not ad hoc mutable globals.

### 4. Linux hwmon Permission Model

- The Linux hwmon sysfs ABI expects hardware-monitoring attributes to be world-readable while writable attributes remain privileged.
- This matches the project’s read-open and write-privileged DBus model.
- The daemon should remain the only writer to controllable sysfs fan nodes.

**Planning implication:** open inventory and status methods to unprivileged DBus callers, but keep enrollment, apply, and control-mode writes behind privileged authorization.

### 5. Enrollment Safety Model

- Phase 1 already established support classification and stable IDs.
- Phase 2 should reuse those IDs as the authority for persisted managed-fan references.
- Writable control without tach remains acceptable, but enrollment must still require confidence that the daemon can force safe maximum output later.
- Partial or unavailable hardware should remain visible but not be silently promoted into managed state.

**Planning implication:** persisted lifecycle config should reference the existing stable fan IDs, not path strings or labels.

### 6. Config Lifecycle Model

- The user explicitly wants staged edits with explicit apply.
- The current config system already provides daemon-owned persistence and is the correct extension point.
- The cleanest model is two layers:
  - draft config: mutable, user-editable, validated before apply
  - applied config: single authoritative configuration used for boot recovery and runtime behavior

**Planning implication:** Phase 2 should not mix draft and applied state in one structure. Validation must run before promote-to-applied.

### 7. Boot Reconciliation

- Best-effort partial apply is the desired behavior.
- Reconciliation should evaluate each previously managed fan independently.
- Outcomes per fan should include:
  - restored as managed
  - skipped because now unsafe or missing
  - degraded with explicit reason

**Planning implication:** startup should not fail the whole daemon just because one managed fan no longer matches. Startup should complete with a degraded summary and event history entry.

### 8. Crash And Fallback Semantics

- On daemon failure, previously daemon-controlled fans must move to safe maximum while unmanaged fans remain untouched.
- The inspectable state should distinguish degraded boot mismatch from active runtime fallback.
- Small event history is enough for v1; durable audit logging is not required in this phase.

**Planning implication:** Phase 2 must define a runtime lifecycle state model and explicit ownership tracking for fans the daemon has claimed.

### 9. Authorization Transition Strategy

- Near term, privileged writes are acceptable.
- Long term, the product should be distributable as `.deb` or `.rpm` and should not depend on production `sudo` usage.
- This is cleanly achievable if authorization is enforced at the daemon DBus boundary instead of inside CLI-specific logic.

**Planning implication:** Phase 2 should introduce an authorization seam around write operations so later `polkit` integration can replace the initial privileged-caller check without changing the DBus API.

## Recommended Architecture Direction

### Runtime Layers

1. Inventory layer
- owns live hardware discovery and support classification

2. Lifecycle config layer
- owns draft config and applied config
- validates enrollment and control-mode selections

3. Reconciliation layer
- resolves applied config against current live hardware on startup
- produces managed set, skipped set, degraded state, and lifecycle events

4. Control-ownership layer
- tracks which fans the daemon currently owns
- provides safe shutdown and fallback fan writes

5. DBus facade
- exposes read methods, write methods, and change signals

### Recommended State Model

Per fan, distinguish at least:
- `unmanaged`
- `managed`
- `degraded`
- `fallback`
- `unavailable`

For configuration, distinguish:
- `draft`
- `applied`

## Risks To Address In Plans

1. Persisting hardware references that no longer match at boot
2. Accidentally applying partial writes before validation succeeds
3. Fallback logic touching unmanaged fans
4. DBus writes being tied too closely to the CLI instead of daemon policy
5. Choosing a startup-ready signal that fires before reconciliation finishes

## Concrete Recommendations For The Planner

- Prefer `Type=notify` readiness semantics in the service design notes.
- Keep DBus writes daemon-authoritative and policy-neutral so auth can evolve later.
- Extend `AppConfig` to hold draft and applied lifecycle config separately.
- Reuse stable fan IDs from Phase 1 as persisted managed references.
- Implement best-effort per-fan reconciliation on startup.
- Record degraded or fallback transitions in a small bounded event history.
- Keep CLI as a thin DBus client that reflects draft/apply semantics explicitly.

## Suggested Plan Split

1. Managed config domain and persistence
2. DBus draft/apply and authorization contract
3. Boot reconciliation, ownership tracking, and fallback lifecycle
4. CLI lifecycle flows and inspectable degraded-state UX

## Sources

- `.planning/PROJECT.md`
- `.planning/REQUIREMENTS.md`
- `.planning/STATE.md`
- `.planning/phases/02-safe-enrollment-lifecycle-recovery/02-CONTEXT.md`
- `crates/core/src/config.rs`
- `crates/core/src/inventory.rs`
- `crates/daemon/src/main.rs`
- `crates/cli/src/main.rs`
- Context7 `/dbus2/zbus` — service interfaces, properties, and signals
- Context7 `/tokio-rs/tokio` — async task coordination and shutdown patterns
- `https://man7.org/linux/man-pages/man5/systemd.service.5.html`
- `https://www.kernel.org/doc/html/latest/hwmon/sysfs-interface.html`
