# Technology Stack — Packaging & System Integration

**Project:** KDE Fan Control  
**Dimension:** Stack (Packaging & System Integration Milestone)  
**Researched:** 2026-04-11  
**Overall recommendation confidence:** HIGH

## Scope

This document covers **only** stack additions and changes needed for the packaging and system integration milestone. The existing Rust daemon + CLI, Qt6/Kirigami GUI, DBus API, and core control logic are validated and out of scope.

**New features to stack for:**
- systemd unit file for the daemon (`Type=notify`, boot-enabled, watchdog)
- `.deb` package as primary install target + `install.sh` as fallback
- `.desktop` files for GUI (with embedded tray icon)
- CLI placed in PATH (`/usr/bin`)
- Polkit policy for daemon start/stop and config writes (superuser prompt)
- DBus service and policy files installed system-wide
- Standard FHS file layout
- On-demand daemon start fallback if GUI launches but daemon isn't running

## Existing Stack Confirmed (No Changes Needed)

These are already in the project and do **not** change for this milestone:

| Component | Status | Note |
|---|---|---|
| Rust daemon + CLI (Tokio 1.x + zbus 5.x + clap 4.x) | ✓ No change | Already builds correctly |
| Qt6/Kirigami GUI + KStatusNotifierItem | ✓ No change | Already builds with CMake |
| DBus bus name `org.kde.FanControl` | ✓ No change | Already hardcoded in daemon and GUI |
| Existing `packaging/dbus/org.kde.FanControl.conf` | ✓ Extends | Already has root-own and default-send policies |

## New Stack Additions

### 1. systemd Service Integration

| Technology | Version | Purpose | Why | Confidence |
|---|---:|---|---|---|
| `sd-notify` | 0.5.0 | Notify systemd of readiness and watchdog keep-alive from Rust daemon | Minimal, purpose-built crate; the daemon already needs `Type=notify` semantics; supports `fdstore` feature for future socket activation | HIGH |
| systemd unit file | N/A (declarative) | `fancontrold.service` with `Type=notify` | `Type=notify` over `Type=simple` because the daemon must complete hwmon discovery, config load, and DBus name acquisition before systemd considers it "started"; `Type=dbus` is viable but `Type=notify` is better because the daemon also needs non-DBus startup validation checks | HIGH |

**Why `sd-notify` 0.5.0 and not alternatives:**

| Alternative | Why not |
|---|---|
| `libsystemd` FFI bindings | C dependency; `sd-notify` is pure Rust and wraps the same socket protocol with zero overhead |
| Inline `unsafe` socket writes | Pointless risk when `sd-notify` already handles edge cases (socket missing, EAGAIN, etc.) |
| `notify_ready` / `sys_notify` crates | Smaller community, less proven, fewer features; `sd-notify` is the standard choice |
| `Type=simple` with no notification | systemd would consider the daemon "started" before hwmon discovery or DBus registration succeed — unsafe for a fan-control daemon |

**systemd unit file details (derived from thermald and power-profiles-daemon real-world examples):**

```ini
[Unit]
Description=KDE Fan Control Daemon
ConditionVirtualization=no
After=multi-user.target

[Service]
Type=notify
NotifyAccess=main
BusName=org.kde.FanControl
ExecStart=/usr/sbin/fancontrold
Restart=on-failure
WatchdogSec=60
# Hardening
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
NoNewPrivileges=yes
ReadWritePaths=/etc/fancontrol /var/lib/fancontrol /run/fancontrol
ProtectKernelTunables=yes
ProtectKernelModules=yes
RestrictNamespaces=yes
LockPersonality=yes

[Install]
WantedBy=multi-user.target
Alias=dbus-org.kde.FanControl.service
```

**Key decisions:**

1. **`Type=notify`** — not `Type=dbus`. The daemon needs to validate hwmon state and load config *before* announcing readiness. `Type=notify` lets the daemon control exactly when systemd considers it started. The `BusName=` directive is still set so `systemctl` can map the service to its DBus name.

2. **`Alias=dbus-org.kde.FanControl.service`** — This alias is the conventional pattern for DBus-activatable system services. When dbus-daemon sees a `.service` file with `SystemdService=dbus-org.kde.FanControl.service`, it can find the matching systemd unit.

