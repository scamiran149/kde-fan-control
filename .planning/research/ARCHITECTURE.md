# Architecture Research: Packaging & System Integration

**Domain:** Linux desktop system service packaging (systemd, polkit, DBus, .deb, .desktop)
**Researched:** 2026-04-11
**Confidence:** HIGH

## Standard Architecture

### System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                     System Integration Layer                     │
├──────────────┬──────────────┬──────────────┬─────────────────────┤
│  systemd     │  DBus system │  polkit      │  FHS install paths  │
│  unit file   │  bus policy  │  .policy     │  /usr/bin,          │
│  (Type=notify│  + service   │  (auth for   │  /usr/lib/systemd,  │
│   + watchdog)|  activation  │  privileged  │  /usr/share/dbus-1, │
│              │  .service    │  writes)     │  /usr/share/polkit-1│
├──────────────┴──────────────┴──────────────┴─────────────────────┤
│                     Installed Artifacts                          │
├──────────────┬──────────────┬──────────────┬─────────────────────┤
│  Rust daemon │  Rust CLI    │  Qt6/Kirigami│  Static data files  │
│  binary      │  binary     │  GUI binary   │  (unit, policy,     │
│  → /usr/bin/ │  → /usr/bin/│  → /usr/bin/ │   .desktop, dbus)   │
├──────────────┴──────────────┴──────────────┴─────────────────────┤
│                     Build & Package Layer                        │
├──────────────┬──────────────┬──────────────┬─────────────────────┤
│  cargo build │  cmake build │  dpkg-deb /  │  install.sh         │
│  (daemon,cli)|  (GUI)       │  .deb package│  (fallback)         │
└──────────────┴──────────────┴──────────────┴─────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| systemd unit file | Daemon lifecycle, boot startup, readiness notification, watchdog | `.service` file in `/usr/lib/systemd/system/` |
| DBus system bus policy | ACL for who can own/call the `org.kde.FanControl` name | `.conf` file in `/usr/share/dbus-1/system.d/` |
| DBus service activation file | Maps bus name → systemd unit for on-demand start | `.service` file in `/usr/share/dbus-1/system-services/` |
| polkit `.policy` file | Defines privileged actions with auth prompts for unprivileged callers | XML policy in `/usr/share/polkit-1/actions/` |
| `.desktop` file | Desktop entry for GUI app launch + tray icon | `.desktop` file in `/usr/share/applications/` |
| `.deb` package | Primary install target with all artifacts + postinst/prerm hooks | `dpkg-deb` built from staged FHS tree |
| `install.sh` | Fallback installer for non-deb systems | Shell script that copies files + enables systemd unit |

## Recommended Project Structure

```
packaging/
├── dbus/
│   ├── org.kde.FanControl.conf          # DBus system bus policy (EXISTS)
│   └── org.kde.FanControl.service       # DBus service activation file (NEW)
├── systemd/
│   └── kde-fan-control-daemon.service   # systemd unit file (NEW)
├── polkit/
│   └── org.kde.fancontrol.policy        # polkit policy (NEW)
├── desktop/
│   └── org.kde.fancontrol.gui.desktop    # .desktop file for GUI (NEW)
├── icons/
│   └── hicolor/                         # App icon in SVG + standard PNG sizes (NEW)
├── debian/
│   ├── control                          # deb package metadata (NEW)
│   ├── rules                            # deb build rules (NEW)
│   ├── postinst                          # post-install script (NEW)
│   ├── prerm                            # pre-remove script (NEW)
│   └── changelog                        # deb changelog (NEW)
└── install.sh                           # fallback installer (NEW)
```

### Structure Rationale

- **`packaging/`** already exists in the repo with a `dbus/` subdirectory; extend it rather than scatter files
- **`packaging/debian/`** is the standard deb packaging layout; this is where `dpkg-buildpackage` expects metadata
- **`packaging/systemd/`**, **`packaging/polkit/`**, **`packaging/desktop/`** group static config files by the subsystem they target
- **`packaging/icons/`** provides the hicolor icon directory structure that install will copy into `/usr/share/icons/hicolor/`
- **`install.sh`** at the packaging root is the standalone fallback — users who aren't on a deb-based distro run this directly

## Architectural Patterns

### Pattern 1: Systemd Type=notify with Deferred Readiness

**What:** The daemon signals `READY=1` to systemd only after hwmon discovery, config load, boot reconciliation, AND DBus name acquisition all succeed. Systemd waits until this explicit notification before considering the service "started" and proceeding with dependent units.

**When:** Any long-running system service with non-trivial startup that must be communication-ready before dependents start.

