---
phase: 04-kde-gui-tray-experience
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - .planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-VERIFICATION.md
autonomous: false
requirements:
  - GUI-01
  - GUI-02
  - GUI-03
  - GUI-04
  - GUI-05
must_haves:
  truths:
    - "VERIFICATION.md reflects verification of the BUILT GUI binary, not just source code review"
    - "All previously claimed verifications are re-confirmed against actual build artifacts and runtime behavior"
    - "Review critical/warning findings are checked in the built code"
    - "Stale or inaccurate claims are corrected"
  artifacts:
    - path: ".planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-VERIFICATION.md"
      provides: "Re-verified report against built GUI application"
  key_links:
    - from: "gui/build/gui_app"
      to: "04-VERIFICATION.md"
      via: "build artifact verification"
      pattern: "ELF.*executable"
---

<objective>
Re-verify Phase 4 (KDE GUI & Tray Experience) against the actually-built GUI application binary and runtime artifacts, correcting the previous VERIFICATION.md which was a code review conducted before the GUI was built.

Purpose: The prior verification was a source-code review, not a build/runtime verification. Now that `gui_app` exists as a ~975KB ELF binary with all KF6 dependencies resolved and the daemon is running on the system bus, the verification report must reflect the actual built state.

Output: Updated `04-VERIFICATION.md` reflecting verification of the built GUI, with corrected evidence references and resolved/updated finding statuses.
</objective>

<execution_context>
@$HOME/.config/opencode/get-shit-done/workflows/execute-plan.md
@$HOME/.config/opencode/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@.planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-VERIFICATION.md
@.planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-REVIEW.md
@.planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-UAT-CHECKLIST.md
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Verify built GUI artifacts and correct VERIFICATION.md evidence</name>
  <files>.planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-VERIFICATION.md</files>
  <behavior>
    - Verify gui_app ELF binary exists and links to required KF6/Qt6 libraries
    - Verify all QML modules are bundled/accessible at runtime
    - Verify no TODO/FIXME/PLACEHOLDER remains in source
    - Verify no direct sysfs access from GUI source
    - Verify CR-01 fix (handleNameOwnerChanged is declared and defined)
    - Verify CR-02 is resolved (no hardcoded library paths, uses find_package)
    - Verify all WR findings against current source state
    - Check behavioral spot-checks against the built binary
    - Update VERIFICATION.md with build-artifact evidence replacing source-only claims
  </behavior>
  <action>
    Run the following verification checks and update 04-VERIFICATION.md to reflect verification against the built GUI binary:

    **Phase 1 — Build artifact verification:**
    1. `file gui/build/gui_app` — confirm ELF 64-bit executable
    2. `ldd gui/build/gui_app` — confirm KF6::StatusNotifierItem, KF6::Notifications, Qt6::DBus, etc. are linked
    3. `ls -la gui/build/org/kde/fancontrol/` — confirm QML module structure exists
    4. Check that all `.qml` files are bundled in the QML module (check qt_add_qml_module in CMakeLists.txt)

    **Phase 2 — Source verification (re-run against current source):**
    5. `grep -rn "TODO\|FIXME\|PLACEHOLDER" gui/src/ gui/qml/` — must produce no results
    6. `grep -rn "/sys/class/hwmon" gui/src/ gui/qml/` — must produce no results
    7. `grep -n "handleNameOwnerChanged" gui/src/daemon_interface.h gui/src/daemon_interface.cpp` — verify CR-01 fix: method must be declared in header and defined in cpp
    8. `grep -n "KF6_SNI_LIB\|KF6_NOTIF_LIB\|/usr/lib/x86_64" gui/CMakeLists.txt` — verify CR-02 fix: no hardcoded library paths
    9. `grep -n "find_package(KF6" gui/CMakeLists.txt` — confirm proper CMake find_package usage
    10. Check each WR and IN finding from 04-REVIEW.md against current source to determine if it's been resolved since review

    **Phase 3 — Daemon connectivity verification:**
    11. `busctl --system list | grep FanControl` — verify daemon is on system bus
    12. `busctl --system introspect org.kde.FanControl /org/kde/FanControl` — verify DBus interface is available
    13. Verify the 3 DBus interfaces (Inventory, Lifecycle, Control) are present on the bus

    **Phase 4 — Update VERIFICATION.md:**
    Replace the existing 04-VERIFICATION.md with an updated version that:
    - Changes `verified` timestamp to current date
    - Changes `status` to `re-verified` 
    - Updates Behavioral Spot-Checks to reflect build verification (binary builds, links correctly)
    - Updates the "Human Verification Required" section to reflect what's actually testable now
    - Adds a "Review Findings Re-Check" section showing the status of each CR and WR finding against current source
    - Removes stale "previous_status: failed" gap references that were already resolved
    - Updates evidence where previous claims said "X lines of code" — re-verify line counts
    - Marks the verification as "against built application" rather than "code review"
    - Preserves all 10 truth verifications that remain valid
    - Updates Key Link Verification evidence to reflect actual artifact paths (gui/build/gui_app)
    - Updates Required Artifacts evidence with "✓ BUILT" status for the binary alongside source verification

    **Key principles for the update:**
    - Every truth that was verified should be re-confirmed against current source
    - The verification was previously a code review — now it must evidence the built binary
    - Review findings (CR-01, CR-02, WR-01 through WR-08, IN-01 through IN-10) should each be checked for resolution status
    - The "Human Verification Required" items should remain but note that the GUI is now buildable and launchable
    - The deferred truth about polling vs DBus signals should remain unchanged
  </action>
  <verify>
    <automated>test -f .planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-VERIFICATION.md && grep -q "re-verified" .planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-VERIFICATION.md && grep -q "gui_app" .planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-VERIFICATION.md && grep -q "Review Findings" .planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-VERIFICATION.md</automated>
  </verify>
  <done>04-VERIFICATION.md is updated with re-verification against the built GUI binary, review findings are status-checked against current source, and all evidence reflects build artifacts rather than source-only review claims.</done>
