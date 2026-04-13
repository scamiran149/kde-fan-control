# KDE Fan Control

Linux desktop fan control for machines where `fancontrol` is too rigid or cumbersome.

Users can safely and flexibly control desktop fan behavior with understandable per-fan PID policies, without losing fail-safe behavior.

## Features

- **Hardware inventory** from Linux kernel hwmon sysfs (`/sys/class/hwmon/hwmon*`)
- **Per-fan enrollment** with draft/apply lifecycle — changes are staged, validated, then promoted
- **Temperature-target PID control** — fans track a temperature setpoint, not an RPM target
- **Sensor aggregation** — average, max, min, or median across multiple temperature inputs
- **Auto-tune** with bounded observation window and softened proposals; review before accepting
- **Safe-maximum fallback** — controlled fans fail to PWM 255 on any daemon failure (panic, crash, graceful shutdown)
- **Boot reconciliation** — managed fans auto-resume after reboot
- **Read-open / write-privileged** DBus access (UID-0 for writes; polkit planned)
- **Friendly names** for sensors and fans
- **KDE-native GUI** with wizard dialog, fan detail pages, system tray, and desktop notifications

## Architecture

```
                      +-----------------+
                      |  KDE/Qt6 GUI   |
                      | (Kirigami/QML) |
                      +--------+--------+
                               |
                          DBus (system bus)
                               |
+----------------+    +--------+--------+    +-----------------+
|  Linux kernel  |    |   Rust daemon   |    |   Rust CLI      |
|  hwmon sysfs   +--->|   (root)        +--->|   (kde-fan-     |
| /sys/class/    |    |  - PID loops    |    |    control)     |
|  hwmon/hwmon*  |    |  - DBus server  |    |                 |
+----------------+    |  - config owner  |    +-----------------+
                      +--------+--------+
                               |
                          DBus
                               |
                      +--------+--------+
                      |  System tray   |
                      | (KStatus-      |
                      |  NotifierItem) |
                      +-----------------+
```

**DBus bus name:** `org.kde.FanControl`

| Path | Interface | Purpose |
|---|---|---|
| `/org/kde/FanControl` | `org.kde.FanControl.Inventory` | Hardware discovery, naming |
| `/org/kde/FanControl/Lifecycle` | `org.kde.FanControl.Lifecycle` | Draft/apply config, runtime state |
| `/org/kde/FanControl/Control` | `org.kde.FanControl.Control` | PID control, auto-tune |

## Quick Start

### Prerequisites

- Rust toolchain (stable, edition 2024)
- Qt 6.8+ development packages (Core, Quick, Qml, QuickControls2, DBus, Widgets)
- KDE Frameworks 6 packages (Kirigami, StatusNotifierItem, Notifications, I18n)
- CMake 3.20+
- `libclang` for Rust bindgen (if building with udev crate)

### Build

```sh
# Rust daemon + CLI
cargo build --release

# KDE GUI
cmake -B gui/build -S gui
cmake --build gui/build
```

### Run

```sh
# Daemon (requires root, system bus by default)
sudo ./target/release/kde-fan-control-daemon

# Daemon on session bus (development)
./target/release/kde-fan-control-daemon --session-bus

# GUI
./gui/build/gui_app

# CLI
./target/release/kde-fan-control inventory
./target/release/kde-fan-control state
```

## CLI Quick Reference

| Command | Description |
|---|---|
| `inventory` | List detected sensors and fans from hwmon sysfs |
| `rename <id> <name>` | Assign a friendly name to a sensor (use `--fan` for fans) |
| `unname <id>` | Remove a friendly name |
| `draft` | Show the current draft (staged) configuration |
| `applied` | Show the current applied (live) configuration |
| `degraded` | Show degraded fan summary with reasons |
| `events` | Show recent lifecycle events |
| `enroll <fan_id>` | Stage a fan enrollment in the draft config |
| `unenroll <fan_id>` | Remove a fan from the draft config |
| `discard` | Discard the entire draft configuration |
| `validate` | Validate the draft without applying it |
| `apply` | Promote the draft configuration to live |
| `state` | Show runtime state of all fans |
| `control set <fan_id>` | Stage PID control profile changes for a managed fan |
| `auto-tune start <fan_id>` | Start a bounded auto-tune run |
| `auto-tune result <fan_id>` | Inspect the latest auto-tune result |
| `auto-tune accept <fan_id>` | Accept the latest auto-tune proposal into draft |

Most commands accept `--format json` for machine-readable output and `--detail` for expanded information.

## Configuration Basics

**Config location:** `$XDG_STATE_DIR/kde-fan-control/config.toml` (typically `~/.local/state/kde-fan-control/config.toml`)

The daemon owns the configuration. Clients never write the file directly — all changes go through the draft/apply lifecycle:

1. `enroll` or `control set` stages changes in the **draft**
2. `validate` checks the draft without side effects
3. `apply` atomically promotes the draft to the **applied** (live) configuration
4. `discard` throws away the draft if you change your mind

Example workflow:

```sh
sudo kde-fan-control enroll hwmon1/pwm2 --managed --control-mode pwm --temp-sources hwmon1/temp1,hwmon1/temp2
sudo kde-fan-control control set hwmon1/pwm2 --target-temp 60 --aggregation max --kp 2.0 --ki 0.5 --kd 1.0 --sample-ms 1000 --control-ms 2000 --write-ms 2000
sudo kde-fan-control validate
sudo kde-fan-control apply
```

## Safety Model

Controlled fans **always** fail to high speed (PWM 255):

- A Rust panic hook writes safe-maximum before the process exits
- Graceful shutdown and crash paths both trigger fallback
- Boot reconciliation restores managed fans automatically after reboot
- BIOS-managed fans are never touched unless explicitly enrolled
- Fallback incidents are persisted and inspectable via `degraded` and `events`

See [docs/safety-model.md](docs/safety-model.md) for full details.

## Documentation

| Document | Content |
|---|---|
| [docs/safety-model.md](docs/safety-model.md) | Fallback behavior, panic hook, boot reconciliation |
| [docs/dbus-api.md](docs/dbus-api.md) | DBus interface contract |
| [docs/configuration.md](docs/configuration.md) | Config file format, draft/apply model, examples |
| [docs/architecture.md](docs/architecture.md) | Internal structure, control loops, data flow |

## Project Status

**v1.0 MVP shipped** (2026-04-12). Functional daemon, CLI, and KDE GUI with system tray. See the commit log for recent changes.

## License

GPL-3.0-or-later