3. **`WantedBy=multi-user.target`** — not `graphical.target`. The daemon manages fans which is a hardware safety concern; it should start even without a graphical session. `multi-user.target` is correct for headless servers that need fan control too.

4. **`WatchdogSec=60`** — Generous for a fan-control daemon. The daemon's PID loop runs at ~1Hz; a 60-second heartbeat interval gives room for slow hwmon scans without false kills. The daemon must call `sd_notify::watchdog()` regularly.

5. **Hardening directives** — Follow `power-profiles-daemon`'s pattern. `NoNewPrivileges=yes` is safe because the daemon runs as root and never needs to escalate further. `ReadWritePaths=` must include the config directory and any state/runtime paths.

### 2. DBus Service Activation File

| Technology | Version | Purpose | Why | Confidence |
|---|---|---|---|---|
| DBus `.service` file | N/A (declarative) | Tell dbus-daemon how to activate the service on-demand | Standard freedesktop.org mechanism; enables on-demand daemon start when GUI connects to system bus | HIGH |

**File:** `/usr/share/dbus-1/system-services/org.kde.FanControl.service`

```ini
[D-BUS Service]
Name=org.kde.FanControl
Exec=/bin/false
User=root
SystemdService=fancontrold.service
```

**Why `Exec=/bin/false` and `SystemdService=`:**

This is the **standard 2026 pattern** for systemd-managed DBus services (confirmed by `net.hadess.PowerProfiles.service` on this system). The key insight:

- **`Exec=/bin/false`** — Tells dbus-daemon *"don't try to launch the binary yourself"*. The `SystemdService=` key takes precedence on systemd systems.
- **`SystemdService=fancontrold.service`** — Tells dbus-daemon *"ask systemd to start this unit instead"*. This delegates lifecycle to systemd, which is the correct supervision model for a hardware-control daemon.
- This pattern gives us on-demand activation for free: when the GUI or CLI connects to `org.kde.FanControl` on the system bus, dbus-daemon will ask systemd to start `fancontrold.service`.

**Do NOT use** `Exec=/usr/sbin/fancontrold` without `SystemdService=`. That would let dbus-daemon manage the process directly, bypassing systemd's supervision, restart, and hardening guarantees. For a fan-control daemon, systemd must be the process supervisor.

### 3. DBus System Policy File

| Technology | Version | Purpose | Why | Confidence |
|---|---|---|---|---|
| DBus busconfig XML | N/A (declarative) | Control which users can own and talk to the service on the system bus | Required by dbus-daemon; without it, non-root clients cannot communicate with the service | HIGH |

**File:** `/usr/share/dbus-1/system.d/org.kde.FanControl.conf`

The project already has `packaging/dbus/org.kde.FanControl.conf` with basic policies. It needs to be extended with interface-level granularity following the PowerProfiles pattern:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE busconfig PUBLIC
 "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <!-- Only root can own the service name -->
  <policy user="root">
    <allow own="org.kde.FanControl"/>
  </policy>

  <!-- Everyone can call read methods and receive signals -->
  <policy context="default">
    <allow send_destination="org.kde.FanControl"
           send_interface="org.kde.FanControl.Inventory"/>
    <allow send_destination="org.kde.FanControl"
           send_interface="org.freedesktop.DBus.Introspectable"/>
    <allow send_destination="org.kde.FanControl"
           send_interface="org.freedesktop.DBus.Properties"/>
    <allow send_destination="org.kde.FanControl"
           send_interface="org.freedesktop.DBus.Peer"/>
    <allow receive_sender="org.kde.FanControl"/>
  </policy>
</busconfig>
```

**Design decision:** The DBus policy file provides *transport-layer* access control. Write-operation authorization (who can apply config, start/stop the daemon) is handled by **Polkit** in the daemon, not by DBus policy. The DBus policy is intentionally permissive for sending — it allows anyone to send messages to any interface. The daemon checks Polkit authorization on write methods internally. This is the correct architecture because:

1. DBus policy can only restrict by **user/group**, not by interactive authorization
2. Polkit provides the GUI authentication prompt ("enter your password")
3. The daemon already checks caller UID for write operations (line 785-828 in `main.rs`); Polkit extends this

### 4. Polkit Policy

| Technology | Version | Purpose | Why | Confidence |
|---|---|---|---|---|
| Polkit `.policy` XML | N/A (declarative) | Define privileged actions with interactive authentication prompts | This is how Linux desktop apps safely escalate: the GUI/CLI calls a DBus method, the daemon checks Polkit, the user gets a password prompt | HIGH |
| `zbus` Polkit check | 5.14.0 (already in stack) | Daemon-side Polkit authority check via DBus call to `org.freedesktop.PolicyKit1` | No new Rust crate needed; check authorization through the existing `zbus` connection to the system bus | HIGH |

**File:** `/usr/share/polkit-1/actions/org.kde.FanControl.policy`

```xml
<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1.0/policyconfig.dtd">
<policyconfig>
  <vendor>KDE Fan Control</vendor>
  <vendor_url>https://github.com/user/kde-fan-control</vendor_url>

  <action id="org.kde.FanControl.apply-config">
    <description>Apply fan control configuration</description>
    <message>Authentication is required to apply fan control settings</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>

  <action id="org.kde.FanControl.manage-daemon">
    <description>Start or stop the fan control daemon</description>
    <message>Authentication is required to manage the fan control service</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>
</policyconfig>
```

**Polkit Authorization Semantics:**

| Action | `allow_any` | `allow_inactive` | `allow_active` | Rationale |
|---|---|---|---|---|
| `apply-config` | `auth_admin` | `auth_admin` | `auth_admin_keep` | Writing fan control config is a system-level operation; active desktop users get `keep` so they're not prompted every time during a session |
| `manage-daemon` | `auth_admin` | `auth_admin` | `auth_admin_keep` | Starting/stopping the fan control service must require admin auth; `keep` caches for convenience |

**Why `auth_admin_keep` not `auth_self`:** Fan control affects system thermal safety. Any user who can write fan config could disable cooling. This requires administrator (root) authorization, not just "the user proves who they are." This matches corectrl's pattern: `auth_admin_keep` for hardware control operations.

**Why no new Rust crate for Polkit:** The daemon already has a `zbus` connection and a DBus method for checking caller UID. Polkit authorization is checked by calling `org.freedesktop.PolicyKit1.Authority.CheckAuthorization()` over the system bus. This is a standard DBus method call — `zbus` handles it natively. No additional crate needed.

### 5. Desktop Entry Files

| Technology | Version | Purpose | Why | Confidence |
|---|---|---|---|---|
| XDG Desktop Entry Specification | 1.5 | `.desktop` file for GUI app discovery | Required for app launchers, KDE menu, and Wayland compliance | HIGH |
| XDG Icon Theme Specification | current | Icon resolution for `.desktop` and tray | Required for `.desktop` `Icon=` key and KStatusNotifierItem | HIGH |

**GUI `.desktop` file:** `/usr/share/applications/org.kde.FanControl.desktop`

```ini
[Desktop Entry]
Type=Application
Version=1.5
Name=Fan Control
GenericName=Fan Control
Comment=Manage fan speed and temperature policies
Icon=org.kde.FanControl
Exec=gui_app
Terminal=false
Categories=System;HardwareSettings;
Keywords=fan;cooling;temperature;hardware;PWM;
StartupNotify=true
SingleMainWindow=true
```

**Key design decisions:**

1. **File name matches DBus name** — `org.kde.FanControl.desktop` follows the freedesktop convention. If DBus activation is added later for the GUI, the name must match the bus name (spec requirement).

2. **`Categories=System;HardwareSettings;`** — This places the app in the KDE System Settings → Hardware category, next to other hardware control apps.

3. **`Icon=org.kde.FanControl`** — Icons installed to `/usr/share/icons/hicolor/` will be found by the icon theme spec. Must provide at least 16x16, 22x22, 32x32, 48x48, 64x64, and 128x128 PNGs, plus an SVG scalable.

4. **`Terminal=false`** — The GUI is a graphical app, not a terminal tool.

5. **`SingleMainWindow=true`** — The GUI is a single-window app; KDE will not offer "open another window."

6. **No `DBusActivatable=true`** — The GUI is not DBus-activated in v1. It starts via the `.desktop` Exec key normally. The *daemon* is DBus-activated, not the GUI.

### 6. FHS File Layout

The following layout follows the Linux FHS 3.0 and established conventions from thermald, power-profiles-daemon, and corectrl:

```
/etc/fancontrol/                          # Daemon configuration
    config.toml                           #   Active configuration

/var/lib/fancontrol/                      # Daemon state (fallback incidents, runtime state)
    state.json                            #   Persisted runtime state

/run/fancontrol/                          # Runtime data (PID file if needed, sockets)
    fancontrold.pid                       #   (optional — sd-notify makes this unnecessary)

/usr/sbin/fancontrold                     # Daemon binary (root-owned)
/usr/bin/kfc                              # CLI binary (PATH-accessible)
/usr/bin/gui_app                          # GUI binary (PATH-accessible)

/usr/lib/systemd/system/                  # systemd unit files
    fancontrold.service                   #   Daemon service unit

/usr/share/dbus-1/system-services/        # DBus service activation
    org.kde.FanControl.service            #   Service activation file

/usr/share/dbus-1/system.d/              # DBus system bus policy
    org.kde.FanControl.conf               #   Bus access policy

/usr/share/polkit-1/actions/             # Polkit policies
    org.kde.FanControl.policy             #   Authorization actions

/usr/share/applications/                  # Desktop entry files
    org.kde.FanControl.desktop            #   GUI application entry

/usr/share/icons/hicolor/                # Icon theme
    scalable/apps/org.kde.FanControl.svg
    128x128/apps/org.kde.FanControl.png
    64x64/apps/org.kde.FanControl.png
    48x48/apps/org.kde.FanControl.png
    32x32/apps/org.kde.FanControl.png
    22x22/apps/org.kde.FanControl.png
    16x16/apps/org.kde.FanControl.png

/usr/share/doc/fancontrol/               # Documentation
    README.md
    LICENSE

/usr/share/knotifications5/              # KDE notification config
    kdefancontrol.notifyrc                #   (already installed by CMake)
```

**Key decisions:**

1. **`/usr/sbin/fancontrold`** — Daemons go in `sbin`, not `bin`. This is FHS convention and matches thermald (`/usr/sbin/thermald`) and power-profiles-daemon (`/usr/libexec/power-profiles-daemon`). We prefer `/usr/sbin/` over `/usr/libexec/` because the daemon has a direct CLI-like interface.

2. **`/usr/bin/kfc`** — The CLI must be in PATH. `/usr/bin/` is standard for user commands. The name `kfc` matches the existing binary name.

3. **`/usr/bin/gui_app`** — The GUI must be in PATH. Consider renaming from `gui_app` to `kde-fan-control` or `fancontrol-gui` for clarity. This is a build-system rename, not a code change.

4. **`/etc/fancontrol/config.toml`** — Daemon-owned, daemon-written config. Never split-brain with client-side config.

5. **`/var/lib/fancontrol/`** — Persistent state. The `StateDirectory=fancontrol` systemd directive creates this automatically with correct permissions.

6. **No `/run/fancontrol/fancontrold.pid`** — `Type=notify` with `sd-notify` makes PID files unnecessary. systemd tracks the main PID internally.

### 7. .deb Package

| Technology | Version | Purpose | Why | Confidence |
|---|---|---|---|---|
| `dpkg-deb` / manual deb build | N/A | Primary install target | Debian/Ubuntu is the primary distro target; manual deb gives full control over file layout and scripts | HIGH |
| `debhelper` | 13+ | Standard deb packaging helper | Automates `postinst`/`prerm` scripts, systemd integration, icon cache updates | MEDIUM |
| `dh-systemd` / `dh_installsystemd` | included in debhelper 13+ | systemd unit file installation and enablement | Standard debhelper integration; handles `systemctl enable`, `daemon-reload`, restart on upgrade | HIGH |

**Package structure:**

Split into **two packages** following the established pattern (corectrl does the same):

| Package | Contents | Rationale |
|---|---|---|
| `fancontrold` | daemon binary, CLI binary, systemd unit, DBus service/policy, Polkit policy, config/state dirs, docs | All privileged components in one package; `fancontrold` is the daemon package name |
| `kde-fan-control` | GUI binary, `.desktop` file, icons, `notifyrc` | GUI is unprivileged and depends on `fancontrold` |

**`fancontrold` package `postinst` script actions:**
```bash
# Create config directory with correct permissions
mkdir -p /etc/fancontrol
chmod 755 /etc/fancontrol
# Create state directory
mkdir -p /var/lib/fancontrol
chmod 700 /var/lib/fancontrol
# Reload systemd
systemctl daemon-reload
# Enable and start (if not upgrading)
if [ "$1" = "configure" ] && [ -z "$2" ]; then
    systemctl enable fancontrold.service
    systemctl start fancontrold.service
