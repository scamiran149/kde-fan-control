# Requirements: KDE Fan Control

**Defined:** 2026-04-11
**Core Value:** Users can safely and flexibly control desktop fan behavior with understandable per-fan PID policies, without losing fail-safe behavior.

## v1.1 Requirements

Requirements for packaging and system integration milestone. Each maps to roadmap phases.

### FHS & Paths

- [ ] **PATH-01**: All installed artifacts follow standard FHS paths (daemon in /usr/sbin, GUI/CLI in /usr/bin, config in /etc, data in /usr/share)
- [ ] **PATH-02**: CLI binary installed to /usr/bin/kde-fan-control-cli with /usr/bin/kfc symlink

### systemd Integration

- [ ] **SYSD-01**: systemd unit file for the daemon with Type=notify, Restart=on-failure, WantedBy=graphical.target
- [ ] **SYSD-02**: Daemon signals readiness via sd-notify READY=1 after hwmon discovery, config load, and DBus registration
- [ ] **SYSD-03**: WatchdogSec= with periodic WATCHDOG=1 pings from the daemon
- [ ] **SYSD-04**: ExecStopPost= fallback helper that forces owned fans to safe-max on crash or stop
- [ ] **SYSD-05**: Service hardening directives (ProtectSystem, ProtectHome, NoNewPrivileges, ReadWritePaths)
- [ ] **SYSD-06**: StandardOutput=journal and StandardError=journal for structured log capture

### DBus Integration

- [ ] **DBUS-01**: DBus service activation file with SystemdService= key for on-demand daemon start
- [ ] **DBUS-02**: DBus policy file installed to /usr/share/dbus-1/system.d/
- [ ] **DBUS-03**: dbus-org.kde.FanControl.service symlink enabling systemd-managed activation

### Polkit Authorization

- [ ] **AUTH-01**: polkit .policy file declaring granular actions (enroll-fan, apply-config, write-config, start-auto-tune, manage-daemon) with auth_admin_keep for interactive ops
- [ ] **AUTH-02**: Daemon replaces require_authorized UID=0 check with polkit CheckAuthorization via zbus
- [ ] **AUTH-03**: Unprivileged GUI/CLI users can perform privileged operations after polkit authentication prompt

### Desktop Integration

- [ ] **DESK-01**: freedesktop .desktop file for the GUI installed to /usr/share/applications/
- [ ] **DESK-02**: SVG app icon installed to /usr/share/icons/hicolor/scalable/apps/

### Packaging

- [ ] **PACK-01**: .deb package (fancontrold + kde-fan-control) with postinst/postrm for systemd daemon-reload, enable, and start
- [ ] **PACK-02**: install.sh fallback script that mirrors .deb file installs, requires root, and is idempotent

## v2 Requirements (Deferred)

### Enhanced Packaging

- **PACK-03**: AppStream metadata (.metainfo.xml) for software center discoverability
- **PACK-04**: RPM packaging for Fedora/openSUSE
- **PACK-05**: APT repository hosting with signed packages for automatic updates

### Advanced Security

- **SEC-01**: SELinux/AppArmor profile for the daemon
- **SEC-02**: Tightened DBus method-level policy (deny write methods by default, allow individually)

### GUI Enhancement

- **GUI-01**: Autostart .desktop file for optional login auto-launch
- **GUI-02**: GUI shows polkit authentication prompt natively via KDE Auth

## Out of Scope

| Feature | Reason |
|---------|--------|
| Flatpak/sandboxed GUI | GUI needs direct DBus system-bus and native Qt/Kirigami behavior; sandboxing contradicts product purpose |
| Session bus for the daemon | Daemon is a system service controlling hardware; session bus has wrong privilege/lifecycle model |
| User systemd service for the daemon | Daemon writes sysfs hwmon and must survive logout; wrong privilege model |
| Bundled/config-file polkit rules | polkit docs say applications must never install authorization rules; only administrators write .rules files |
| Direct sysfs writes from GUI/CLI | Breaks privilege boundaries and bypasses safety layer |
| Config file as dpkg conffile | Daemon-owned live config must not be overwritten on upgrade; use runtime copy-from-template |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| PATH-01 | — | Pending |
| PATH-02 | — | Pending |
| SYSD-01 | — | Pending |
| SYSD-02 | — | Pending |
| SYSD-03 | — | Pending |
| SYSD-04 | — | Pending |
| SYSD-05 | — | Pending |
| SYSD-06 | — | Pending |
| DBUS-01 | — | Pending |
| DBUS-02 | — | Pending |
| DBUS-03 | — | Pending |
| AUTH-01 | — | Pending |
| AUTH-02 | — | Pending |
| AUTH-03 | — | Pending |
| DESK-01 | — | Pending |
| DESK-02 | — | Pending |
| PACK-01 | — | Pending |
| PACK-02 | — | Pending |

**Coverage:**
- v1.1 requirements: 18 total
- Mapped to phases: 0
- Unmapped: 18 ⚠️

---
*Requirements defined: 2026-04-11*
*Last updated: 2026-04-11 after v1.1 requirements definition*