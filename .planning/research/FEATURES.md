# Feature Landscape: Packaging & System Integration

**Domain:** Linux desktop application packaging, systemd integration, polkit authorization, DBus deployment
**Project:** KDE Fan Control — packaging & system integration milestone
**Researched:** 2026-04-11
**Overall confidence:** HIGH

## Executive Take

Packaging and system integration for a Linux daemon+GUI fan-control app is a well-understood problem with clear reference implementations across the freedesktop/DBus/systemd/polkit ecosystem. Projects like UDisks2, ModemManager, NetworkManager, and systemd-logind demonstrate exactly how root D-Bus services, polkit policies, systemd units, .desktop files, and DBus service files should be composed. The feature surface is broad but not deep: each individual piece (unit file, .desktop file, polkit policy, DBus config) has standard patterns, and the main challenge is ensuring they interoperate correctly and are all installed to the right FHS paths by both the .deb and the install.sh fallback.

The existing codebase already has a working DBus bus name (`org.kde.FanControl`), a basic DBus policy config, a `require_authorized` stub that currently checks UID=0, and a CMake install rule for the GUI binary. The packaging work is therefore about **formalizing and completing** these integration points, not designing them from scratch.

---

## Table Stakes

Features users and operators expect from a properly-installed Linux system service with a desktop GUI. Missing these means the product is a development prototype, not a deployable application.

| Feature | Why Expected | Complexity | Dependencies / notes |
|---|---|---|---|
| systemd unit file for the daemon | Any Linux system service that must survive reboots and be supervised needs a proper `.service` file; `fancontrol` and `thinkfan` both ship service units | Low | Requires `ExecStart=` path to be stable, `Type=notify` integration with `sd-notify` in daemon code |
| Daemon enabled at boot (`enable`/`disable`) | Users expect `fancontrol` replacement to start automatically; reboot-persistence is a core value | Low | `WantedBy=` in `[Install]` section; postinst `systemctl enable` |
| `Type=notify` with readiness signaling | The daemon already does discovery + config reconciliation before it is truly ready; `Type=notify` lets systemd wait for that instead of guessing | Low | Requires `sd-notify` crate integration (already in STACK.md); daemon must call `READY=1` after discovery+DBus registration succeed |
| Watchdog integration | Thermal-safety daemon should self-ping so systemd can detect hangs and restart | Low | `WatchdogSec=` in unit; daemon must send `WATCHDOG=1` periodically via `sd-notify` |
| `ExecStopPost=` fallback to safe-max | Crash safety requires an out-of-process helper that can force fans to safe-max even after the daemon is dead; `ExecStopPost=` runs on both clean stop and crash | **Med** | Requires persisting owned-fan set (already in codebase via `PanicFallbackMirror`), plus a small standalone helper binary or script |
| Restart policy (`Restart=on-failure`) | System service reliability: if the daemon crashes, systemd should restart it rather than leave fans unmanaged | Low | One line in the unit file |
| Service hardening directives | Modern Linux packaging expectations include `ProtectHome=`, `ProtectSystem=`, `NoNewPrivileges=`, restricted writable paths — demonstrates security awareness | Low-Med | Must verify hwmon sysfs access still works under hardened settings; some directives (`ProtectSystem=strict`) need `ReadWritePaths=` for config and sysfs |
| .deb package as primary install target | Debian/Ubuntu is the primary target; `.deb` is the native package format; `dpkg -i` and `apt` are what users expect | Med | Requires `debian/` directory skeleton: `control`, `rules`, `postinst`, `postrm`, `install` file lists, maintainer scripts for systemd daemon-reload/enable |
| install.sh as fallback installer | Not all users want .deb; a shell script that copies files to FHS paths and runs `systemctl daemon-reload` + `systemctl enable` is the standard fallback for standalone Linux apps | Low | Must do the same file installs as the .deb; must be idempotent; must handle cleanup on future runs |
| `.desktop` file for the GUI | Desktop environments require `.desktop` files to show the application in launchers, menus, and search; without it the GUI is invisible | Low | Install to `/usr/share/applications/`; must include `Name`, `Comment`, `Exec`, `Icon`, `Categories`, `Type=Application` |
| Embedded/system-tray icon | The tray app needs an icon; the `.desktop` file should reference it; the icon must be installed to the XDG icon theme path | **Med** | SVG or PNG icon installed to `/usr/share/icons/hicolor/`; `.desktop` file `Icon=` key references the icon name; KStatusNotifierItem picks up themed icons automatically |
| CLI in PATH (`/usr/bin/kfc`) | Users expect to type `kfc` at a shell, not `/opt/kde-fan-control/bin/kfc`; PATH availability is table stakes for a CLI tool | Low | Package must install the CLI binary to `/usr/bin/`; `install.sh` must also copy it there |
| DBus service activation file | D-Bus needs a `.service` file in `/usr/share/dbus-1/system-services/` so it can auto-start the daemon when a client connects; this enables "on-demand daemon start" when the GUI launches first | Low | Standard `[D-BUS Service]` format: `Name=`, `Exec=`, `User=root`, `SystemdService=` pointing to the systemd unit |
| DBus policy file installed system-wide | The existing `org.kde.FanControl.conf` must be installed to `/usr/share/dbus-1/system.d/` for dbus-daemon to route messages correctly | Low | File already exists in `packaging/dbus/`; just needs correct install path |
| polkit policy for privileged mutating operations | The daemon currently rejects non-root callers for enroll/apply/override; a proper polkit policy lets authorized desktop users perform these actions via a graphical password prompt instead of having to run as root | **Med** | XML `.policy` file in `/usr/share/polkit-1/actions/`; daemon `require_authorized` must be updated to check polkit instead of UID=0 |
| Standard FHS file layout | Users and package maintainers expect files in standard paths: binary in `/usr/bin/`, config in `/etc/`, data in `/usr/share/`, runtime in `/run/` | Low | FHS paths documented in Debian Policy; mostly about correct `install` targets in packaging |
| On-demand daemon start if GUI launches first | Desktop users will launch the GUI from a menu; if the daemon isn't running, D-Bus activation (via the `.service` file + `SystemdService=` key) should start it automatically | Med | Requires the DBus service activation file to point to `SystemdService=`; systemd must be able to start the service on demand |

