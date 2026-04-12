---
plan: 02-04
phase: 02-safe-enrollment-lifecycle-recovery
status: complete
started: 2026-04-11
completed: 2026-04-11
---

# Plan 02-04: CLI Lifecycle Flows And Inspectable Recovery State

## Summary

Implemented the Phase 2 lifecycle CLI surface as a thin DBus client. The CLI now supports staging enrollment changes, inspecting draft and applied configuration, validating and applying drafts, discarding drafts, and reading degraded state, lifecycle events, and runtime state in both text and JSON forms.

## Tasks Completed

| # | Task | Status |
|---|------|--------|
| 1 | Extend CLI subcommands | ✓ Complete |
| 2 | Improve inspectability | ✓ Complete |
| 3 | Preserve thin-client architecture | ✓ Complete |
| 4 | Keep the future UX path open | ✓ Complete |

## Key Files

### Modified
- `crates/cli/src/main.rs` — Added lifecycle draft/applied/degraded/event/state commands, staged enrollment flows, and clearer permission-aware output

## User-Facing Outcomes

- Users can stage managed or unmanaged fan enrollment through DBus-backed CLI commands
- Users can inspect draft vs applied configuration distinctly
- Users can validate, apply, or discard the draft explicitly
- Users can inspect degraded-state reasons, lifecycle events, and current runtime state without reading daemon logs
- CLI output makes it clear that staged changes are not live until `apply`

## Deviations

None — implementation stayed within the plan boundary.

## Commits

- `2f15d41` — feat(02-04): add CLI lifecycle flows with inspectable recovery state