**Trade-offs:**
- Pro: Boot ordering is precise — no guessing about readiness
- Pro: Failed startup (missing hardware, bad config) is reported immediately rather than silently
- Con: The daemon must integrate `sd_notify` calls, adding a small code dependency

**Example:**
```rust
// In daemon main.rs, AFTER DBus name acquisition and boot reconciliation:
#[cfg(target_os = "linux")]
if sd_notify::notify(true, &[sd_notify::SdNotify::Ready]).is_err() {
    tracing::warn!("sd_notify READY=1 failed — not running under systemd?");
}
```

The `sd-notify` crate (already in the STACK recommendation) handles the `$NOTIFY_SOCKET` protocol. The call is a no-op when the socket is absent, so the same binary works for `install.sh` or manual invocation.

### Pattern 2: DBus Service Activation → Systemd

**What:** A DBus `.service` file tells dbus-daemon that `org.kde.FanControl` is provided by the `kde-fan-control-daemon.service` systemd unit. When a client (GUI or CLI) calls a method on that bus name, dbus-daemon asks systemd to start the unit if it isn't running. This is "on-demand daemon start."

**When:** System services exposed via DBus where clients should not need to manually start the daemon first.

**Trade-offs:**
- Pro: GUI can simply start, try to reach the daemon, and if the daemon isn't running dbus-daemon will auto-start it
- Pro: The daemon can be stopped when idle (if desired later)
- Con: Adds a dependency on correct systemd ↔ dbus integration (standard on modern Linux but worth noting)

**Example:**
```ini
# /usr/share/dbus-1/system-services/org.kde.FanControl.service
[D-BUS Service]
Name=org.kde.FanControl
SystemdService=kde-fan-control-daemon.service
```

Key detail: the `SystemdService=` key tells dbus-daemon to use systemd activation rather than `Exec=`. The `Type=dbus` systemd unit approach is an alternative, but `Type=notify` with `SystemdService=` is superior because readiness is explicit (not just "name is on the bus").

### Pattern 3: Polkit Authorization Replacing UID-0 Check

**What:** Replace the current `require_authorized()` function (which checks UID == 0) with a polkit check that allows unprivileged users to perform privileged operations after authentication. The polkit policy defines actions; the daemon checks them via `org.freedesktop.PolicyKit1` DBus calls.

**When:** Any system service where unprivileged desktop users need to perform privileged operations (config writes, fan enrollment) without running the client as root.

**Trade-offs:**
- Pro: GUI and CLI don't need `sudo` — users get an authentication prompt naturally
- Pro: Follows standard Linux desktop privilege escalation convention
- Pro: Admins can configure fine-grained auth rules (allow specific users/groups)
- Con: Adds a runtime dependency on polkitd (present on virtually all desktop Linux systems)
- Con: The daemon must add a polkit DBus proxy dependency; implementation must handle polkit-unavailable fallback

**Example polkit policy:**
```xml
<!-- /usr/share/polkit-1/actions/org.kde.fancontrol.policy -->
<policyconfig>
  <action id="org.kde.fancontrol.write-config">
    <description>Modify fan control configuration</description>
    <message>Authentication is required to modify fan control settings</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>
</policyconfig>
```

**Example daemon-side check:**
```rust
// Updated require_authorized in daemon — checks polkit, falls back to UID-0:
async fn require_authorized(
    connection: &zbus::Connection,
    header: &zbus::message::Header<'_>,
) -> fdo::Result<()> {
    let sender = header.sender()
        .ok_or_else(|| fdo::Error::AccessDenied("no sender".into()))?;

    // Resolve caller identity for subject
    let dbus_proxy = fdo::DBusProxy::new(connection).await
        .map_err(|e| fdo::Error::AccessDenied(format!("dbus proxy error: {e}")))?;
    let uid: u32 = dbus_proxy.get_connection_unix_user(sender.clone().into()).await
        .map_err(|e| fdo::Error::AccessDenied(format!("identity resolution: {e}")))?;

    // Try polkit authorization first
    if let Ok(authority) = check_polkit_authorization(connection, &sender, uid).await {
        return Ok(());
    }

    // Fallback: require UID 0 if polkit is unavailable
    if uid != 0 {
        tracing::warn!(caller_uid = uid, "unauthorized write attempt (polkit unavailable, requiring root)");
        return Err(fdo::Error::AccessDenied(
            "privileged operations require root access (polkit unavailable)".into(),
        ));
    }
    Ok(())
}
```

Note: The `ALLOW_INTERACTIVE_AUTHORIZATION` DBus flag (0x4) should be set on write method calls from the GUI, so polkit can show graphical auth prompts. The CLI already prints a helpful message when AccessDenied occurs.

### Pattern 4: FHS-Standard Install Paths

