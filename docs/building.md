# Building KDE Fan Control

Build and development guide for contributors and packagers.

## Build prerequisites

**Rust:**
- Rust stable 1.94.1+ (edition 2024)
- Install via rustup: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

**Qt6 and KDE Frameworks (Debian/Ubuntu packages):**
- `qt6-base-dev`, `qt6-declarative-dev`, `qt6-tools-dev`
- `libqt6dbus6-dev` (Qt DBus)
- `libkf6kirigami-dev` (Kirigami QML components)
- `libkf6statusnotifieritem-dev` (system tray)
- `libkf6notifications-dev` (desktop notifications)
- `libkf6i18n-dev` (internationalization)
- `libkf6iconthemes-dev` (optional, icon theme support)
- `cmake` (3.20+)
- `extra-cmake-modules` (ECM)

**Fedora packages:**
- `qt6-qtbase-devel`, `qt6-qtdeclarative-devel`, `qt6-qttools-devel`
- `kf6-kirigami-devel`, `kf6-kstatusnotifieritem-devel`, `kf6-knotifications-devel`, `kf6-ki18n-devel`
- `cmake`, `extra-cmake-modules`

**Build essentials:**
- `build-essential` (Debian) or `gcc-c++` (Fedora)
- `pkg-config`

**Optional:**
- `libclang-dev` — needed if the `udev` crate is enabled (for hardware hotplug detection)

## Workspace structure

```
kde-fan-control/
├── crates/
│   ├── core/          # Shared types: inventory, config, control, lifecycle
│   ├── daemon/        # Root-privileged DBus system service
│   └── cli/           # Command-line interface (DBus client)
├── gui/
│   ├── src/           # C++ backend (DaemonInterface, models, tray, notifications)
│   ├── qml/           # QML UI pages, components, delegates
│   ├── data/          # kdefancontrol.notifyrc
│   └── CMakeLists.txt
├── packaging/
│   ├── dbus/          # DBus system bus policy + activation
│   ├── systemd/       # systemd unit file
│   └── polkit/        # polkit authorization policy
└── Cargo.toml         # Rust workspace root
```

## Building the Rust workspace

```bash
# Debug build (faster compile, includes debug symbols)
cargo build

# Release build (optimized, for deployment)
cargo build --release

# Run tests
cargo test

# Run with verbose test output
cargo test -- --nocapture
```

The workspace produces 4 binaries:
- `target/debug/kde-fan-control-daemon` (or `target/release/`)
- `target/debug/kde-fan-control` (CLI)
- `target/debug/kde-fan-control-fallback` (ExecStopPost helper)
- Core crate is a library — no standalone binary

## Building the GUI

```bash
# Configure (out-of-source build required)
cmake -B gui/build -S gui

# Build
cmake --build gui/build

# Run
./gui/build/kde-fan-control-gui
```

### GUI build troubleshooting

- **Kirigami not found at CMake time:** This is normal. Kirigami is a QML-only module with no C++ link target. The build will succeed; Kirigami is resolved at QML runtime from the system QML plugin path.
- **KF6 dev packages not found:** Some distros package KF6 CMake configs differently. If `find_package` fails for KNotifications or KStatusNotifierItem, install the `-devel` packages or point CMake to the config files.
- **QML module not found at runtime:** Ensure `QML2_IMPORT_PATH` includes the Kirigami QML plugin directory.

## Running for development

### Local developer install

If you want the system bus service, polkit prompt, desktop launcher, and icon to behave like an installed app during local testing, use the helper script:

```bash
cargo build --release
cmake -B gui/build -S gui
cmake --build gui/build
sudo ./scripts/dev-install.sh install --release
sudo systemctl enable --now kde-fan-control-daemon
```

This installs the packaging assets into `/usr/local`, writes a dev-only systemd unit into `/etc/systemd/system`, and copies the current build outputs into `/usr/local/bin` and `/usr/local/libexec`. Remove everything with:

```bash
sudo ./scripts/dev-install.sh uninstall
```

See [docs/developer-install.md](developer-install.md) for the installed paths.

