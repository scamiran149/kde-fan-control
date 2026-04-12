# Project Research Summary

**Project:** KDE Fan Control — Packaging & System Integration
**Domain:** Linux desktop system service packaging (systemd, DBus, polkit, .deb, .desktop)
**Researched:** 2026-04-11
**Confidence:** HIGH

## Executive Summary

KDE Fan Control is a Linux desktop fan-control application with a privileged Rust daemon, a Qt6/Kirigami GUI, and a CLI — all communicating via DBus on the system bus. This research covers the **packaging and system integration milestone**: making the existing application properly installable, boot-persistent, supervisor-managed, and authorize-able on a Linux desktop. This is a well-understood domain with clear reference implementations (UDisks2, ModemManager, power-profiles-daemon, corectrl, thermald) that demonstrate exactly how root DBus services, polkit policies, systemd units, `.desktop` files, and packaging should compose.

The recommended approach is a four-phase build: (1) create all static data files (systemd unit, DBus activation, polkit policy, `.desktop`, icons), (2) wire `sd-notify` and polkit `CheckAuthorization` into the daemon, (3) add `ALLOW_INTERACTIVE_AUTHORIZATION` flags and install targets to the GUI, and (4) assemble everything into a `.deb` package plus `install.sh` fallback. Only one new Rust crate is needed (`sd-notify 0.5.0`); everything else is declarative config files. The polkit daemon-side wiring — replacing the existing `require_authorized()` UID-0 check with a `CheckAuthorization` DBus call — is the only moderate-complexity code change.

The key risks are operational, not architectural: systemd hardening directives silently blocking hwmon sysfs writes, `ExecStopPost=` not being configured for crash-safe fan fallback, and the polkit `.policy` file being installed without actually wiring the daemon to call `CheckAuthorization`. Each of these produces a broken system that *looks* healthy — fans appear managed but aren't, or auth appears configured but isn't. Prevention requires end-to-end testing with real hardware, not just "service starts" validation.

## Key Findings

### Recommended Stack

The packaging milestone adds only one new Rust dependency (`sd-notify 0.5.0`) and relies on declarative system-integration files. The existing stack (Rust/Tokio/zbus daemon, Qt6/Kirigami GUI, DBus system bus) is unchanged. All packaging work is about formalizing and completing integration points that already partially exist (e.g., the DBus bus name and basic policy config are already in the codebase).

**Core additions:**
- **sd-notify 0.5.0**: systemd readiness + watchdog keep-alive — pure Rust, no C FFI, standard choice
- **systemd unit file (Type=notify)**: boot persistence, supervision, watchdog, crash recovery — `Type=notify` over `Type=simple` because daemon must validate hwmon + config + DBus before "ready"
- **DBus service activation (.service file)**: on-demand daemon start via `SystemdService=` delegation — lets systemd manage the process, not dbus-daemon
- **Polkit .policy XML**: replaces hard UID-0 check with `auth_admin_keep` — standard Linux privilege escalation for desktop apps; no new crate needed (zbus calls `org.freedesktop.PolicyKit1` directly)
- **.deb package (two packages: `fancontrold` + `kde-fan-control`)**: primary install target; manual `dpkg-deb` build because `cargo-deb` can't handle the Qt artifacts
- **install.sh**: POSIX shell fallback for non-Debian systems

### Expected Features

**Must have (table stakes):**
- systemd unit file with `Type=notify`, watchdog, `ExecStopPost=` fallback, and hardening — any Linux system service needs this
- Daemon enabled at boot (`systemctl enable` in postinst)
- DBus service activation file for on-demand daemon start when GUI launches first
- DBus system bus policy installed to `/usr/share/dbus-1/system.d/`
- Polkit policy replacing UID-0 check — enables proper auth prompts instead of requiring `sudo`
- `.desktop` file for GUI discoverability in KDE menu/launcher
- App icon in hicolor theme (SVG + PNG fallbacks) for `.desktop` and tray
- CLI in PATH at `/usr/bin/kfc`
- Standard FHS file layout across all install paths
- `.deb` package with postinst/prerm hooks for daemon-reload, enable, start
- `install.sh` fallback installer