</task>

<task type="checkpoint:human-verify">
  <name>Task 2: Human review of updated VERIFICATION.md</name>
  <files>.planning/milestones/v1.0-phases/04-kde-gui-tray-experience/04-VERIFICATION.md</files>
  <action>Human reviews the updated VERIFICATION.md to confirm: (1) it correctly references the built gui_app binary, (2) review findings have current resolution status, (3) truth evidences reflect current source, (4) human verification items are accurate for runtime testing.</action>
  <verify>Human types "approved" to confirm</verify>
  <done>VERIFICATION.md is confirmed accurate and complete by human reviewer</done>
  <what-built>Updated Phase 4 VERIFICATION.md reflecting verification against the built GUI application binary and re-checked review findings</what-built>
  <how-to-verify>
    1. Read the updated VERIFICATION.md
    2. Confirm it references the built gui_app binary
    3. Confirm review findings (CR-01, CR-02, WR-* and IN-*) have current resolution status
    4. Confirm "Human Verification Required" items are accurate for what needs manual runtime testing
    5. Spot-check 2-3 truth evidences to confirm they reflect current source state
    6. If satisfied, type "approved"
    7. If corrections needed, describe what to fix
  </how-to-verify>
  <resume-signal>Type "approved" or describe corrections needed</resume-signal>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| Read-only verification | This task only reads source and build artifacts — no production code changes |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-04-01 | Tampering | VERIFICATION.md | accept | Verification report is documentation, not runtime code; no integrity risk |
</threat_model>

<verification>
- VERIFICATION.md references `gui_app` built binary
- VERIFICATION.md contains "Review Findings Re-Check" section
- All 10 original truths remain verified with updated evidence
- Status is "re-verified" 
- Review findings have resolution status checked against current source
</verification>

<success_criteria>
- 04-VERIFICATION.md is updated to reflect verification against the built GUI binary
- All review findings from 04-REVIEW.md have been checked for resolution status
- Behavioral spot-checks include build verification evidence
- Human verification items are accurate for runtime testing of the built application
</success_criteria>

<output>
After completion, create `.planning/quick/260412-grd-re-verify-phase-4-with-built-gui-applica/260412-SUMMARY.md`
</output>