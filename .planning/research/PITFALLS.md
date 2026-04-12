# Domain Pitfalls: Packaging & System Integration

**Domain:** Adding systemd integration, .deb packaging, polkit policies, .desktop files, and DBus service installation to an existing Rust/Qt6 Linux desktop fan-control application
**Researched:** 2026-04-11
**Overall confidence:** HIGH

## Critical Pitfalls

### Pitfall 1: `ProtectSystem=strict` and `PrivateDevices=yes` silently block hwmon writes

**What goes wrong:** The DAEMON unit file includes systemd hardening directives copied from reference services (power-profiles-daemon, fancontrol), but `ProtectSystem=strict` makes `/sys` read-only and `PrivateDevices=yes` creates a private `/dev` namespace. The daemon starts successfully, can bind its DBus name, but later hwmon writes fail silently or return EPERM. Fans appear to be managed in the UI but control is actually dead.

**Why it happens:** Debian's `fancontrol` package uses `ProtectSystem=strict` + `PrivateDevices=yes` but only reads config — it does NOT write sysfs at runtime in a way that conflicts. The Arch/Debian fancontrol unit works because it only writes to `/run` for PID tracking; the actual PWM writes go through `/sys/class/hwmon` which fancontrol accesses as root before `ProtectSystem=strict` is deeply applied. But the existing fancontrol service hardening is a model people copy without understanding what paths it actually needs.

**Consequences:** Daemon appears healthy but cannot control fans. Thermal risk. Very difficult to diagnose because the service shows "active (running)" and DBus responds to queries. Writable `/sys` failures only surface when a user tries to enroll a fan or when a control loop first writes PWM.

**Warning signs:**
- Service unit has `ProtectSystem=strict` without `ReadWritePaths=/sys/class/hwmon`
- Service unit has `PrivateDevices=yes` but needs to write sysfs
- Testing only checks DBus connectivity, not actual fan control write success
- Daemon logs show "permission denied" on `/sys/class/hwmon/hwmon*/pwm*` writes

**Prevention strategy:**
- Explicitly add `ReadWritePaths=/sys/class/hwmon` to the service unit if using `ProtectSystem=strict`
- Do NOT use `PrivateDevices=yes` — it creates a private `/dev` namespace that blocks access to device sysfs nodes; real hwmon control needs real device access
- Test the packaged service end-to-end: not just "service starts" but "daemon can enroll and control a fan"
- Start with minimal hardening and add directives one at a time, testing each with real hwmon writes
- Consider using `ProtectSystem=full` (which leaves `/sys` writable) instead of `strict` if the full hardening matrix is hard to validate

**Which phase should address it:** Phase implementing the systemd unit file — test real hwmon writes before declaring the unit file done

---

### Pitfall 2: DBus service activation file vs systemd `Type=dbus` mismatch

**What goes wrong:** The DBus `.service` activation file (in `/usr/share/dbus-1/system-services/`) has `Exec=` pointing to the daemon binary but `SystemdService=` pointing to a nonexistent or differently-named systemd unit. Or, the systemd unit uses `Type=notify` but the activation file doesn't list `SystemdService=` at all, so DBus activation tries to launch the binary directly instead of going through systemd.

**Why it happens:** There are two independent activation paths — DBus bus activation and systemd service activation — and they must agree. If the DBus `.service` file has `SystemdService=org.kde.FanControl.service` but the actual systemd unit is named `kde-fan-control-daemon.service`, DBus activation fails silently and falls back to `Exec=` (if set), or just fails entirely. The reference pattern from UDisks2, Avahi, and PowerProfiles shows the correct structure but there are subtle naming traps.