**Should have (differentiators):**
- `auth_admin_keep` polkit default — avoids repeated password prompts for 5 minutes
- Journal integration (`StandardOutput=journal`) — `tracing` logs queryable via `journalctl -u`
- `dbus-org.kde.FanControl.service` alias symlink — standard pattern for bus activation
- Documented hardening exceptions — shows security posture audibly, unlike `fancontrol`

**Defer (v2+):**
- AppStream metadata (`.metainfo.xml`) — nice for software centers, not actionable without repo indexing
- RPM/Arch/COPR packaging — per-distro contribution pattern
- APT repository hosting — requires CI + signing + hosting
- Flatpak/sandboxed GUI — fights the product's need for direct sysfs + DBus system-bus access
- SELinux/AppArmor profiles — niche for initial KDE-desktop target

### Architecture Approach

The packaging layer is a thin integration surface that sits between the existing application components and the Linux system. It follows six established patterns: (1) **systemd Type=notify with deferred readiness** — daemon signals READY=1 only after hwmon discovery, config load, and DBus name acquisition all succeed; (2) **DBus service activation → systemd** — `.service` file with `SystemdService=` tells dbus-daemon to delegate to systemd; (3) **Polkit authorization replacing UID-0** — daemon calls `CheckAuthorization` with caller identity; falls back to UID-0 if polkit unavailable; (4) **FHS-standard install paths** — all files at their conventional Linux locations; (5) **Dual packaging** — `.deb` primary + `install.sh` fallback, using identical file paths; (6) **Single polkit action** — one `write-config` action covers all privileged writes, avoiding unnecessary granularity.

**Major components:**
1. **systemd unit file** — daemon lifecycle, boot startup, readiness, watchdog, crash recovery, hardening
2. **DBus activation + policy files** — on-demand start, bus-level ACL for name ownership and message routing
3. **Polkit policy + daemon auth wiring** — defines privileged actions; daemon checks them per-method
4. **.deb package + install.sh** — assembles all artifacts into installable form with correct FHS paths and postinst hooks

### Critical Pitfalls

1. **`ProtectSystem=strict` silently blocks hwmon writes** — daemon appears healthy but cannot control fans. Add `ReadWritePaths=/sys/class/hwmon`; do NOT use `PrivateDevices=yes`; test with real hwmon writes, not just service-start checks.
2. **Polkit policy installed but not wired into daemon** — `.policy` file exists but daemon still checks `uid == 0`. Must replace `require_authorized()` body with `CheckAuthorization` call; this is a mandatory code change, not just file installation.
3. **`ExecStopPost=` not configured for crash-safe fallback** — `ExecStop=` is skipped on crash or startup failure. Fans stay at last PWM value (possibly low). Install `ExecStopPost=` with a standalone recovery helper that reads persisted enrolled-fan list and forces PWM to safe-max.
4. **DBus activation file `SystemdService=` name mismatch** — on-demand daemon start fails silently. File name must match bus name; `SystemdService=` value must exactly match the installed unit filename. Test: stop daemon, launch GUI, verify auto-start works.
5. **`.deb` conffile handling overwrites daemon config on upgrade** — don't mark daemon-owned config as a dpkg conffile. Install a default template to `/usr/share/`, let the daemon copy to its runtime location on first start. Include a `config_version` field from day one.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Static Data Files & File Layout
**Rationale:** All packaging artifacts are declarative files with zero code dependencies. Getting them right first provides the foundation that daemon integration, GUI integration, and packaging all depend on. Canonical file paths must be defined before any install logic is written.
**Delivers:** systemd unit file, DBus activation file, DBus policy (extended), polkit `.policy` XML, `.desktop` file, icon SVG/PNGs, canonical FHS path mapping
**Addresses:** systemd unit, DBus activation, polkit policy, `.desktop` file, CLI in PATH, FHS layout, icons
**Avoids:** Pitfall #20 (install.sh and .deb install to different paths) — canonical layout defined once before either exists

