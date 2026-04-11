# Phase 2: Safe Enrollment & Lifecycle Recovery - Context

**Gathered:** 2026-04-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 2 adds safe fan enrollment, daemon-owned lifecycle control, persisted managed configuration, boot-time recovery, and failure fallback behavior on top of the Phase 1 inventory surface. This phase is about deciding which fans the daemon owns, how that ownership is persisted and applied, and how degraded or fallback states are surfaced when hardware or runtime conditions are unsafe.

</domain>

<decisions>
## Implementation Decisions

### Configuration Apply Model
- **D-01:** Enrollment and lifecycle configuration should use staged edits with an explicit apply step rather than immediately applying every change.
- **D-02:** The configuration model should still leave room for selected live-adjustment paths later, especially target temperature changes during tuning, even though most Phase 2 enrollment changes are staged and applied as a validated batch.

### Boot Recovery And Hardware Mismatch
- **D-03:** Boot recovery should use best-effort partial apply, not all-or-nothing startup.
- **D-04:** If previously managed hardware still matches safely, the daemon should resume management automatically on boot.
- **D-05:** If a previously managed fan no longer matches safely, that fan should be skipped, surfaced clearly as degraded or mismatched, and not silently claimed as managed.

### Failure And Degraded-State UX
- **D-06:** Fallback and degraded states should remain persistently visible until resolved rather than appearing only as transient errors.
- **D-07:** The daemon should retain a small event history describing what happened and when for lifecycle and fallback incidents.
- **D-08:** The status model should support future tray and desktop-notification surfacing in the KDE UI, even if that user-facing presentation lands in Phase 4.

### DBus Access Policy
- **D-09:** Read access to inventory and runtime status should be open to local users.
- **D-10:** Write operations that change enrollment, lifecycle, or daemon-owned configuration should require privileged authorization.

### Already Locked From Project Context
- **D-11:** v1 keeps a single active daemon-owned configuration.
- **D-12:** Managed fans auto-start on boot from the persisted active configuration.
- **D-13:** Unsafe hardware remains visible but enrollment is refused.
- **D-14:** Writable control without tach feedback is acceptable for enrollment if safe control and fallback semantics are available.

### the agent's Discretion
- Exact DBus object layout, method naming, and signal structure, as long as it preserves staged apply plus read-open or write-privileged policy.
- Exact persistence schema and internal state-machine structure, as long as it supports single-config ownership, boot auto-resume, degraded-state tracking, and future live tuning hooks.
- Exact event-history representation, retention count, and formatting, as long as it stays small and useful for users diagnosing fallback or degraded state.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Product And Scope
- `.planning/PROJECT.md` — Product constraints, daemon-owned persistence, boot behavior, safety boundaries, and previously locked architecture decisions.
- `.planning/REQUIREMENTS.md` — Phase 2 requirements for enrollment, safe fallback, persistence, DBus authority, and CLI-backed lifecycle behavior.
- `.planning/ROADMAP.md` — Phase 2 goal, success criteria, dependency on Phase 1, and scoped plans for safe enrollment and recovery.
- `.planning/STATE.md` — Current project state, current blockers, and continuity notes affecting Phase 2 planning.

### Existing Implementation
- `crates/core/src/inventory.rs` — Current inventory model, support classification, stable identities, and capability surface that Phase 2 will build on.
- `crates/core/src/config.rs` — Existing daemon-owned persisted config pattern and current friendly-name storage path.
- `crates/daemon/src/main.rs` — Current daemon ownership boundary, DBus inventory interface, and config-loading pattern.
- `crates/cli/src/main.rs` — Current CLI-to-DBus interaction pattern and fallback behavior.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `crates/core/src/inventory.rs`: Already provides stable hardware IDs, support-state classification, and fan capability details that Phase 2 can reuse for enrollment decisions.
- `crates/core/src/config.rs`: Already establishes a daemon-owned TOML persistence path and simple config load/save helpers that can be extended to store managed fan lifecycle configuration.
- `crates/daemon/src/main.rs`: Already owns DBus exposure and config loading, which is the natural integration point for staged edits, apply, boot recovery, and degraded-state reporting.
- `crates/cli/src/main.rs`: Already models the CLI as a thin DBus client, which aligns with the project decision that lifecycle changes should be daemon-authoritative rather than direct file edits.

### Established Patterns
- The current project split is shared core model plus thin daemon and CLI layers.
- Persistence is already daemon-owned rather than client-owned.
- DBus currently exposes snapshot-oriented data and simple mutation methods; Phase 2 should extend that pattern rather than bypass it.
- Hardware state is derived from live inventory and then decorated with persisted friendly-name config, which suggests managed enrollment state should follow the same authoritative-merge model.

### Integration Points
- Enrollment eligibility should hang off the existing `FanChannel` capability and support-state model.
- Persisted managed configuration should extend `AppConfig` in `crates/core/src/config.rs`.
- Boot auto-resume and degraded-state calculation should live in the daemon startup path in `crates/daemon/src/main.rs`.
- CLI enrollment and apply commands should layer onto the existing DBus proxy pattern in `crates/cli/src/main.rs`.

</code_context>

<specifics>
## Specific Ideas

- The user wants a staged draft plus apply model for most lifecycle changes.
- The user specifically wants target temperature changes to feel live so fan response is immediately visible during tuning.
- The user wants degraded and fallback states to feel persistent and diagnosable, not ephemeral.
- The user explicitly wants this lifecycle and fault state to later surface in the conceptualized tray icon and desktop notification flow.

</specifics>

<deferred>
## Deferred Ideas

- Live target-temperature adjustment during tuning is a concrete requirement for later control work, but the actual user-facing tuning workflow belongs primarily to Phase 3.
- KDE tray icon and persistent desktop notification UX are important, but the actual UI implementation belongs to Phase 4.

</deferred>

---

*Phase: 02-safe-enrollment-lifecycle-recovery*
*Context gathered: 2026-04-11*