**Consequences:** On-demand daemon start (when GUI launches but daemon isn't running) fails. Users see "daemon not reachable" errors. The only workaround is manually running `systemctl start`.

**Warning signs:**
- DBus `.service` file `SystemdService=` name doesn't match the installed systemd unit filename
- The systemd unit `Alias=` doesn't create a `dbus-<BusName>.service` symlink
- Testing only covers `systemctl start` but never tests launching from GUI with daemon stopped
- The `Name=` field in the DBus `.service` file doesn't match the bus name the daemon actually requests

**Prevention strategy:**
- The DBus `.service` file name MUST match the bus name: `org.kde.FanControl.service` for bus name `org.kde.FanControl`
- Set `SystemdService=` in the DBus `.service` file to point to the exact systemd unit filename
- Set `Exec=/bin/false` in the activation file when `SystemdService=` is used (to prevent direct binary launch, as Avahi and PowerProfiles-daemon do)
- Use `Type=dbus` + `BusName=org.kde.FanControl` in the systemd unit for cleanest bus activation integration, OR use `Type=notify` and add an `Alias=dbus-org.kde.FanControl.service` to the systemd unit so bus activation resolves correctly
- Test on-demand activation: stop the daemon, launch the GUI, verify the daemon starts and the GUI connects

**Which phase should address it:** Phase implementing DBus service installation + systemd unit — test bus activation explicitly

---

### Pitfall 3: Polkit policy file installed but not wired into daemon authorization

**What goes wrong:** A `.policy` file is installed to `/usr/share/polkit-1/actions/` with correct action IDs, but the daemon's authorization check still uses the existing UID==0 check from `require_authorized()`. The polkit dialog never appears because no code calls `polkit`'s `CheckAuthorization`. Non-root users try to use the GUI for write operations and get `AccessDenied` errors even though the polkit policy says `auth_admin_keep`.

**Why it happens:** The existing codebase at `crates/daemon/src/main.rs:789` explicitly documents: "This function is explicitly structured so that a future `polkit` check can replace the UID comparison without changing the DBus method contract." But installing the `.policy` file is NOT the same as wiring the polkit CheckAuthorization call. The `.policy` file declares actions; the daemon must actively check them. There is a common misconception that polkit "automatically" intercepts DBus method calls — it does not. The DAEMON must opt in.

**Consequences:** Polkit appears to be installed but is completely non-functional. GUI users cannot perform write operations without running as root. Users may try `sudo` or `pkexec` workarounds that bypass the designed flow. The authorization UX remains the same as before the polkit work.

**Warning signs:**
- `.policy` file is installed but daemon code only checks `uid != 0`
- No zbus/bustlec calls to `org.freedesktop.PolicyKit1.Authority.CheckAuthorization`
- GUI error messages say "privileged operations require root access" instead of triggering an auth dialog
- polkit agent logs show no authorization checks for the action IDs

**Prevention strategy:**
- Replace `require_authorized()` body with a polkit `CheckAuthorization` call using `zbus` to talk to `org.freedesktop.PolicyKit1` on the system bus
- The daemon must extract caller PID + UID from the DBus message header and pass them as the `subject` to `CheckAuthorization`
- Set the `ALLOW_INTERACTIVE_AUTHORIZATION` flag on DBus method calls from the GUI side so the daemon knows it can prompt the user
- Map each privileged DBus method to a specific polkit action ID (e.g., `org.kde.FanControl.enroll-fan`, `org.kde.FanControl.apply-config`)
- Test from a non-root user session: GUI click should trigger a polkit authentication dialog
- Keep the UID==0 fallback for direct CLI use where no polkit agent is available

**Which phase should address it:** Phase implementing polkit — daemon code change is mandatory, not just file installation

---

### Pitfall 4: `ExecStopPost=` not configured for crash-safe fan fallback

**What goes wrong:** The service unit relies on `ExecStop=` for cleanup, but `ExecStop=` only runs after a **successful** start. If the daemon crashes during startup, segfaults, or is killed by the watchdog, `ExecStop=` is skipped entirely. Previously-controlled fans stay at their last PWM value (possibly low).

**Why it happens:** The systemd docs are explicit: "commands specified in ExecStop= are only executed when the service started successfully first. They are not invoked if the service was never started at all, or in case its start-up failed. Use ExecStopPost= to invoke commands when a service failed to start up correctly." This is a very common misunderstanding.

**Consequences:** Thermal risk — daemon-controlled fans stay at whatever PWM value they were at when the daemon died, with no recovery. This directly undermines the fail-safe design.

**Warning signs:**
- Service unit has `ExecStop=` but no `ExecStopPost=`
- Recovery code lives only inside the daemon's own shutdown path
- Testing only covers `systemctl stop` (clean shutdown), not `kill -9` or watchdog timeout
- No persisted record of which fans need recovery (the daemon's in-memory state is gone)

**Prevention strategy:**
- Install `ExecStopPost=` with a minimal, standalone helper script that reads the persisted enrolled-fan list and forces each enrolled channel to safe-max
- `ExecStopPost=` runs in ALL scenarios — clean stop, crash, startup failure, watchdog kill
- The helper must not depend on the daemon being alive or reachable
- Persist the set of daemon-owned fans to a known path (e.g., `/var/lib/kde-fan-control/enrolled.json`) so the recovery tool can find it
- Test: start daemon, enroll a fan, `kill -9` the daemon, verify fans go to safe-max
- Test: `TimeoutStartSec=` expiry during a wedged startup, verify `ExecStopPost=` runs

**Which phase should address it:** Phase implementing systemd unit — this IS the safety contract

---

### Pitfall 5: `.deb` conffile handling silently overwrites daemon config on upgrade

**What goes wrong:** The daemon's TOML config file is installed as a conffile in `/etc/kde-fan-control/config.toml`. On package upgrade, `dpkg` sees the config as a conffile and either silently installs the new version (if the user never edited it) or prompts with a confusing diff. If the user accepts the maintainer version, their enrolled fan configuration is wiped. If they reject it, the new config schema changes may not take effect.

**Why it happens:** Debian's conffile mechanism is designed for files the admin edits, but a daemon-owned active config has different semantics — it's BOTH the package's default template AND a user-modified live document. The `dpkg` conffile prompt on upgrade is confusing for desktop users and can lead to data loss either way.

**Consequences:** Users lose their fan configuration on package upgrades. Or, upgrades fail because config schema changed and the old config won't parse. Support burden and user distrust.

**Warning signs:**
- Config installed to `/etc/` and marked as a conffile
- No config migration code in `preinst`/`postinst` maintainer scripts
- No version field in the config schema
- Package upgrade tests only check install/remove, not upgrade

**Prevention strategy:**
- Do NOT mark the daemon's live config as a dpkg conffile
- Install a default/template config to `/usr/share/kde-fan-control/config.default.toml` and let the daemon copy it to its runtime location on first start
- Use `/var/lib/kde-fan-control/` for the active config and `/etc/kde-fan-control/` only for admin overrides (if any)
- Include a config schema version field from day one: `config_version = 1`
- Write a `preinst` or `postinst` migration that can transform v1 → v2 config if schema changes
- Test upgrade path: install old .deb, create config, upgrade to new .deb, verify config preserved

**Which phase should address it:** Phase implementing .deb packaging — config handling is a packaging design decision

---

### Pitfall 6: DBus policy file install location confusion (`/etc/` vs `/usr/share/`)

**What goes wrong:** The DBus policy `.conf` file is installed to `/etc/dbus-1/system.d/` by the .deb package. But on many modern systems, `dbus-daemon` loads policies from `/usr/share/dbus-1/system.d/` first, and `/etc/dbus-1/system.d/` is reserved for admin overrides. If a distro or a future admin also installs a policy to `/usr/share/`, the two files conflict or the wrong one wins. Worse, the existing file at `/etc/dbus-1/system.d/org.kde.FanControl.conf` is already present (from manual install), and a .deb package installing to the same path creates a conffile conflict.

**Why it happens:** The DBus spec and `dbus-daemon` implementation have evolved their policy search paths. Modern `dbus-daemon 1.12+` loads from both `_/usr/share/dbus-1/system.d/_` (packaged) and `/etc/dbus-1/system.d/` (admin), with `/etc/` winning on conflicts. But many .deb packages still install to `/etc/` because that's the older convention. The project's existing manual DBus config is already in `/etc/dbus-1/system.d/`.

**Consequences:** Duplicate or conflicting policy files. Package upgrade conflicts with manually installed files. Admin customizations overwritten. DBus policy behavior unpredictable because load order varies.

**Warning signs:**
- Package installs to `/etc/dbus-1/system.d/` but the reference convention is `/usr/share/dbus-1/system.d/`
- Existing manual file at `/etc/dbus-1/system.d/org.kde.FanControl.conf` from pre-packaging era
- No `dpkg` diversions or handling for existing manual config files
- Corectrl (closest analog) installs to `/usr/share/dbus-1/system.d/`

**Prevention strategy:**
- Install DBus `.conf` policy to `/usr/share/dbus-1/system.d/org.kde.FanControl.conf` — this is the packaged-policy location (confirmed by examining corectrl, power-profiles-daemon, NetworkManager packages on this system)
- Reserve `/etc/dbus-1/system.d/` for admin overrides
- In `preinst` or `postinst`, check for and remove or redirect any previously manually-installed `/etc/dbus-1/system.d/org.kde.FanControl.conf` to avoid conflicts
- This matches what corectrl does on this system: `/usr/share/dbus-1/system.d/org.corectrl.helper.conf`

**Which phase should address it:** Phase implementing DBus policy installation + .deb packaging

---

### Pitfall 7: No `enable` in postinst — daemon must be manually started after install

**What goes wrong:** The .deb package installs the systemd unit but doesn't call `systemctl enable` or `systemctl start` in `postinst`. After install, the daemon doesn't run. The GUI launches, can't reach the daemon, and shows an error. Users have to manually figure out they need `sudo systemctl enable --now kde-fan-control-daemon.service`.

**Why it happens:** Debian policy historically discourages auto-enabling services in `postinst`. The `dh_installsystemd` helper with default debhelper compatibility level does NOT enable services by default. But for a desktop fan control app, the daemon needs to be running for the product to work at all.

**Consequences:** Broken out-of-box experience. Users install the package and the app doesn't work. Support burden. Users may try to start the daemon manually with `pkexec` or `sudo` instead of through systemctl.

**Warning signs:**
- Package uses `dh_installsystemd` with default compat level (no auto-enable)
- No `deb-systemd-invoke enable` in `postinst`
- No documentation telling users to enable the service
- GUI has no on-demand daemon start facility

**Prevention strategy:**
- Decide explicitly: should the service auto-enable after install? For a fan-control daemon that owns safety-critical hardware, auto-enable is justified
- If auto-enabling, use debhelper compat level 13+ which supports `dh_installsystemd --enable` and call `deb-systemd-invoke enable <unit>` in `postinst`
- As a fallback, the GUI should detect "daemon not reachable" and offer a one-click polkit-gated "Start Daemon" button that calls `systemctl start` via polkit
- Document the `systemctl enable --now` step prominently in README and GUI help
- The `install.sh` fallback should always call `systemctl enable --now`

**Which phase should address it:** Phase implementing .deb packaging + GUI on-demand start

---

## Moderate Pitfalls

### Pitfall 8: `.desktop` file missing `StartupNotify=false` and tray icon behavior

**What goes wrong:** The `.desktop` file for the GUI doesn't set `StartupNotify=false`. When users launch the app, their desktop environment shows a launching spinner/cursor for 5-10 seconds. Since the GUI is a tray app that may start minimized, the spinner is confusing. Or, the `.desktop` file is placed in `/usr/share/applications/` but there's no autostart entry, so the tray app doesn't start on login.

**Why it happens:** `.desktop` files have many keywords and `StartupNotify` is easy to overlook. Autostart requires a separate `.desktop` file in `/etc/xdg/autostart/` with `X-GNOME-Autostart-enabled=true` or KDE's `X-KDE-autostart-after=panel` directives.

**Prevention strategy:**
- Set `StartupNotify=false` in the GUI `.desktop` file
- Set `Terminal=false` and `Type=Application`
- Use `Categories=System;Settings;` and appropriate `Keywords=`
- For autostart, install a separate `org.kde.FanControl.desktop` to `/etc/xdg/autostart/` with `Hidden=false` and a `NotShowIn=` if needed for non-KDE environments
- Embed icons via the icon naming spec (`Icon=org.kde.FanControl`) and install SVG icons to `/usr/share/icons/hicolor/scalable/apps/`

**Which phase should address it:** Phase implementing .desktop files

---

### Pitfall 9: CLI binary not in PATH or conflicts with existing `fancontrol` binary

**What goes wrong:** The CLI binary (`kfc`) is compiled as a Rust binary but not placed in a PATH-visible location. Or it's installed to `/usr/local/bin/` which isn't on all users' PATH by default. Or, the binary naming conflicts with an existing tool.

**Why it happens:** Rust's `cargo install` puts binaries in `~/.cargo/bin/` by default, which is not in the system PATH. Debian packages should install to `/usr/bin/`. The name `kfc` is short and could theoretically collide with another tool.

**Prevention strategy:**
- Install CLI binary to `/usr/bin/kfc` in the .deb
- Install daemon binary to `/usr/libexec/kde-fan-control/kde-fan-control-daemon` (matching the pattern used by corectrl, UDisks2, power-profiles-daemon — daemons that are not user-facing go in `/usr/libexec/`)
- Never install the daemon to `/usr/bin/` — it should not be in users' PATH since it's managed by systemd
- Add a `Provides:` and `Conflicts:` field in the .deb if `kfc` collides with anything
- The `install.sh` should also use these paths

**Which phase should address it:** Phase implementing .deb packaging

---

### Pitfall 10: `install.sh` fallback runs as root without proper guards

**What goes wrong:** The `install.sh` script is run with `sudo` but doesn't check for critical prerequisites (kernel hwmon support, udev rules, dbus daemon running, polkit installed). It succeeds partially and leaves the system in a broken state: unit installed but dbus policy missing, or binary installed but not executable, or systemd reloaded but the unit has errors.

**Why it happens:** Shell scripts for manual install tend to be written as "best effort" without transactional semantics. Partial install states are hard to recover from.

**Prevention strategy:**
- Start `install.sh` with prerequisite checks: `command -v systemctl`, `command -v dbus-daemon`, `command -v pkexec`, verify `/sys/class/hwmon` exists
- Make the script idempotent: running it twice should produce the same result
- If any step fails, print a clear diagnostic and exit non-zero
- Provide an `uninstall.sh` that cleanly reverses all changes
- Keep `install.sh` in sync with the .deb file list — the same files should be installed in the same locations
- Do NOT use `set -e` blindly — some commands like `systemctl reload` may fail non-fatally in edge cases

**Which phase should address it:** Phase implementing install.sh

---

### Pitfall 11: DBus interface methods rejected by overly restrictive policy `.conf`

**What goes wrong:** The DBus `.conf` policy file grants `send_destination` permission for the bus name, but some DBus daemons enforce policy at the interface level. If the policy only allows `send_destination="org.kde.FanControl"` without specifying interfaces, it works on most systems. But if the policy is too restrictive (e.g., only allowing specific interfaces), new polkit-related calls or properties methods may be blocked.

**Why it happens:** PowerProfiles-daemon's `.conf` explicitly allows `send_interface=net.hadess.PowerProfiles` plus the standard `Introspectable/Properties/Peer` interfaces. A copy-paste of that pattern with missing interface names would block access to new interfaces.

**Prevention strategy:**
- Use the permissive pattern: `<allow send_destination="org.kde.FanControl"/>` in the default policy — this allows all interfaces on that destination, which is appropriate for a v1 app
- Do NOT try to enumerate specific interfaces unless you have a strong security reason (interface-level filtering is for high-security contexts, not a desktop fan control app)
- Always allow the standard interfaces: `org.freedesktop.DBus.Introspectable`, `org.freedesktop.DBus.Properties`, `org.freedesktop.DBus.Peer`
- Root policy should `<allow own="org.kde.FanControl"/>` plus `<allow send_destination="org.kde.FanControl"/>`

**Which phase should address it:** Phase implementing DBus policy installation

---

### Pitfall 12: zbus service name race condition on bus activation

**What goes wrong:** When the daemon is DBus-activated (started on demand because a client called a method on its bus name), the name acquisition and interface setup can race. If the name is requested before interfaces are served, incoming method calls arrive before handlers are registered, and the calls fail with "Method not found" errors.

**Why it happens:** This is a documented zbus pitfall. The zbus book explicitly warns: "When using ObjectServer, it is crucial to request the service name after setting up your interface handlers. If the service name is requested before the handlers are set up, incoming D-Bus messages might be lost."

**Consequences:** Intermittent GUI failures on first launch when daemon starts via bus activation. "Method not found" errors that disappear on retry.

**Warning signs:**
- Daemon works when started via `systemctl start` but sometimes fails on bus activation
- GUI sees "No such method" on first call after cold start
- No `connection::Builder` pattern used for service setup

**Prevention strategy:**
- The existing code already uses `connection::Builder` with `.name().serve_at().build()` — this is the correct pattern
- Verify that the builder chain completes before the event loop starts processing messages
- Do NOT call `request_name()` separately after building the connection
- Test specifically: stop the daemon, launch the GUI, verify the first DBus call succeeds without error

**Which phase should address it:** Already handled in existing code — but re-verify during packaging after adding `SystemdService=` activation

---

### Pitfall 13: Polkit `auth_admin_keep` doesn't work headless / no authentication agent

**What goes wrong:** The polkit default for write actions is `auth_admin_keep`, which triggers an authentication dialog. But if the user is in an SSH session, running under `sudo`, or using a minimal window manager without a polkit authentication agent, there's no agent to handle the prompt. The authorization check times out, and the operation fails with a confusing error.

**Why it happens:** Polkit requires an authentication agent per session. KDE Plasma provides one (through `_polkitagent`), but SSH sessions, bare i3/sway, or minimal environments may not.

**Prevention strategy:**
- Set `allow_any=no` for write operations (non-local, non-active sessions denied)
- Set `allow_inactive=no` for inactive sessions (e.g., SSH)
- Set `allow_active=auth_admin_keep` for active local sessions with an agent
- In the daemon, if `CheckAuthorization` fails with `org.freedesktop.PolicyKit1.Error.Failed` or similar, check if the caller is UID 0 and allow it directly (root bypass)
- Document that SSH/CLI users should use `sudo kfc <command>` for write operations
- Consider a `--allow-root` or `--no-polkit` CLI flag for scriptable use

**Which phase should address it:** Phase implementing polkit

---

### Pitfall 14: `systemctl daemon-reload` not called after install or upgrade

**What goes wrong:** The .deb installs or modifies a systemd unit file but doesn't trigger `systemctl daemon-reload`. systemd still has the old unit file cached. New changes (like adding `WatchdogSec=` or changing `ExecStart=`) don't take effect until the next reboot or manual `daemon-reload`. On upgrade, this means the old service definition (possibly with broken paths) runs.

**Why it happens:** Debian's `dh_installsystemd` normally handles this, but custom packaging or `install.sh` may forget it.

**Prevention strategy:**
- Use `dh_installsystemd` in `debian/rules` — it handles `daemon-reload` automatically
- In `install.sh`, call `systemctl daemon-reload` after installing unit files
- In `postinst` maintainer script (if not using debhelper), explicitly call `deb-systemd-invoke daemon-reload` or `systemctl daemon-reload`
- Test: install the .deb, verify `systemctl cat kde-fan-control-daemon.service` shows the new version immediately

**Which phase should address it:** Phase implementing .deb packaging

---

### Pitfall 15: Icon not found — `.desktop` file references icon that doesn't exist

**What goes wrong:** The `.desktop` file has `Icon=org.kde.FanControl` but no icon is installed at the expected XDG icon theme path. On KDE, the icon shows as a generic placeholder or broken image. The tray icon may fail entirely if KStatusNotifierItem can't find the icon.

**Why it happens:** Icon installation requires placing correctly-named SVG or PNG files into `/usr/share/icons/hicolor/<size>/apps/` and then running `gtk-update-icon-cache` (or relying on KDE's icon cache). If the icon is embedded in a QML resource but not installed as a theme icon, it won't be found by the `.desktop` file or tray icon.

**Prevention strategy:**
- Install the application icon as an SVG to `/usr/share/icons/hicolor/scalable/apps/org.kde.FanControl.svg`
- Also install PNG fallbacks for common sizes (16, 22, 32, 48, 64, 128) to `/usr/share/icons/hicolor/<size>x<size>/apps/org.kde.FanControl.png`
- Run `gtk-update-icon-cache -f /usr/share/icons/hicolor/` in `postinst` and `postrm`
- For the tray icon specifically: test that `KStatusNotifierItem` can find the icon by name — it may need the full path if the icon name is not in the theme
- The `.desktop` file `Icon=` value should be just the icon name without extension: `Icon=org.kde.FanControl`

**Which phase should address it:** Phase implementing .desktop files + packaging

---

## Minor Pitfalls

### Pitfall 16: `NotifyAccess=all` vs `NotifyAccess=main` for sd-notify

**What goes wrong:** The service unit sets `NotifyAccess=main` (or leaves it as the default), but the Rust daemon's notification socket is inherited by child tasks or subprocesses. If the watchdog ping comes from a non-main process, systemd ignores it and the watchdog triggers, killing the service.

**Prevention strategy:**
- Use `NotifyAccess=main` (the correct, most restrictive setting) and ensure all `sd_notify` / `sd-notify` calls come from the main process
- The `sd-notify` crate should be called from the Tokio main task, not spawned tasks
- Test with `WatchdogSec=` enabled and verify the daemon stays alive for >60 seconds

**Which phase should address it:** Phase implementing systemd + sd-notify

---

### Pitfall 17: `.deb` architecture mismatch — native Rust binary targets wrong arch

**What goes wrong:** The .deb is built on an `x86_64` host but the `Cargo.toml` target or `CMake` build doesn't match the packaging architecture. Or the .deb declares `Architecture: all` when it contains compiled binaries that are arch-dependent.

**Prevention strategy:**
- Use `Architecture: amd64` (or the appropriate arch) for the .deb containing compiled Rust + C++ binaries
- Use `dpkg --print-architecture` to detect the target arch during build
- Cross-compilation is out of scope for v1 — build on the target architecture
- The `install.sh` should detect architecture mismatch and warn

**Which phase should address it:** Phase implementing .deb packaging

---

### Pitfall 18: Missing `postinst` trigger for DBus policy reload

**What goes wrong:** After installing the DBus `.conf` file, the DBus daemon doesn't pick it up until restart. Some systems auto-reload; some don't. The daemon can't own its bus name because the policy isn't loaded yet.

**Prevention strategy:**
- Modern `dbus-daemon` (1.12+) watches policy directories and auto-reloads changes, but don't rely on this
- In `postinst`, call `dbus-send --system --type=method_call --dest=org.freedesktop.DBus / org.freedesktop.DBus.ReloadConfig` or rely on `dh_install` + `dbus` trigger
- Debian's `dbus` package provides a dpkg trigger that reloads policies automatically — ensure the .deb uses `dh_dbus` or declares the trigger dependency

**Which phase should address it:** Phase implementing .deb packaging

---

### Pitfall 19: GUI on-demand daemon start doesn't wait for readiness

**What goes wrong:** The GUI detects the daemon isn't running and calls `systemctl start` via polkit. But then it immediately tries to connect to the DBus name. The daemon hasn't finished startup yet (hwmon scan, config load). The GUI gets connection errors and shows a confusing "daemon not reachable" message even though it just asked for it to start.

**Why it happens:** With `Type=notify`, the systemd unit isn't "active" until the daemon sends `READY=1`. But the GUI may poll faster than the daemon starts.

**Prevention strategy:**
- After triggering `systemctl start`, wait for the systemd unit to reach "active" state (use `systemctl is-active --wait` or poll `systemctl is-active`)
- Then, wait for the DBus name to appear (use `dbus-monitor` pattern or Qt's name ownership tracking)
- Show a "Starting daemon..." status in the GUI during this period
- Set a reasonable timeout (e.g., 30s) before showing a failure message
- The existing `StatusMonitor::checkDaemonConnected()` polling loop is the right foundation — reuse it with a "waiting for daemon start" state

**Which phase should address it:** Phase implementing GUI on-demand start

---

### Pitfall 20: `install.sh` and .deb install the same files to different paths

**What goes wrong:** The `install.sh` installs the daemon binary to `/usr/local/bin/` while the .deb installs to `/usr/libexec/kde-fan-control/`. The systemd unit in `install.sh` points to `/usr/local/bin/` but the .deb version points to `/usr/libexec/`. If a user runs both, they get conflicting installations and unpredictable behavior.

**Why it happens:** `install.sh` is often written first with simpler paths, and .deb packaging applies FHS conventions later. No one reconciles the two.

**Prevention strategy:**
- Both `install.sh` and .deb MUST install to the same paths
- Define the canonical file layout once and use it everywhere
- Recommended layout:
  - Daemon: `/usr/libexec/kde-fan-control/kde-fan-control-daemon`
  - CLI: `/usr/bin/kfc`
  - GUI: `/usr/bin/kde-fan-control`
  - Config: `/var/lib/kde-fan-control/config.toml`
  - Recovery helper: `/usr/libexec/kde-fan-control/kde-fan-control-recover`
  - Systemd unit: `/lib/systemd/system/kde-fan-control-daemon.service`
  - DBus policy: `/usr/share/dbus-1/system.d/org.kde.FanControl.conf`
  - DBus service: `/usr/share/dbus-1/system-services/org.kde.FanControl.service`
  - Polkit policy: `/usr/share/polkit-1/actions/org.kde.FanControl.policy`
  - Desktop file: `/usr/share/applications/org.kde.FanControl.desktop`
  - Autostart: `/etc/xdg/autostart/org.kde.FanControl.desktop`
  - Icons: `/usr/share/icons/hicolor/scalable/apps/org.kde.FanControl.svg`
  - KNotification config: `/usr/share/knotifications5/org.kde.fancontrol.notifyrc`

**Which phase should address it:** First phase defining the packaging layout — before either install.sh or .deb is implemented

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| File layout design | `install.sh` and .deb install to different paths | Define canonical layout once, use everywhere |
| systemd unit | `ProtectSystem=strict` blocks hwmon writes | Test real hwmon writes; use `ReadWritePaths=/sys/class/hwmon`; don't use `PrivateDevices=yes` |
| systemd unit | `ExecStop=` skipped on crash | Add `ExecStopPost=` with standalone recovery helper |
| systemd unit | `Type=notify` but no sd_notify READY=1 signal sent | Wire `sd-notify` crate into daemon startup completion |
| systemd + DBus | Bus activation file `SystemdService=` name mismatch | Match exactly to installed unit filename; test GUI launch without daemon running |
| polkit policy | Policy file installed but authorization check still UID==0 | Replace `require_authorized()` body with `CheckAuthorization` call |
| polkit policy | No auth agent available in SSH/headless | Allow root bypass; document CLI `sudo` usage |
| .deb packaging | Config file treated as conffile, lost on upgrade | Don't conffile the daemon config; use runtime copy-from-template |
| .deb packaging | Service not enabled after install | Enable in `postinst` or provide GUI "Start Daemon" button |
| .deb packaging | `daemon-reload` not triggered | Use `dh_installsystemd` or explicit `systemctl daemon-reload` in `postinst` |
| DBus policy | Install to `/etc/` instead of `/usr/share/` | Use `/usr/share/dbus-1/system.d/` for packaged policies |
| .desktop file | No `StartupNotify=false`, no autostart entry | Set both; install autostart `.desktop` to `/etc/xdg/autostart/` |
| .desktop file | Icon theme path mismatch | Install SVG to hicolor theme dirs; run `gtk-update-icon-cache` |
| CLI placement | Binary in wrong PATH location | CLI → `/usr/bin/`, daemon → `/usr/libexec/` |
| GUI on-demand start | Race between `systemctl start` and first DBus call | Wait for unit active + DBus name appearance before connecting |

## Recommended Research Flags for Roadmap

- **Must research deeply before implementation:** systemd hardening vs hwmon access matrix, polkit CheckAuthorization wiring in zbus, DBus bus activation sequence of operations
- **Can implement with standard patterns:** .desktop file content, icon installation, FHS file layout, postinst/postrm maintainer scripts
- **Should be validated with real hardware:** ExecStopPost recovery behavior, hwmon write paths under different ProtectSystem settings, watchdog + sd_notify timing

## Sources

### HIGH confidence
- systemd service semantics and ExecStop/ExecStopPost distinction: https://man7.org/linux/man-pages/man5/systemd.service.5.html
- systemd hardening directives and their interaction with filesystem access: https://man7.org/linux/man-pages/man5/systemd.exec.5.html
- D-Bus specification, service activation, and policy search paths: https://dbus.freedesktop.org/doc/dbus-specification.html
- polkit architecture, action declaration, and authorization check flow: https://polkit.pages.freedesktop.org/polkit/polkit.8.html
- zbus service activation pitfall documentation: Context7 `/dbus2/zbus` — book section on service activation
- Linux kernel hwmon sysfs interface: https://www.kernel.org/doc/html/latest/hwmon/sysfs-interface.html
- Real system reference: fancontrol `.deb` package layout, corectrl DBus/polkit/systemd integration, power-profiles-daemon systemd/DBus patterns, UDisks2 bus activation patterns (all examined on this system)

### MEDIUM confidence
- DBus policy file location convention evolution (`/etc/` → `/usr/share/`): based on examining real packages on this system (corectrl uses `/usr/share/`, NetworkManager uses `/usr/share/`, some older packages still in `/etc/`)
- Debian dh_installsystemd auto-enable behavior: based on debhelper documentation patterns and common .deb packaging practice
- `gtk-update-icon-cache` and icon theme path requirements: based on freedesktop icon naming spec and KDE packaging conventions

### LOW confidence
- Exact `sd-notify` crate behavior with Tokio watchdog: based on crate docs and common patterns, not tested against this specific daemon