### Phase 2: Daemon Integration (sd-notify + Polkit)
**Rationale:** The daemon needs two code changes: sd-notify readiness/watchdog/stop signals, and polkit CheckAuthorization replacing the UID-0 check. Both are small, well-contained changes. Phase 1 files must be available for end-to-end testing, but Phase 2 can be developed in parallel with Phase 1 using manual installs.
**Delivers:** `sd-notify` crate integration (READY=1, WATCHDOG=1, STOPPING=1), polkit auth wiring in `require_authorized()`, UID-0 fallback for polkit-unavailable environments
**Uses:** sd-notify 0.5.0, zbus (existing)
**Implements:** Architecture patterns #1 (Type=notify) and #3 (Polkit auth)
**Avoids:** Pitfall #1 (ProtectSystem=strict blocks hwmon — test with Phase 1 unit file), Pitfall #3 (polkit not wired in), Pitfall #13 (no auth agent headless — UID-0 fallback)

### Phase 3: GUI Integration & ExecStopPost Fallback
**Rationale:** GUI needs `ALLOW_INTERACTIVE_AUTHORIZATION` flag on write calls (harmless without polkit, essential with it) and CMake install targets for `.desktop` + icons. The `ExecStopPost=` helper is a standalone binary/script that reads persisted fan state and forces safe-max — it's safety-critical and independent from the daemon code.
**Delivers:** GUI DBus flag for interactive auth, CMake install targets, `ExecStopPost=` recovery helper binary/script, improved auth-denied error messages
**Implements:** Architecture pattern #4 (FHS paths for GUI artifacts)
**Avoids:** Pitfall #4 (ExecStopPost not configured — this phase builds the recovery helper), Pitfall #19 (GUI doesn't wait for daemon readiness — StatusMonitor enhancement)

### Phase 4: Packaging (.deb + install.sh)
**Rationale:** Packaging depends on all artifacts from Phases 1–3 being finalized with correct install paths. The `.deb` must stage both Rust and Qt build artifacts; `cargo-deb` can't handle this, so manual `dpkg-deb` is simpler. `install.sh` must mirror the `.deb` file list exactly.
**Delivers:** `fancontrold` `.deb` package, `kde-fan-control` `.deb` package, `install.sh` + `uninstall.sh`, postinst/prerm maintainer scripts
**Avoids:** Pitfall #5 (conffile overwrites config — don't mark daemon config as conffile), Pitfall #6 (DBus policy path — install to `/usr/share/`), Pitfall #7 (no enable in postinst), Pitfall #14 (no daemon-reload), Pitfall #18 (no DBus policy reload)

### Phase Ordering Rationale

- Phase 1 has zero code dependencies — it's pure data files that can be tested by manual installation
- Phase 2 depends on Phase 1 files being installed for end-to-end testing, but can be developed in parallel
- Phase 3 depends on Phase 2's polkit integration for interactive auth to work, but the `ALLOW_INTERACTIVE_AUTHORIZATION` flag itself is harmless without polkit
- Phase 4 depends on Phases 1–3 — all artifacts and their install paths must be finalized before packaging
- The ExecStopPost recovery helper (Phase 3) is safety-critical and should not be deferred — it's the only out-of-process fan safety net

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (polkit wiring):** The exact `CheckAuthorization` call signature with zbus needs verification against the polkit D-Bus API; handling of `ALLOW_INTERACTIVE_AUTHORIZATION` flag from the Qt DBus side needs testing
- **Phase 3 (ExecStopPost):** The recovery helper's behavior under different crash scenarios (SIGKILL, watchdog expiry, OOM) should be validated with real hardware; `ProtectSystem=strict` + `ReadWritePaths` interaction with sysfs may need system-specific testing