## Strong Differentiators Worth Building

Features that go beyond "minimal viable packaging" and materially improve the user experience.

| Feature | Value Proposition | Complexity | Why it is worth early inclusion |
|---|---|---|---|
| `auth_admin_keep` polkit default for config writes | Avoids requiring a password prompt for every single config change; keeps authorization for ~5 minutes | Low | Dramatically better UX for the enrollment wizard and tuning workflows; standard pattern (UDisks2 uses `auth_admin_keep` for mount operations) |
| Granular polkit actions (separate enroll vs. tune vs. override) | Different actions get different auth levels; "view" needs none, "enroll a fan" needs admin, "adjust PID" might only need self | Low-Med | Shows security maturity; aligns with how UDisks2, NetworkManager, and ModemManager separate read/control/admin actions |
| Syslog/journal integration via `LogNamespace=` or `StandardOutput=journal` | Makes daemon logs queryable via `journalctl -u kde-fan-control-daemon` and debuggable without custom log file paths | Low | Already using `tracing`; just need `StandardOutput=journal` + `StandardError=journal` in unit file |
| `dbus-org.kde.FanControl.service` symlink in systemd | Allows DBus auto-activation by bus name — the standard systemd pattern where `dbus-org.<name>.service` is a symlink to the real `.service` file | Low | Already how logind, udisks2, and hostname1 handle this; one-line addition to `postinst` or `.install` |
| systemd hardening with documented exceptions | Shows which hardening directives are applied and which are skipped (with rationale), making the security posture auditable | Low | Sets the project apart from `fancontrol` which has zero hardening; builds user trust |

## Differentiators to Defer