**What:** All installed files follow the Linux FHS standard with well-defined paths. The packaging layer maps source files to their install destinations.

**When:** Any Linux system service.

**Install path mapping:**

| Source | Install Destination | Purpose |
|--------|-------------------|---------|
| `target/release/kde-fan-control-daemon` | `/usr/bin/kde-fan-control-daemon` | Daemon binary |
| `target/release/kde-fan-control` | `/usr/bin/kde-fan-control` | CLI binary |
| `gui/build/gui_app` | `/usr/bin/kde-fan-control-gui` | GUI binary |
| `packaging/systemd/kde-fan-control-daemon.service` | `/usr/lib/systemd/system/kde-fan-control-daemon.service` | Systemd unit |
| `packaging/dbus/org.kde.FanControl.conf` | `/usr/share/dbus-1/system.d/org.kde.FanControl.conf` | DBus policy |
| `packaging/dbus/org.kde.FanControl.service` | `/usr/share/dbus-1/system-services/org.kde.FanControl.service` | DBus activation |
| `packaging/polkit/org.kde.fancontrol.policy` | `/usr/share/polkit-1/actions/org.kde.fancontrol.policy` | Polkit policy |
| `packaging/desktop/org.kde.fancontrol.gui.desktop` | `/usr/share/applications/org.kde.fancontrol.gui.desktop` | Desktop entry |
| GUI icon assets (SVG + PNGs) | `/usr/share/icons/hicolor/{scalable,48x48,...}/apps/` | App/tray icon |
| `gui/data/kdefancontrol.notifyrc` | `/usr/share/knotifications5/kdefancontrol.notifyrc` | Notification config (already in CMakeLists.txt) |

**Rationale:**
- `/usr/bin/` for executables — standard PATH location
- `/usr/lib/systemd/system/` for system unit files — where systemctl looks
- `/usr/share/dbus-1/system.d/` for DBus policy — where dbus-daemon loads per-service policy
- `/usr/share/dbus-1/system-services/` for activation — where dbus-daemon finds service files
- `/usr/share/polkit-1/actions/` for polkit policy — where polkitd loads action definitions
- `/usr/share/applications/` for .desktop — where desktop environments find app entries
- `/usr/share/icons/hicolor/` — standard freedesktop icon theme path

### Pattern 5: Dual Packaging Path (.deb primary + install.sh fallback)

**What:** The `.deb` package is the primary install target, built by staging files into a FHS tree and running `dpkg-deb`. For non-deb distros, a self-contained `install.sh` does the same file copies and systemd enablement manually.

**When:** Projects that target Debian/Ubuntu as primary but want to support other distros.

**Trade-offs:**
- Pro: `.deb` gets proper dependency declarations, uninstall, upgrade behavior
- Pro: `install.sh` is a safety net — works on Arch, Fedora, etc.
- Con: Two paths to test and keep in sync
- Con: `install.sh` must handle edge cases that dpkg handles natively

**Example build flow:**
```
1. cargo build --release          → produces daemon + CLI binaries
2. cd gui && cmake .. && make     → produces GUI binary
3. Stage files into packaging/dist/ following FHS layout
4a. dpkg-deb --build packaging/dist/ kde-fan-control_0.1.0_amd64.deb
4b. install.sh copies from packaging/dist/ to live filesystem
```

### Pattern 6: Single Polkit Action for All Privileged Writes

**What:** Define one polkit action (`org.kde.fancontrol.write-config`) that covers all privileged daemon write operations: enrollment changes, draft apply, control-profile mutations, auto-tune actions. The authorization boundary is "can you change what the daemon does?" — a single action captures this cleanly.

**When:** A system service where all privileged operations share the same trust level.

**Trade-offs:**
- Pro: Simple — users authenticate once and get access to all write operations
- Pro: Aligns with the current architecture where `require_authorized()` is called from every write method
- Con: Less granular than separate actions per operation (but nobody needs "can enroll fans but not apply config" as a separate permission)
- Con: If granular control is needed later, the policy can be extended with additional action IDs

## Data Flow

### Daemon Lifecycle Under Systemd