Phases with standard patterns (skip research-phase):
- **Phase 1 (static files):** Well-documented formats (systemd.unit, DBus .service, polkit .policy, .desktop); all derived from reference implementations on this system
- **Phase 4 (.deb packaging):** Standard Debian packaging; debhelper + dh-systemd automate most of it

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Only one new crate (sd-notify); all other additions are declarative files with no version risk; reference implementations examined on this system |
| Features | HIGH | Well-understood packaging surface; every feature has a reference implementation (corectrl, thermald, power-profiles-daemon, udisks2); MVP ordering is clear |
| Architecture | HIGH | Six patterns all derived from real system services; data flows validated against existing codebase; dependency graph is linear |
| Pitfalls | HIGH | 20 pitfalls identified from real-world packaging experience and systemd/DBus docs; top 5 have specific mitigation strategies; low-certainty items are minor (sd-notify + Tokio timing) |

**Overall confidence:** HIGH

### Gaps to Address

- **Polkit CheckAuthorization exact API:** The daemon must call `org.freedesktop.PolicyKit1.Authority.CheckAuthorization()` via zbus. The exact struct layout for the `Subject` parameter (unix-process with PID + UID + start-time) needs verification against the polkit D-Bus interface specification. Handle during Phase 2 implementation.
- **sd-notify + Tokio watchdog timing:** The `sd_notify::notify(false, "WATCHDOG=1")` call in the Tokio runtime should be called from the main task, not a spawned task. The exact timing (every 30s when `WatchdogSec=60`) needs real-world validation. Handle during Phase 2 testing.
- **Qt6/Kirigami package names for .deb dependencies:** Package naming varies across Ubuntu/Debian releases (`qml6-module-org-kde-kirigami` vs. `libkirigami6`). Verify at package-build time, not during research.
- **ProtectSystem=strict + sysfs writes on different kernels:** Some `/sys/class/hwmon` writes may resolve through symlinks outside the `ReadWritePaths=` exception on certain kernel/driver combinations. May need fallback to `ProtectSystem=full`. Validate with real hardware during Phase 1 testing.

## Sources

### Primary (HIGH confidence)
- systemd.service(5) man page — service unit configuration, Type=notify, BusName=, ExecStopPost=, watchdog, hardening — https://man7.org/linux/man-pages/man5/systemd.service.5.html
- D-Bus Specification — service activation, SystemdService key, ALLOW_INTERACTIVE_AUTHORIZATION flag — https://dbus.freedesktop.org/doc/dbus-specification.html
- polkit(8) — authorization framework, action declarations, .policy XML format, auth_admin_keep — https://www.freedesktop.org/software/polkit/docs/latest/
- freedesktop Desktop Entry Specification v1.5 — .desktop file format — https://specifications.freedesktop.org/desktop-entry-spec/latest/
- Linux FHS 3.0 — standard paths for binaries, config, data — https://refspecs.linuxfoundation.org/FHS_3.0/
- Linux kernel hwmon sysfs ABI — /sys/class/hwmon interface — https://www.kernel.org/doc/html/latest/hwmon/sysfs-interface.html
- zbus documentation (Context7 `/dbus2/zbus`) — Rust DBus server/client, service activation pitfalls, Tokio integration
- Real system references: power-profiles-daemon, corectrl, thermald, udisks2, ModemManager — unit files, DBus configs, polkit policies examined on this system
- Existing codebase: daemon auth code (`main.rs:789`), GUI DBus interface, existing DBus policy, config paths

### Secondary (MEDIUM confidence)
- Debian Policy Manual / debhelper documentation — .deb packaging conventions, maintainer scripts
- sd-notify crate 0.5.0 — behavior under Tokio async runtime; fdstore feature
- KStatusNotifierItem tray icon behavior with themed icons — needs KDE Plasma verification

### Tertiary (LOW confidence)
- Exact Qt6/Kirigami `.deb` package names — varies by Ubuntu/Debian release; verify at build time
- dpkg `dh_installsystemd` auto-enable behavior — Debian 13+ specifics unverified
- `sd-notify` crate behavior with Tokio watchdog timing — not tested against this specific daemon

---
*Research completed: 2026-04-11*
*Ready for roadmap: yes*