| Feature | Why users like it | Why defer | Complexity |
|---|---|---|---|
| AppStream metadata (`.metainfo.xml`) | Software centers (Discover, GNOME Software) can display the app with screenshots, descriptions, and release notes | Nice-to-have for discoverability; the product is initially distributed via direct .deb, not a repo with AppStream indexing | Low |
| RPM/Arch/COPR packaging | Covers non-Debian distributions | Defer until .deb is proven; distro-specific packaging is a per-distro contribution pattern | Med per distro |
| Automatic updates via repo | Users get updates without manual `.deb` download | Requires APT repository hosting, signing keys, CI pipeline — out of scope for initial packaging | High |
| Sandboxed/Flatpak GUI | Isolates the GUI from the host | The GUI needs DBus system-bus access and Qt/Kirigami native behavior; sandboxing fights the product's purpose | High |
| SELinux/AppArmor profile | Additional mandatory-access-control hardening | Important for enterprise distros but niche for the initial KDE-desktop target; no existing reference profile to start from | Med-High |

## Anti-Features

Things to explicitly NOT build.

| Anti-Feature | Why Avoid | What to Do Instead |
|---|---|---|
| User systemd service for the daemon | The daemon writes sysfs hwmon and must survive logout; a user service runs per-session and lacks the right privilege model | Use a **system** service only — this matches `fancontrol`, `thinkfan`, and every hardware-control daemon in the ecosystem |
| Direct sysfs writes from the GUI or CLI | Breaks privilege boundaries; bypasses the daemon's safety/validation layer and polkit | All writes go through the daemon via D-Bus; the GUI and CLI are unprivileged clients |
| Bundled/config-file-based polkit rules | polkit documentation explicitly says applications must **never** install authorization rules (`.rules` files) — only administrators and special-purpose OS environments should | Install a `.policy` file (action declarations) only; let system administrators write their own `.rules` if they want to override the defaults |
| D-Bus session bus for the daemon | The daemon is a system service that controls hardware; session bus is per-login-session and wrong privilege/lifecycle model | Use the **system bus** exclusively; session bus flag (`--session-bus`) can remain for local development testing only |
| Config file under `/opt/` or `~/.config/` for the daemon | Wrong FHS location for a system service's owned config | Daemon config goes in `/etc/kde-fan-control/` — the standard location for system service configuration |
| Exec path in the DBus `.service` activation file that points to the real binary | ModemManager demonstrates the correct pattern: `Exec=/usr/bin/false` in the D-Bus service file with `SystemdService=` pointing to the systemd unit. This prevents auto-start bypassing systemd supervision. | Use `SystemdService=` in the DBus `.service` file and let systemd handle actual process execution |
| `Type=simple` in the systemd unit | The daemon does real work (hwmon discovery, config reconciliation, DBus registration) before it is usable; `Type=simple` would report ready too early, letting GUI/CLI connect to a not-yet-functional daemon | Use `Type=notify` and explicitly send `READY=1` only after discovery + DBus registration succeed |

## Feature Dependencies

```text
FHS file layout
  → CLI installed to /usr/bin/
  → Daemon binary installed to /usr/sbin/
  → GUI binary installed to /usr/bin/
  → Config dir at /etc/kde-fan-control/
  → Icon installed to /usr/share/icons/
  → .desktop file installed to /usr/share/applications/

systemd unit file
  → Type=notify requires sd-notify integration in daemon code
  → ExecStopPost= requires fallback helper binary/script
  → WantedBy= requires postinst enable step
  → Hardening requires ReadWritePaths= for sysfs and config
  → WatchdogSec= requires periodic WATCHDOG=1 pings in daemon code

DBus policy file
  → Must be installed to /usr/share/dbus-1/system.d/
  → Existing file in packaging/dbus/ needs correct path

DBus service activation file
  → SystemdService= key links to the systemd unit name
  → Enables on-demand daemon start when GUI connects first

polkit policy
  → Daemon require_authorized must be updated from UID=0 to polkit CheckAuthorization
  → Action IDs must match the daemon's method-level authorization checks
  → .policy file installed to /usr/share/polkit-1/actions/

.desktop file
  → Exec= points to GUI binary path
  → Icon= references themed icon name
  → Categories= for KDE/Qt system utility

.deb package
  → Depends on: systemd, dbus, polkitd, libqt6, kirigami runtime
  → postinst: daemon-reload, enable, start (if not interfering)
  → postrm: disable, stop, daemon-reload

install.sh
  → Does same file copies as .deb, plus systemctl calls
  → Must be idempotent for re-runs

On-demand daemon start
  → DBus service activation file with SystemdService= key
  → systemd dbus-org.kde.FanControl.service symlink
```

## Detail: Each Feature Category

### 1. systemd Unit File

**File:** `packaging/systemd/kde-fan-control-daemon.service`

Key decisions based on systemd.service(5) and reference implementations (systemd-logind, udisks2):

| Setting | Value | Rationale |
|---|---|---|
| `Type` | `notify` | Daemon does discovery+reconciliation before ready; lets systemd wait correctly |
| `NotifyAccess` | `main` | Only the main process sends readiness/watchdog notifications |
| `BusName` | `org.kde.FanControl` | Recommended even for `Type=notify` — helps `systemctl` map service to D-Bus name |
| `ExecStart` | `/usr/sbin/kde-fan-control-daemon` | Standard FHS path for system daemon binaries |
| `ExecStopPost` | `/usr/sbin/kde-fan-control-fallback` | Out-of-process fallback helper that reads persisted owned-fan list and forces PWM to safe-max; runs on both clean stop and crash |
| `Restart` | `on-failure` | Standard for long-running system services; restart on crash/timeout/watchdog |
| `WatchdogSec` | `30s` (initial) | Conservative; daemon must ping within this window |
| `TimeoutStartSec` | `60s` | Allows time for hwmon discovery on slow boots |
| `WantedBy` | `graphical.target` | Appropriate for a desktop hardware-control daemon; ensures it starts when the desktop is expected |
| `ReadWritePaths` | `/sys/class/hwmon /etc/kde-fan-control /run/kde-fan-control` | Required under `ProtectSystem=strict` for sysfs writes and persistence |
| `ProtectHome` | `yes` | Daemon has no business reading home directories |
| `ProtectSystem` | `strict` | Only write to explicitly allowed paths |
| `NoNewPrivileges` | `yes` | Daemon already runs as root; no reason to escalate further |
| `StandardOutput` / `StandardError` | `journal` | Redirects `tracing` output to the journal for `journalctl -u` queries |

**Confidence:** HIGH — all settings are derived from systemd.service(5) and verified against reference system services.

### 2. .deb Package

**Directory:** `packaging/deb/`

Standard Debian package structure:

```
packaging/deb/
  debian/
    control           — package metadata, dependencies
    rules             — build instructions (Makefile style)
    compat            — debhelper compatibility level (13+)
    changelog         — version history
    install           — file install paths
    postinst          — systemd daemon-reload, enable, conditional start
    postrm            — systemd disable, stop, daemon-reload
    source/format     — "3.0 (native)" or "3.0 (quilt)"
```

Key dependency declarations:

| Depends | Rationale |
|---|---|
| `systemd` | Service management, daemon-reload, socket activation |
| `dbus` | System bus daemon required for IPC |
| `polkitd` | PolicyKit daemon for authorization prompts |
| `libqt6core6` | Qt6 runtime for the GUI |
| `libkirigami6` (or `qml6-module-org-kde-kirigami`) | Kirigami QML module at runtime |
| `libkf6statusnotifieritem6` | Tray integration |
| `libkf6notifications6` | Desktop notifications |

The `postinst` script must:
1. `systemctl daemon-reload` (after installing/changing unit files)
2. `systemctl enable kde-fan-control-daemon.service` (boot persistence)
3. `deb-systemd-invoke start kde-fan-control-daemon.service` (start after install, respecting policy)

**Confidence:** HIGH — standard Debian packaging patterns; `debhelper` and `dh-systemd` automate most of this.

### 3. .desktop File for GUI

**File:** `packaging/desktop/org.kde.FanControl.desktop`

Per the freedesktop Desktop Entry Specification v1.5:

| Key | Value | Notes |
|---|---|---|
| `Type` | `Application` | Standard for desktop apps |
| `Name` | `KDE Fan Control` | Display name |
| `GenericName` | `Fan Speed Control` | Category description |
| `Comment` | `Manage desktop fan speeds with per-fan PID control` | Tooltip |
| `Exec` | `/usr/bin/kde-fan-control-gui` | GUI binary path |
| `Icon` | `kde-fan-control` | Icon theme name (installed as SVG to hicolor) |
| `Categories` | `System;Qt;KDE` | System utility + KDE/Qt tags |
| `Keywords` | `fan;cooling;temperature;PWM;hwmon` | Search terms |
| `Terminal` | `false` | Not a terminal app |
| `StartupNotify` | `true` | Supports startup notification |
| `SingleMainWindow` | `true` | One main window pattern |