### Daemon (session bus)

The daemon defaults to the system bus, which requires root. For development, use the session bus:

```bash
# Terminal 1: Start daemon on session bus
./target/debug/kde-fan-control-daemon --session-bus

# Terminal 2: Run CLI (auto-detects session bus fallback)
./target/debug/kde-fan-control inventory

# Terminal 3: Run GUI (as regular user, NOT root)
./gui/build/gui-app
```

The GUI must run as a regular user — not root/sudo. KStatusNotifierItem
(the system tray icon) registers on the user's DBus session bus; running as
root connects to a different session bus where the Plasma tray is not
listening. Privileged write operations are handled via polkit: click the
"Unlock" toolbar button to authenticate.

The daemon's session-bus mode is a development convenience. Production use always targets the system bus.

### Daemon (system bus, requires root)

```bash
# Install DBus policy first (one-time)
sudo cp packaging/dbus/org.kde.FanControl.conf /usr/share/dbus-1/system.d/

# Install DBus activation (one-time, delegates to systemd)
sudo cp packaging/dbus/org.kde.FanControl.service /usr/share/dbus-1/system-services/

# Install polkit policy (one-time)
sudo cp packaging/polkit/org.kde.fancontrol.policy /usr/share/polkit-1/actions/

# Install systemd unit (one-time)
sudo cp packaging/systemd/kde-fan-control-daemon.service /usr/lib/systemd/system/
sudo systemctl daemon-reload

# Option A: Start via systemd (recommended)
sudo systemctl enable --now kde-fan-control-daemon

# Option B: Start manually
sudo ./target/release/kde-fan-control-daemon
```

With systemd, the daemon sends `READY=1` after DBus name acquisition and
`WATCHDOG=1` every 20 seconds. The `ExecStopPost=` fallback helper forces
PWM 255 for all owned fans even on crash/SIGKILL.

### Direct inventory scan (no daemon)

The CLI can scan hardware directly without the daemon:

```bash
./target/debug/kde-fan-control inventory --direct
./target/debug/kde-fan-control inventory --root /path/to/fake/sysfs
```

## Configuration

**Location:** `$XDG_STATE_DIR/kde-fan-control/config.toml`
- Falls back to `$XDG_DATA_HOME/kde-fan-control/config.toml`
- Then to `/var/lib/kde-fan-control/config.toml` if neither is set

The daemon creates the config file on first run with default values.

**Debug logging:**

```bash
RUST_LOG=kde_fan_control=debug ./target/debug/kde-fan-control-daemon --session-bus
```

Standard `tracing` filter syntax: `RUST_LOG=kde_fan_control::control=trace,kde_fan_control::inventory=info`

## Testing

```bash
# All Rust tests
cargo test

# Daemon tests only
cargo test -p kde-fan-control-daemon

# Core crate tests only
cargo test -p kde-fan-control-core

# Specific test
cargo test -p kde-fan-control-daemon control_supervisor_runs

# GUI tests (if ctest configured)
cd gui/build && ctest
```

Test fixtures create temporary directories under `/tmp/` with fake sysfs structures (hwmon files, pwm nodes). Each fixture gets a unique directory name with PID + timestamp + counter to avoid conflicts. Fixtures clean up on drop.

The daemon tests use `tokio::test(flavor = "current_thread")` to avoid needing a multi-threaded runtime. Control task tests typically sleep 40-80ms to let sample/control/write intervals fire at least once.

## Known technical debt to work around

1. **OverviewModel `beginResetModel`:** The overview model uses `beginResetModel()` on structural changes instead of the more granular `beginMoveRows`. This is acceptable because structural refreshes are rare (2000ms cooldown-gated), but could be optimized later for very large fan counts.

2. **KF6 CMake packages:** Some KF6 dev packages don't provide standard CMake configs. If the build can't find them, install the development headers and the shared libraries separately.

3. **GUI navigation stubs:** The tray popover "Open Fan Control" button and tray icon click-to-main-window aren't fully wired yet. These are cosmetic issues, not functional blockers.