```
[systemd]
    │
    ├── boot or manual start ──→ ExecStart=/usr/bin/kde-fan-control-daemon
    │                                    │
    │                                    ├── discover hwmon devices
    │                                    ├── load config from /var/lib/kde-fan-control/config.toml
    │                                    ├── boot reconciliation (restore managed fans)
    │                                    ├── register DBus name (org.kde.FanControl)
    │                                    ├── sd_notify READY=1  ←── systemd considers service "active"
    │                                    │
    │                                    ├── (run control loops, serve DBus requests)
    │                                    │
    │                                    ├── periodic: sd_notify WATCHDOG=1 (if WatchdogSec= set)
    │                                    │
    │                                    ├── SIGTERM / SIGINT received
    │                                    │       │
    │                                    │       ├── stop control loops
    │                                    │       ├── write fallback (pwm=255 for owned fans)
    │                                    │       ├── persist fallback incident
    │                                    │       ├── sd_notify STOPPING=1
    │                                    │       └── exit 0
    │                                    │
    │                                    └── (if watchdog expires) → systemd sends SIGABRT → panic fallback hook fires
    │
    ├── on-failure ──→ Restart=on-failure  (auto-restart after failure)
    │
    └── WantedBy=multi-user.target  (boot-enabled)
```

### On-Demand Daemon Start (GUI → DBus Activation Flow)

```
[User launches GUI]
    │
    ├── GUI starts, creates DaemonInterface (QDBusInterface to org.kde.FanControl)
    │       │
    │       ├── DBus call goes to dbus-daemon
    │       │       │
    │       │       ├── Bus name "org.kde.FanControl" not owned?
    │       │       │       │
    │       │       │       └── dbus-daemon reads /usr/share/dbus-1/system-services/org.kde.FanControl.service
    │       │       │               │
    │       │       │               └── SystemdService=kde-fan-control-daemon.service
    │       │       │                       │
    │       │       │                       └── dbus-daemon tells systemd to start the unit
    │       │       │                               │
    │       │       │                               └── systemd starts daemon, waits for READY=1
    │       │       │                                       │
    │       │       │                                       └── DBus name acquired → call delivered
    │       │       │
    │       │       └── Bus name already owned → call delivered directly
    │       │
    │       └── StatusMonitor::checkDaemonConnected() → reads daemon state
    │
    └── GUI renders inventory, fan status, etc.
```

### Polkit Authorization Flow (Unprivileged User Writes Config)

```
[User clicks "Apply" in GUI]
    │
    ├── DaemonInterface.applyDraft() → DBus method call on system bus
    │       │                                        (with ALLOW_INTERACTIVE_AUTHORIZATION flag)
    │       │
    │       ├── write method enters require_authorized()
    │       │       │
    │       │       ├── Extract sender bus name from message header
    │       │       │
    │       │       ├── Call org.freedesktop.PolicyKit1 Authority.CheckAuthorization
    │       │       │       │
    │       │       │       ├── Action ID: org.kde.fancontrol.write-config
    │       │       │       │
    │       │       │       ├── Polkit checks:
    │       │       │       │   ├── Is user in an allowed group? (admin can configure)
    │       │       │       │   └── Default: auth_admin_keep (prompt once, remember briefly)
    │       │       │       │
    │       │       │       ├── If not yet authorized: polkit shows auth dialog
    │       │       │       │   └── User enters password → polkit grants temporary auth
    │       │       │       │
    │       │       │       └── Returns: is_authorized = true/false
    │       │       │
    │       │       ├── is_authorized? → proceed with write
    │       │       └── not authorized? → return fdo::Error::AccessDenied
    │       │
    │       └── Write succeeds → emit signals → GUI updates
    │
    └── Error: AccessDenied → GUI shows "authentication required" message
```

### Package Install → System Integration Flow

```
[dpkg -i kde-fan-control_0.1.0_amd64.deb]
    │
    ├── Unpack files to FHS paths
    │   ├── /usr/bin/kde-fan-control-daemon
    │   ├── /usr/bin/kde-fan-control
    │   ├── /usr/bin/kde-fan-control-gui
    │   ├── /usr/lib/systemd/system/kde-fan-control-daemon.service
    │   ├── /usr/share/dbus-1/system.d/org.kde.FanControl.conf
    │   ├── /usr/share/dbus-1/system-services/org.kde.FanControl.service
    │   ├── /usr/share/polkit-1/actions/org.kde.fancontrol.policy
    │   ├── /usr/share/applications/org.kde.fancontrol.gui.desktop
    │   └── /usr/share/icons/hicolor/... (icons)
    │
    ├── postinst runs:
    │   ├── systemctl daemon-reload           (pick up new unit file)
    │   ├── systemctl enable kde-fan-control-daemon.service  (boot-enable)
    │   ├── systemctl start kde-fan-control-daemon.service   (start now)
    │   └── update-icon-cache /usr/share/icons/hicolor/     (icon cache)
    │
    └── System ready: daemon running, GUI launchable, DBus active
```

### Package Remove → Clean Teardown Flow