**Install path:** `/usr/share/applications/org.kde.FanControl.desktop`

**Icon install path:** `/usr/share/icons/hicolor/scalable/apps/kde-fan-control.svg`

**Confidence:** HIGH — straightforward .desktop file; verified against spec and KDE app examples.

### 4. polkit Policy

**File:** `packaging/polkit/org.kde.FanControl.policy`

Per polkit(8) documentation, the XML `.policy` file declares actions and their default authorizations. Installed to `/usr/share/polkit-1/actions/`.

Recommended action structure:

| Action ID | Description | `allow_active` | `allow_any` | `allow_inactive` |
|---|---|---|---|---|
| `org.kde.FanControl.enroll-fan` | Enroll or unenroll a fan from daemon control | `auth_admin_keep` | `auth_admin` | `auth_admin` |
| `org.kde.FanControl.apply-config` | Apply a draft configuration to the running daemon | `auth_admin_keep` | `auth_admin` | `auth_admin` |
| `org.kde.FanControl.write-config` | Modify daemon configuration (tune PID, set names, control profiles) | `auth_admin_keep` | `auth_admin` | `auth_admin` |
| `org.kde.FanControl.start-auto-tune` | Start an auto-tuning session for a managed fan | `auth_admin_keep` | `auth_admin` | `auth_admin` |
| `org.kde.FanControl.manage-daemon` | Start, stop, or restart the fan-control daemon | `auth_admin` | `auth_admin` | `auth_admin` |

Key design decisions:
- **`auth_admin_keep`** for the primary interactive operations (enroll, apply, tune) — avoids repeated password prompts within a 5-minute window.
- **`auth_admin`** (no keep) for daemon lifecycle management — more conservative since start/stop is a less frequent action and has broader consequences.
- Read-only operations (inventory snapshot, telemetry, runtime state) need NO polkit action — they are already open to all local users in the existing DBus policy.

The **daemon code must be updated** to replace the current `require_authorized(connection, header)` UID=0 check with a polkit `CheckAuthorization` call using `zbus` to contact the polkit authority on the system bus. This is a moderate engineering change that touches every mutating D-Bus method.

**Confidence:** HIGH — polkit architecture is well-documented; action structure follows UDisks2/ModemManager patterns.

### 5. DBus Service Activation & Policy Installation

**Service activation file:** `packaging/dbus/org.kde.FanControl.service`

Format (per DBus spec and ModemManager reference):

```ini
[D-BUS Service]
Name=org.kde.FanControl
Exec=/usr/bin/false
User=root
SystemdService=kde-fan-control-daemon.service
```

The `Exec=/usr/bin/false` pattern is deliberate — it prevents D-Bus from launching the daemon directly (bypassing systemd supervision). Instead, the `SystemdService=` key tells dbus-daemon to ask systemd to start the unit, which ensures the daemon runs under systemd's supervision (restart policy, watchdog, cgroups, etc.).

**Install paths:**
- Service activation: `/usr/share/dbus-1/system-services/org.kde.FanControl.service`
- Bus policy: `/usr/share/dbus-1/system.d/org.kde.FanControl.conf` (file already exists)

**DBus bus policy** (existing file `packaging/dbus/org.kde.FanControl.conf`): the current policy allows all local users to send to the daemon's destination. This is correct for read operations, but once polkit is integrated, the policy should be tightened to **deny** write-type method calls by default and only **allow** them when explicitly permitted. This mirrors the ModemManager pattern where `deny send_type="method_call"` is the default and specific methods are individually allowed. However, this tightening can happen incrementally — the current permissive policy works correctly when polkit is the primary gate, since the daemon itself enforces authorization.

**For on-demand daemon start:** the `dbus-org.kde.FanControl.service` symlink should be created in `/usr/lib/systemd/system/` pointing to `kde-fan-control-daemon.service`. This is the standard pattern: systemd watches for D-Bus bus name acquisition and the symlink name tells dbus-daemon which systemd unit corresponds to the bus name.

**Confidence:** HIGH — well-established patterns from ModemManager, udisks2, and systemd-logind.

### 6. CLI in PATH

The CLI binary (`kde-fan-control-cli`) should be installed to `/usr/bin/kfc` or `/usr/bin/kde-fan-control-cli` with a symlink at `/usr/bin/kfc` for convenience. This is standard for system utilities.

**Confidence:** HIGH.

### 7. Standard FHS File Layout

| Artifact | Install Path | Rationale |
|---|---|---|
| Daemon binary | `/usr/sbin/kde-fan-control-daemon` | System daemon binaries go in sbin |
| Fallback helper | `/usr/sbin/kde-fan-control-fallback` | Also privileged system binary |
| GUI binary | `/usr/bin/kde-fan-control-gui` | User-facing binaries go in bin |
| CLI binary | `/usr/bin/kde-fan-control-cli` | User-facing binaries go in bin |
| CLI symlink | `/usr/bin/kfc` → `kde-fan-control-cli` | Short convenient name |
| Daemon config | `/etc/kde-fan-control/config.toml` | System service configuration in /etc |
| Systemd service | `/usr/lib/systemd/system/kde-fan-control-daemon.service` | Standard systemd unit path |
| Systemd alias | `/usr/lib/systemd/system/dbus-org.kde.FanControl.service` → `kde-fan-control-daemon.service` | Enables D-Bus activation via systemd |
| DBus policy | `/usr/share/dbus-1/system.d/org.kde.FanControl.conf` | Standard dbus-daemon config path |
| DBus service | `/usr/share/dbus-1/system-services/org.kde.FanControl.service` | D-Bus service activation path |
| polkit policy | `/usr/share/polkit-1/actions/org.kde.FanControl.policy` | Standard polkit actions path |
| .desktop file | `/usr/share/applications/org.kde.FanControl.desktop` | Standard desktop entry path |
| App icon (SVG) | `/usr/share/icons/hicolor/scalable/apps/kde-fan-control.svg` | Icon theme path for scalable icon |
| Notification config | `/usr/share/knotifications5/kdefancontrol.notifyrc` | Already in CMakeLists.txt install rule |
| Runtime state | `/run/kde-fan-control/` | Created by daemon via `RuntimeDirectory=` in unit |

**Confidence:** HIGH — FHS paths are well-specified and match existing system service conventions.

### 8. On-Demand Daemon Start

When the GUI launches and the daemon is not yet running, two mechanisms can trigger daemon startup:

1. **D-Bus auto-activation** (primary): The GUI opens a D-Bus connection to `org.kde.FanControl`. D-Bus daemon checks `/usr/share/dbus-1/system-services/org.kde.FanControl.service`, sees `SystemdService=kde-fan-control-daemon.service`, and asks systemd to start that unit. The daemon starts, acquires the bus name, and the GUI's pending connection resolves.

2. **`Type=dbus` / `BusName=` in systemd unit**: With `BusName=org.kde.FanControl` in the service file, systemd knows this service provides a D-Bus name and will start it when something requests that name (if the `dbus-org.kde.FanControl.service` alias symlink exists).

Both mechanisms require:
- The DBus `.service` file with `SystemdService=` key
- The `dbus-org.kde.FanControl.service` symlink in `/usr/lib/systemd/system/`
- The GUI should handle "daemon not available" gracefully during the startup window (it already has `StatusMonitor::checkDaemonConnected`, but should also show a loading/waiting state)

**Confidence:** HIGH — standard D-Bus + systemd activation flow.

### 9. install.sh Fallback

A single script that:
1. Copies binaries to FHS paths (same as .deb)
2. Copies config files, DBus policy, polkit policy, .desktop file, icon
3. Creates the dbus-org symlink
4. Runs `systemctl daemon-reload`
5. Runs `systemctl enable kde-fan-control-daemon.service`
6. Optionally runs `systemctl start kde-fan-control-daemon.service`
7. Updates icon cache (`gtk-update-icon-cache` / `xdg-icon-resource`)
8. Updates desktop database (`update-desktop-database`)

Must be idempotent (safe to re-run) and must require root (check `EUID` at the top).

**Confidence:** HIGH — straightforward shell script.

## Feature Dependencies (Full Graph)