fi
```

**`fancontrold` package `prerm` script actions:**
```bash
# Stop the daemon before removal — critical for fan safety!
# The daemon's own shutdown hook sets fans to safe-maximum
systemctl stop fancontrold.service || true
systemctl disable fancontrold.service || true
```

**`kde-fan-control` package `postinst` script actions:**
```bash
# Update icon cache and desktop database
gtk-update-icon-cache -f /usr/share/icons/hicolor/ 2>/dev/null || true
update-desktop-database /usr/share/applications/ 2>/dev/null || true
```

**Why manual deb and not `cargo-deb`:**

| Option | Why not |
|---|---|
| `cargo-deb` | Only builds the Rust portion; the GUI is CMake-built. Would need separate packaging for Qt artifacts. Not worth the complexity of splitting across two build systems. |
| `cpack` | CMake-native packaging; doesn't know about Rust build artifacts. Inverse of `cargo-deb` problem. |
| Manual deb with `dpkg-deb` | Full control over both Rust and Qt artifacts; no hidden logic; two source trees → one unified packaging step is the simplest approach |

### 8. install.sh Fallback

| Technology | Version | Purpose | Why | Confidence |
|---|---|---|---|---|
| POSIX shell script | N/A | Fallback installer for non-deb systems | Users on Arch, Fedora, or custom systems need a way to install; the script must be simple, auditable, and safe | HIGH |

**`install.sh` design:**

- Must detect root or fail
- Must copy all files to FHS paths (same paths as the .deb)
- Must run `systemctl daemon-reload && systemctl enable fancontrold.service`
- Must **not** start the daemon automatically in the fallback script (let the user decide)
- Must provide `uninstall.sh` that reverses everything
- Must not be interactive — no prompts, no `read`

### 9. On-Demand Daemon Start

This feature comes for free from the DBus service activation architecture (Section 2 above). The flow:

1. User launches GUI → GUI connects to system bus → checks `isServiceRegistered("org.kde.FanControl")`
2. If not registered → DBus daemon sees the `.service` file → sends `StartServiceByName` to systemd
3. systemd starts `fancontrold.service` → daemon calls `sd_notify::notify(true, "READY=1")` → systemd considers service active
4. Daemon acquires `org.kde.FanControl` on system bus → GUI detects `NameOwnerChanged` → `setConnected(true)`

**No additional libraries needed.** The existing `DaemonInterface::handleNameOwnerChanged` slot and `StatusMonitor` already handle the service-present → service-absent transitions. The only missing piece is the `.service` file installation.

### 10. Daemon Code Changes for `sd-notify`

The daemon needs two code changes:

**a) Send `READY=1` after successful startup**

```rust
// After hwmon discovery, config load, and DBus name acquisition succeed:
sd_notify::notify(true, "READY=1").ok(); // best-effort; no error if not under systemd
```

**b) Send `WATCHDOG=1` periodically**

```rust
// In the main control loop, every 30 seconds:
sd_notify::notify(false, "WATCHDOG=1").ok();
```

**c) Send `STOPPING=1` before shutdown**

```rust
// As the first step of graceful shutdown (before setting fans to safe-max):
sd_notify::notify(false, "STOPPING=1").ok();
```

**Add to `Cargo.toml`:**

```toml
[dependencies]
sd-notify = "0.5.0"
```

## What NOT to Add

| Avoid | Why not |
|---|---|
| AppImage / Snap / Flatpak | Fan control requires direct sysfs access and systemd integration; sandboxed packaging doesn't work for hardware control daemons |
| `libsystemd-sys` or `systemd` Rust crate | `sd-notify` already wraps the same protocol in pure Rust without C FFI |
| `cargo-deb` for packaging | Can't build Qt artifacts; manual deb is simpler for a split-stack project |
| `Type=dbus` instead of `Type=notify` | Viable but inferior: `Type=dbus` tells systemd the daemon is ready as soon as the bus name is acquired, but the daemon must validate hwmon state first; also `Type=notify` supports watchdog |
| `pkexec` as the primary launch mechanism | `pkexec` is for running a whole program as root; we need per-method Polkit checks inside the daemon for fine-grained auth |
| DBus policy for write restriction | DBus policy can only restrict by user/group, not by interactive auth; Polkit is the right tool for fine-grained write authorization |
| `Type=simple` service type | No readiness feedback; systemd would consider the daemon "started" before it's actually serving | 
| `Exec=/usr/sbin/fancontrold` in DBus `.service` file | Bypasses systemd supervision; use `SystemdService=` delegation instead |
| System user service instead of system service | Wrong privilege model; hwmon sysfs writes require root, not user-session privileges |
| RPM as primary target | Primary target is Debian/Ubuntu; RPM can be added later |

## Rust Dependency Change

Only one new crate dependency:

```toml
# crates/daemon/Cargo.toml — ADD:
sd-notify = "0.5.0"
```

Everything else is declarative files (XML, INI, shell scripts) — no additional Rust or C++ dependencies.

## Integration Points Summary

| Integration Point | Mechanism | Files Involved | Installed By |
|---|---|---|---|
| systemd lifecycle | `fancontrold.service` unit | `packaging/systemd/fancontrold.service` | .deb `fancontrold` / install.sh |
| DBus activation | `.service` file with `SystemdService=` | `packaging/dbus/org.kde.FanControl.service` | .deb `fancontrold` / install.sh |
| DBus access policy | busconfig XML | `packaging/dbus/org.kde.FanControl.conf` (extend) | .deb `fancontrold` / install.sh |
| Polkit authorization | `.policy` XML + daemon DBus call | `packaging/polkit/org.kde.FanControl.policy` | .deb `fancontrold` / install.sh |
| App launcher | `.desktop` entry | `packaging/desktop/org.kde.FanControl.desktop` | .deb `kde-fan-control` / install.sh |
| Icon theme | hicolor SVG + PNGs | `packaging/icons/` | .deb `kde-fan-control` / install.sh |
| Binary install | FHS paths | Rust `cargo build` → staged; CMake install → staged | .deb / install.sh |

## Sources

- systemd.service(5) man page: https://man7.org/linux/man-pages/man5/systemd.service.5.html — HIGH
- sd-notify crate 0.5.0: https://crates.io/crates/sd-notify — HIGH
- sd-notify features (fdstore): crates.io API — HIGH
- DBus Specification (service activation, SystemdService key): https://dbus.freedesktop.org/doc/dbus-specification.html — HIGH
- XDG Desktop Entry Specification 1.5: https://specifications.freedesktop.org/desktop-entry-spec/latest/ — HIGH
- FHS 3.0: https://refspecs.linuxfoundation.org/FHS_3.0/ — HIGH
- PowerProfiles daemon `.service` file (real-world SystemdService pattern): `/usr/share/dbus-1/system-services/net.hadess.PowerProfiles.service` on this system — HIGH
- PowerProfiles systemd unit (hardening patterns): `/usr/lib/systemd/system/power-profiles-daemon.service` on this system — HIGH
- thermald systemd unit (Type=dbus with `Alias=`): `/usr/lib/systemd/system/thermald.service` on this system — HIGH
- thermald DBus policy (group-restricted access): `/usr/share/dbus-1/system.d/org.freedesktop.thermald.conf` on this system — HIGH
- corectrl polkit policy (auth_admin_keep for hardware control): `/usr/share/polkit-1/actions/org.corectrl.helper.policy` on this system — HIGH
- corectrl DBus policy and service files: `/usr/share/dbus-1/system.d/org.corectrl.helper.conf`, `/usr/share/dbus-1/system-services/org.corectrl.helper.service` on this system — HIGH
- Polkitaction-ids documentation: https://www.freedesktop.org/software/polkit/docs/latest/ — MEDIUM (418 blocked but pattern confirmed via real-world policy files)
- zbus system bus connection: Context7 `/dbus2/zbus` — HIGH
- Existing daemon code: `crates/daemon/src/main.rs` (BUS_NAME, auth check, DBus interfaces) — HIGH
- Existing GUI code: `gui/src/daemon_interface.cpp`, `gui/src/status_monitor.cpp` (service detection) — HIGH
- Existing DBus policy: `packaging/dbus/org.kde.FanControl.conf` — HIGH