```
[dpkg -r kde-fan-control]
    │
    ├── prerm runs:
    │   ├── systemctl stop kde-fan-control-daemon.service
    │   │   └── daemon SIGTERM → shutdown path → fallback (pwm=255 for owned fans) → exit 0
    │   ├── systemctl disable kde-fan-control-daemon.service
    │   └── daemon no longer running; fans at safe max or BIOS-managed
    │
    ├── Files removed from FHS paths
    │
    └── postrm runs:
        └── systemctl daemon-reload
        └── (config at /var/lib/kde-fan-control/ may be preserved via dpkg conffile or purged on purge)
```

### Key Data Flows

1. **Daemon startup → systemd readiness:** hwmon discovery → config load → boot reconciliation → DBus name claim → `sd_notify READY=1`. Only after all five succeed does systemd consider the unit active.

2. **GUI launch → daemon auto-start:** GUI makes DBus call → dbus-daemon sees missing bus name → reads activation file → tells systemd to start unit → daemon starts → READY=1 → DBus name acquired → method call delivered.

3. **Privileged write → polkit check:** Unprivileged caller → `require_authorized()` → polkit CheckAuthorization (with `ALLOW_INTERACTIVE_AUTHORIZATION` flag) → possibly interactive auth dialog → authorized or denied → proceed or reject.

4. **Daemon failure → systemd restart + fallback:** Panic/crash → systemd detects process exit → `Restart=on-failure` triggers new start → old process's panic hook already wrote pwm=255 for owned fans → new process reconciles persisted config on boot.

5. **Package install → system integration:** dpkg installs files → `postinst` runs `systemctl daemon-reload` + `systemctl enable` + `systemctl start` → daemon running, GUI launchable from desktop.

6. **Package remove → clean teardown:** dpkg `prerm` runs `systemctl stop` + `systemctl disable` → daemon shutdown runs fallback path → fans at safe max → package files removed.

## Integration Points

### Changes to Existing Components

| Component | Change Type | What Changes | Impact |
|-----------|-------------|--------------|--------|
| `crates/daemon/src/main.rs` | **MODIFY** | Add `sd_notify::notify(READY=1)` after DBus name acquisition; add periodic `sd_notify(WATCHDOG=1)` in control loop; add `sd_notify(STOPPING=1)` before shutdown fallback | Small, well-contained additions to startup, runtime, and shutdown paths |
| `crates/daemon/src/main.rs` | **MODIFY** | Replace `require_authorized()` UID-0 check with polkit CheckAuthorization call; fallback to UID-0 if polkit is unavailable | Changes auth internals but preserves DBus method contract; all write methods call `require_authorized()` already |
| `crates/daemon/Cargo.toml` | **MODIFY** | Add `sd-notify = "0.5"` dependency; add polkit check dependency | Two new deps, both small and well-established |
| `crates/cli/src/main.rs` | **MODIFY** | Improve error message when AccessDenied to mention polkit auth; hint at `--allow-interactive-auth` context | Minor UX improvement, no functional change |
| `gui/CMakeLists.txt` | **MODIFY** | Add install targets for `.desktop` file, icon assets, and polkit policy | CMake install additions |
| `gui/src/daemon_interface.cpp` | **MODIFY** | Set `ALLOW_INTERACTIVE_AUTHORIZATION` flag on DBus method calls for write operations so polkit can show interactive prompts | Small flag addition per write call |
| `gui/data/` | **ADD** | Application icon SVG/PNG files for hicolor icon theme | New static files |

### New Components (Data Files)

| Component | Source Location | Install Destination | Purpose |
|-----------|----------------|---------------------|---------|
| `kde-fan-control-daemon.service` | `packaging/systemd/` | `/usr/lib/systemd/system/` | Systemd unit for daemon lifecycle |
| `org.kde.FanControl.service` | `packaging/dbus/` | `/usr/share/dbus-1/system-services/` | DBus service activation → systemd |
| `org.kde.fancontrol.policy` | `packaging/polkit/` | `/usr/share/polkit-1/actions/` | Polkit actions for privileged writes |
| `org.kde.fancontrol.gui.desktop` | `packaging/desktop/` | `/usr/share/applications/` | Desktop entry for GUI app |
| Icon SVG/PNG | `packaging/icons/hicolor/` | `/usr/share/icons/hicolor/` | App + tray icon; referenced by .desktop and KStatusNotifierItem |
| Debian packaging files | `packaging/debian/` | (build metadata, not installed) | `control`, `rules`, `postinst`, `prerm`, `changelog` |
| `install.sh` | `packaging/` | (run directly, not installed) | Standalone installer script |

### No-Change Components