```text
Standard FHS file layout
  ├── all installed files depend on correct paths
  └── both .deb and install.sh must use same paths

systemd unit file
  ├── sd-notify integration in daemon code (Type=notify)
  ├── sd-notify watchdog pings (WatchdogSec=)
  ├── fallback helper binary (ExecStopPost=)
  └── postinst: daemon-reload + enable + start

polkit policy
  ├── .policy file installed to /usr/share/polkit-1/actions/
  ├── daemon code: replace require_authorized with CheckAuthorization
  ├── daemon code: map D-Bus method calls to polkit action IDs
  └── DBus policy: tightening around write methods (optional, can be deferred)

DBus service activation
  ├── .service file with SystemdService= key
  ├── dbus-org.kde.FanControl.service symlink
  └── enables on-demand daemon start when GUI connects

.desktop file + icon
  ├── .desktop file installed to /usr/share/applications/
  ├── SVG icon installed to /usr/share/icons/hicolor/
  └── icon cache / desktop database updates

.deb package
  ├── includes all above artifacts
  ├── postinst script: daemon-reload, enable, start
  ├── postrm script: disable, stop, daemon-reload
  └── dependency declarations for systemd, dbus, polkitd, Qt6, Kirigami

install.sh
  └── mirrors .deb behavior via shell script
```

## MVP Recommendation

**Prioritize in this order:**

1. **Standard FHS layout + file installs** — the foundation; every other feature depends on files being in the right place
2. **systemd unit file with `Type=notify`** — boot persistence and service supervision; the single most important packaging artifact
3. **DBus service activation** — enables "GUI first, daemon auto-starts" workflow
4. **FHS-installed DBus policy** — existing file, just needs correct install path
5. **.desktop file + icon** — makes the GUI discoverable in the desktop environment
6. **CLI in PATH** — trivial but important for usability
7. **polkit policy** — replaces the hard UID=0 check with a proper authorization flow; medium complexity but high UX impact
8. **.deb package** — formalizes everything into a proper distributable
9. **install.sh** — fallback for non-Debian or manual install

The polkit integration (step 7) is the only moderate-complexity item because it requires daemon code changes. Everything else is packaging and file installation.

## Explicitly Deferred

- AppStream metadata (.metainfo.xml)
- RPM, Arch, COPR packages
- APT repository hosting and signing
- Flatpak/sandboxed GUI
- SELinux/AppArmor profiles
- Tightened DBus method-level policy (current permissive policy is safe with daemon-side polkit checks)

## Sources

### HIGH confidence
- systemd.service(5) manual page — service unit configuration, Type=notify, BusName=, ExecStopPost=, watchdog, hardening directives. https://man7.org/linux/man-pages/man5/systemd.service.5.html
- D-Bus specification — service activation, bus name ownership, message routing. https://dbus.freedesktop.org/doc/dbus-specification.html
- polkit(8) — authorization framework, action declarations, .policy XML format, defaults, auth_admin_keep. https://polkit.pages.freedesktop.org/polkit/polkit.8.html
- freedesktop Desktop Entry Specification v1.5 — .desktop file format, recognized keys, D-Bus activation. https://specifications.freedesktop.org/desktop-entry-spec/latest/
- Linux FHS (Filesystem Hierarchy Standard) — standard paths for binaries, config, data. https://refspecs.linuxfoundation.org/fhs.shtml
- Reference implementations: udisks2.service, ModemManager1, systemd-logind — real-world examples of system bus services with polkit, DBus activation, and systemd integration. /usr/lib/systemd/system/ and /usr/share/dbus-1/

### MEDIUM confidence
- Debian Policy Manual and debhelper documentation — .deb packaging conventions, maintainer scripts. https://www.debian.org/doc/debian-policy/
- KStatusNotifierItem tray icon behavior with themed icons — the tray picks up icons from the icon theme, but exact SVG naming/raster fallback requirements need verification with KDE Plasma. Based on existing KDE app patterns.

### LOW confidence
- Exact Qt6/Kirigami package names across Ubuntu/Debian releases — package naming varies; `qml6-module-org-kde-kirigami` vs. `libkirigami6` needs verification at package-build time
- Exact `debhelper`/`dh-systemd` automation level — Debian 13+ may have improved systemd integration in debhelper; specifics should be verified during implementation