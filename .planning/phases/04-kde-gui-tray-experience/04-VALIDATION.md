---
phase: 4
slug: kde-gui-tray-experience
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-11
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Qt Test (QTest) for C++ unit tests; Qt Quick Test for QML tests |
| **Config file** | `gui/tests/CMakeLists.txt` |
| **Quick run command** | `ctest --test-dir gui/build -L quick --output-on-failure` |
| **Full suite command** | `ctest --test-dir gui/build --output-on-failure` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Build GUI target + run quick tests
- **After every plan wave:** Full CMake configure, build, and test suite
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 01 | 1 | GUI-01 | T-04-01 | DBus reads restricted to system bus, no sysfs access | build | `cmake --build gui/build && ctest --test-dir gui/build -L quick` | ❌ W0 | ⬜ pending |
| 04-02-01 | 02 | 2 | GUI-02 | T-04-03 | Write methods require authorization, errors surfaced | build | `cmake --build gui/build && ctest --test-dir gui/build -L quick` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `gui/tests/CMakeLists.txt` — test infrastructure and fixtures
- [ ] `gui/src/models/` — model stubs for unit testing
- [ ] Qt Test and Qt Quick Test frameworks configured in CMake

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Visual layout matches UI-SPEC | GUI-01, GUI-02 | Rendering requires display server | Launch GUI, verify Overview, Inventory, FanDetail pages match spec |
| Tray icon appearance and popover | GUI-04 | Requires Plasma desktop | Right-click tray icon, verify popover structure |
| Notification delivery | GUI-04, D-11 | Requires desktop notification service | Trigger degraded/fallback state in daemon, verify notification appears |
| Keyboard navigation order | GUI-01 | Accessibility verification | Tab through all controls, verify order follows visual layout |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending