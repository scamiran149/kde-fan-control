# Roadmap: KDE Fan Control

## Milestones

- ✅ **v1.0 MVP** — Phases 1-4 (shipped 2026-04-12)
- 🚧 **v1.1 Packaging & System Integration** — Phases 5-8 (in progress)

## Phases

<details>
<summary>✅ v1.0 MVP (Phases 1-4) — SHIPPED 2026-04-12</summary>

- [x] Phase 1: Hardware Inventory & Visibility (4 plans) — completed 2026-04-11
- [x] Phase 2: Safe Enrollment & Lifecycle Recovery (6 plans) — completed 2026-04-11
- [x] Phase 3: Temperature Control & Runtime Operations (5 plans) — completed 2026-04-11
- [x] Phase 4: KDE GUI & Tray Experience (4 plans) — completed 2026-04-11

</details>

### 🚧 v1.1 Packaging & System Integration (In Progress)

**Milestone Goal:** Make KDE Fan Control properly installable, boot-persistent, supervisor-managed, and authorize-able on Linux desktops.

- [ ] **Phase 5: System Integration Files** — Create all static integration, desktop, and policy files at standard FHS paths
- [ ] **Phase 6: Daemon System Integration** — Wire sd-notify readiness/watchdog and polkit CheckAuthorization into the daemon
- [ ] **Phase 7: Crash-Safe Recovery** — Build ExecStopPost fallback helper that guarantees owned fans reach safe-max on daemon exit
- [ ] **Phase 8: Distribution Packaging** — Assemble everything into installable .deb packages and install.sh fallback

## Phase Details

### Phase 5: System Integration Files
**Goal**: All integration and desktop configuration files exist at standard FHS paths with correct content, establishing the canonical file layout that daemon integration and packaging depend on
**Depends on**: Phase 4
**Requirements**: PATH-01, PATH-02, SYSD-01, SYSD-05, SYSD-06, DBUS-01, DBUS-02, DBUS-03, AUTH-01, DESK-01, DESK-02
**Success Criteria** (what must be TRUE):
  1. All project artifacts install to their correct FHS locations (daemon in /usr/sbin, CLI in /usr/bin with kfc symlink, config in /etc, shared data in /usr/share)
  2. The systemd unit file parses without error and specifies Type=notify, WantedBy=graphical.target, hardening directives, and journal output
  3. DBus service activation file and system policy are valid XML installed to standard system bus paths, with the activation file delegating to systemd via SystemdService=
  4. KDE Fan Control appears in the KDE application launcher with correct name, comment, and SVG icon
  5. polkit action list shows the five fan control actions (enroll-fan, apply-config, write-config, start-auto-tune, manage-daemon) with auth_admin_keep default
**Plans**: TBD

### Phase 6: Daemon System Integration
**Goal**: The daemon integrates with systemd lifecycle management and polkit authorization, enabling supervised boot-persistent operation and authenticated privileged access for unprivileged users
**Depends on**: Phase 5
**Requirements**: SYSD-02, SYSD-03, AUTH-02, AUTH-03
**Success Criteria** (what must be TRUE):
  1. Daemon signals READY=1 to systemd only after hwmon discovery, config load, and DBus name registration all succeed; systemd reports the service as active (running)
  2. Daemon sends periodic WATCHDOG=1 pings that prevent systemd watchdog timeout; systemd tracks the service as alive
  3. An unprivileged user can perform a privileged fan operation (e.g., apply-config) after authenticating via a polkit prompt — no sudo or root required
  4. Daemon falls back to UID=0 check when polkit authority is unavailable, preserving existing root-or-nothing behavior
**Plans**: TBD

### Phase 7: Crash-Safe Recovery
**Goal**: Owned fans are guaranteed to reach safe-maximum speed even if the daemon crashes, is killed, or fails during startup — no thermal risk from stale PWM values
**Depends on**: Phase 5
**Requirements**: SYSD-04
**Success Criteria** (what must be TRUE):
  1. When the daemon service stops or crashes, the ExecStopPost helper forces all previously-owned fans to safe-maximum PWM within seconds
  2. The recovery helper works independently of the daemon process (reads persisted enrolled-fan list from disk, no IPC dependency on the daemon)
  3. Owned fans never remain at their last PWM value after an unclean daemon exit — they always reach safe-maximum
**Plans**: TBD

### Phase 8: Distribution Packaging
**Goal**: Users can install KDE Fan Control via a .deb package or a self-contained install script, with proper post-install service enablement and clean removal
**Depends on**: Phase 5, Phase 6, Phase 7
**Requirements**: PACK-01, PACK-02
**Success Criteria** (what must be TRUE):
  1. Installing the .deb package places all artifacts at correct FHS paths, reloads systemd, enables and starts the daemon
  2. Removing the .deb package stops the daemon and cleans up systemd unit files
  3. Running install.sh as root installs all artifacts to the same FHS paths as the .deb, idempotently (repeatable without error)
  4. After either install method, the daemon is running and the GUI launches from the application menu
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 5 → 6 → 7 → 8

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Hardware Inventory & Visibility | v1.0 | 4/4 | Complete | 2026-04-11 |
| 2. Safe Enrollment & Lifecycle Recovery | v1.0 | 6/6 | Complete | 2026-04-11 |
| 3. Temperature Control & Runtime Operations | v1.0 | 5/5 | Complete | 2026-04-11 |
| 4. KDE GUI & Tray Experience | v1.0 | 4/4 | Complete | 2026-04-11 |
| 5. System Integration Files | v1.1 | 0/? | Not started | - |
| 6. Daemon System Integration | v1.1 | 0/? | Not started | - |
| 7. Crash-Safe Recovery | v1.1 | 0/? | Not started | - |
| 8. Distribution Packaging | v1.1 | 0/? | Not started | - |