| Component | Why No Change |
|-----------|---------------|
| `crates/core/` | Core config, inventory, control, and lifecycle modules have no packaging/integration concerns |
| DBus interface contract (`org.kde.FanControl.*`) | Adding polkit doesn't change method signatures — auth is an internal implementation detail |
| DBus system bus policy (`org.kde.FanControl.conf`) | Already exists with correct policy (root owns, all can send) — polkit adds auth but doesn't change bus-level ACL |
| GUI QML files | No QML changes needed for packaging milestone |
| Notification config (`kdefancontrol.notifyrc`) | Already installed by CMake `install()` directive; no change |

## Systemd Unit Specification

```ini
[Unit]
Description=KDE Fan Control Daemon
Documentation=https://github.com/user/kde-fan-control
After=multi-user.target
Wants=multi-user.target

[Service]
Type=notify
NotifyAccess=main
ExecStart=/usr/bin/kde-fan-control-daemon
Restart=on-failure
RestartSec=5

# Watchdog: daemon sends WATCHDOG=1 periodically
WatchdogSec=60

# Hardening: daemon needs sysfs write + DBus + config persistence
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/sys/class/hwmon /var/lib/kde-fan-control
NoNewPrivileges=true

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=kde-fan-control-daemon

[Install]
WantedBy=multi-user.target
```

**Key decisions:**
- `Type=notify` — explicit readiness via `sd_notify` after full startup
- `NotifyAccess=main` — only the main process sends readiness
- `Restart=on-failure` — auto-restart unless cleanly stopped; daemon's panic hook + systemd's SIGABRT handling ensure fallback on crash
- `WatchdogSec=60` — daemon must ping every 60s; if it doesn't, systemd assumes hung and restarts
- `ProtectSystem=strict` + `ReadWritePaths=` — only write to the specific directories needed
- `WantedBy=multi-user.target` — boot-enabled in normal runlevel

**Note on `ReadWritePaths=/sys/class/hwmon`:** `ProtectSystem=strict` blocks all writes to `/sys`. The daemon needs to write PWM values. The `ReadWritePaths=` exception is required. If this proves insufficient (some `/sys` writes go through symlinks that resolve outside the hwmon path), `ProtectSystem=full` (less strict: only `/usr` is read-only) can be used instead.

## Polkit Policy Specification

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
  "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
  "http://www.freedesktop.org/standards/PolicyKit/1.0/policyconfig.dtd">
<policyconfig>
  <vendor>KDE</vendor>
  <vendor_url>https://invent.kde.org/user/kde-fan-control</vendor_url>
  <icon_name>kde-fan-control-gui</icon_name>

  <action id="org.kde.fancontrol.write-config">
    <description>Modify fan control configuration</description>
    <description xml:lang="x-generic">Modify fan control configuration</description>
    <message>Authentication is required to modify fan control settings</message>
    <message xml:lang="x-generic">Authentication is required to modify fan control settings</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>
</policyconfig>
```

**Key decisions:**
- One action ID `org.kde.fancontrol.write-config` covers all privileged write methods (enrollment, draft apply, control-profile changes, auto-tune, naming). The security boundary is "can you change what the daemon does?" — a single action captures this.
- `auth_admin_keep` for active sessions: authorized admins authenticate once, get a short-lived credential cache (typically 5 min). The GUI user enters a password once and then can perform all write operations for a session.
- `auth_admin` for inactive/any: stricter — no cached auth for non-active sessions (SSH, cron, etc.).
- No separate action for daemon start/stop — that's already constrained by systemd's own polkit policies (`org.freedesktop.systemd1.manage-units`).

**Alternative considered:** Separate actions for `org.kde.fancontrol.enroll-fan`, `org.kde.fancontrol.apply-config`, `org.kde.fancontrol.auto-tune`, etc. Rejected because: (1) no real-world use case for allowing enrollment but blocking apply, (2) adds complexity without security benefit, (3) all write operations go through the same `require_authorized()` function, (4) can be split later if needed without breaking the API.

## DBus Service Activation File

```ini
# /usr/share/dbus-1/system-services/org.kde.FanControl.service
[D-BUS Service]
Name=org.kde.FanControl
SystemdService=kde-fan-control-daemon.service
# No Exec= line — we use systemd activation, not direct launch
```

**Why SystemdService=, not Exec=:**
- `Exec=` would have dbus-daemon launch the binary directly, bypassing systemd's lifecycle and sandboxing
- `SystemdService=` tells dbus-daemon to ask systemd to start the unit, getting all systemd benefits (cgroups, sandboxing, watchdog, restart policy)
- This is the standard pattern for modern system DBus services

## .desktop File Specification

```ini
[Desktop Entry]
Type=Application
Name=KDE Fan Control
Name[x-generic]=KDE Fan Control
Comment=Desktop fan control for Linux
Comment[x-generic]=Desktop fan control for Linux
Icon=kde-fan-control-gui
Exec=/usr/bin/kde-fan-control-gui
Terminal=false
Categories=System;HardwareSettings;Qt;KDE;
Keywords=fan;cooling;hardware;temperature;pwm;
StartupNotify=true
DBusActivatable=false
SingleMainWindow=true
```

**Key decisions:**
- `Terminal=false` — GUI app, no terminal window
- `Categories=System;HardwareSettings;Qt;KDE;` — appears in system settings / hardware category
- `DBusActivatable=false` — the GUI is NOT a DBus-activated service; it's a regular desktop app
- No `OnlyShowIn=KDE;` — the app works on any desktop with Qt6 + Kirigami, though KDE is the primary target
- Tray icon is embedded in the GUI process via KStatusNotifierItem — no separate .desktop entry for the tray

## Anti-Patterns to Avoid

### Anti-Pattern 1: User Service Instead of System Service

**What people do:** Install the daemon as a systemd user service (`~/.config/systemd/user/`) because it's "easier" for a desktop app.
**Why it's wrong:** The daemon must write to `/sys/class/hwmon/*/pwm*` — these sysfs files require root. User services run as the unprivileged user and cannot write to sysfs. The daemon MUST run as root.
**Do this instead:** System service (`/usr/lib/systemd/system/`) running as root, with `Type=notify`.

### Anti-Pattern 2: DBus Policy Allowing Non-Root to Own the Name

**What people do:** Set `<allow own="org.kde.FanControl"/>` in the default policy so any user can own the name.
**Why it's wrong:** Any user process could claim `org.kde.FanControl` and serve a malicious interface. Only the daemon, running as root, should own this name.
**Do this instead:** `<allow own="org.kde.FanControl"/>` ONLY in `<policy user="root">`, while `<allow send_destination="org.kde.FanControl"/>` is in the default policy. This is what the existing `org.kde.FanControl.conf` already does correctly.

### Anti-Pattern 3: Direct Exec= in DBus Service Activation

**What people do:** Use `Exec=/usr/bin/kde-fan-control-daemon` in the DBus `.service` file.
**Why it's wrong:** dbus-daemon launches the binary directly, bypassing systemd's service manager. The daemon loses watchdog, restart policy, cgroup isolation, and `Type=notify` readiness semantics.
**Do this instead:** `SystemdService=kde-fan-control-daemon.service` — let systemd manage the process.

### Anti-Pattern 4: Requiring `sudo` for GUI/CLI Write Operations

**What people do:** Don't add polkit; instead require the user to `sudo kde-fan-control-gui` or `sudo kde-fan-control apply`.
**Why it's wrong:** Running the entire GUI as root is a security anti-pattern. The GUI is a large C++/QML codebase with many attack surfaces. CLI requiring `sudo` for every privileged call is cumbersome and error-prone.
**Do this instead:** Polkit policy with `auth_admin_keep` — authenticate once, then use the GUI/CLI normally for a period. The daemon (small, Rust, root) is the only privileged process.

### Anti-Pattern 5: Packaging Without systemd Post-Install Hooks

**What people do:** Install the `.service` file but don't run `systemctl daemon-reload` / `systemctl enable` in postinst.
**Why it's wrong:** The unit file sits on disk but systemd doesn't know about it until the next boot (or manual `daemon-reload`). The daemon won't auto-start on boot.
**Do this instead:** `postinst` script runs `systemctl daemon-reload`, `systemctl enable kde-fan-control-daemon.service`, and optionally `systemctl start kde-fan-control-daemon.service`.

### Anti-Pattern 6: Splitting Config Between Package and Runtime

**What people do:** Install a default config file in `/etc/` and have the daemon read it, creating a split-brain scenario where the package manager and daemon both think they own the config.
**Why it's wrong:** The daemon already owns its config at `/var/lib/kde-fan-control/config.toml`. Two sources of truth causes upgrade conflicts and confusion.
**Do this instead:** Package installs NO config file. The daemon creates its config on first run (it already does this via `AppConfig::default()`). The package only installs the daemon binary and integration files.

### Anti-Pattern 7: install.sh Using Blind `cp -f` Over System Files

**What people do:** `install.sh` just `cp` files over system directories without checking.
**Why it's wrong:** Can clobber existing configs, doesn't handle upgrades cleanly, no undo mechanism.
**Do this instead:** `install.sh` should: check for existing installation, back up replaced files, use `install -D` with proper permissions, and provide an `--uninstall` option.

## Build Order

The packaging and system integration milestone has a clear dependency order:

```
Phase 1: Static Data Files (no code changes needed)
  ├── systemd unit file
  ├── DBus service activation file
  ├── polkit policy XML
  ├── .desktop file
  └── Icon assets

  Why first: These are pure data files that can be tested independently
  by installing them manually and verifying systemd/dbus/polkit behavior.
  No code compilation required.

Phase 2: Daemon Integration (small Rust code changes)
  ├── Add sd-notify dependency + READY=1 / WATCHDOG=1 / STOPPING=1 calls
  ├── Add polkit check to require_authorized() (with UID-0 fallback)
  └── Update Cargo.toml

  Why second: Depends on the unit file and polkit policy existing to
  verify the integration works end-to-end. Can be developed in parallel
  with Phase 1 using manual install for testing.

Phase 3: GUI Integration (small C++/CMake changes)
  ├── Add ALLOW_INTERACTIVE_AUTHORIZATION flag to write DBus calls
  ├── Add .desktop file + icon install targets to CMakeLists.txt
  └── Improve auth-denied error messages

  Why third: GUI changes depend on polkit policy being installed,
  otherwise interactive auth prompts won't appear. However, the
  ALLOW_INTERACTIVE_AUTHORIZATION flag is harmless without polkit
  (it's just a hint), so this can actually be done in parallel with
  Phase 2.

Phase 4: Packaging (.deb + install.sh)
  ├── Debian packaging metadata
  ├── Build script to stage FHS tree from release artifacts
  ├── postinst / prerm hooks for systemd integration
  ├── install.sh fallback installer
  └── End-to-end install + uninstall test

  Why last: Packaging depends on ALL above artifacts being finalized
  and their correct install paths being known.
```

**Dependency rationale:**
- Phase 1 has zero code dependencies — it's static files only.
- Phase 2 depends on Phase 1 files being installed (to test systemd + polkit integration end-to-end), but can be developed in parallel with manual testing.
- Phase 3 depends on Phase 2 (polkit must be in the daemon before GUI can trigger interactive auth), though the flag itself is harmless.
- Phase 4 depends on Phases 1–3 (needs all artifacts and their correct install paths).

**Suggested execution strategy:** Phases 1 + 2 can run in parallel (different file types, different tools). Phase 3 can start as soon as Phase 2's polkit integration is testable. Phase 4 starts after Phases 1–3 are complete.

## Scaling Considerations

| Concern | Single-user desktop | Multi-user system | Headless server |
|---------|---------------------|-------------------|-----------------|
| Systemd unit | Standard boot-start | Standard boot-start | Standard boot-start |
| Polkit auth | Single auth prompt per session | Each user authenticates independently | CLI only; polkit may not be present (fallback to UID-0) |
| DBus activation | On-demand from GUI | On-demand from CLI or GUI | Manual `systemctl start` or DBus activation from CLI |
| Package format | `.deb` | `.deb` | `.deb` or `install.sh` |

This is a single-machine desktop tool. Scaling beyond "one machine, one daemon, one config" is explicitly out of scope for v1.

## Sources

- systemd.service(5) man page: https://man7.org/linux/man-pages/man5/systemd.service.5.html — HIGH
- D-Bus Specification (service activation, ALLOW_INTERACTIVE_AUTHORIZATION flag 0x4): https://dbus.freedesktop.org/doc/dbus-specification.html — HIGH
- zbus documentation (service setup, system bus, connection builder): Context7 `/dbus2/zbus` — HIGH
- sd-notify crate: https://crates.io/crates/sd-notify — MEDIUM
- polkit specification: https://www.freedesktop.org/software/polkit/docs/latest/ — HIGH (site returned 418 but content is well-known from Linux desktop ecosystem)
- Linux FHS standard: https://refspecs.linuxfoundation.org/fhs.shtml — HIGH
- freedesktop.org desktop entry specification: https://specifications.freedesktop.org/desktop-entry-spec/latest/ — HIGH
- freedesktop.org icon theme specification: https://specifications.freedesktop.org/icon-theme-spec/latest/ — HIGH
- DBus service activation semantics: D-Bus Specification § "Message Bus Starting Services (Activation)" — HIGH
- Existing codebase: `/home/samiran/kde-fan-control/` — HIGH
- Existing DBus policy: `packaging/dbus/org.kde.FanControl.conf` — HIGH
- Existing daemon auth code: `crates/daemon/src/main.rs` require_authorized() function — HIGH
- Existing GUI DBus interface: `gui/src/daemon_interface.h` and `gui/src/daemon_interface.cpp` — HIGH
- Existing config path: `crates/core/src/config.rs` config_path() function using `dirs::state_dir()` — HIGH
- Previous architecture research: `.planning/research/ARCHITECTURE.md` — HIGH (context for existing architecture)

---
*Architecture research for: KDE Fan Control — Packaging & System Integration*
*Researched: 2026-04-11*