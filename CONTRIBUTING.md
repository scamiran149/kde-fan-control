# Contributing to KDE Fan Control

## Project structure

The project has 3 Rust crates + a Qt/KDE GUI:

| Path | Language | Purpose |
|------|----------|---------|
| `crates/core/` | Rust | Shared types: inventory discovery, config/validation, PID control, lifecycle/ownership |
| `crates/daemon/` | Rust | Root-privileged DBus system service |
| `crates/cli/` | Rust | Command-line interface (thin DBus client) |
| `gui/src/` | C++ | GUI backend: DBus proxy, data models, tray, notifications |
| `gui/qml/` | QML | UI pages, components, delegates |
| `gui/data/` | Data | KNotification event config |

## Code style

### Rust

- Follow `rustfmt` defaults. Run `cargo fmt` before committing.
- Run `cargo clippy` and fix warnings.
- Edition 2024.
- Use `tracing` for logging, not `println!`.

### C++

- Follow KDE/Qt conventions. C++20 standard.
- Use `Q_PROPERTY`, `Q_INVOKABLE`, signals/slots.
- Snake_case for methods matching QML API.

### QML

- Kirigami conventions.
- Use `Kirigami.FormData.isChecking` patterns.
- QtQuick.Controls 2 + Kirigami components.

## Testing expectations

- **Daemon and core**: unit tests via `cargo test`. Tests use temporary sysfs fixtures with fake hwmon structures.
- **CLI**: unit tests for serialization and text rendering.
- **GUI**: currently manual testing only (no QML test framework set up yet).
- All tests must pass before submitting changes.

## How to add a DBus method (full-stack)

1. Add the method to the daemon interface in `crates/daemon/src/main.rs` (zbus `#[interface]` attribute)
2. Add a proxy method to `crates/cli/src/main.rs` (zbus `#[proxy]` trait)
3. Add the method to `gui/src/daemon_interface.h` and `gui/src/daemon_interface.cpp`
4. If needed, add a model update in `gui/src/status_monitor.h`/`.cpp`
5. Update QML pages if the UI needs to expose the new method
6. Add a CLI subcommand if appropriate
7. Add tests

## How to add a config field

1. Add the field to the appropriate struct in `crates/core/src/config.rs` with `#[serde(default)]` and a default function
2. Update `DraftFanEntry::resolved_*()` methods if the field has draft resolution behavior
3. Update `validate_draft()` or `validate_cadence()`/`validate_actuator_policy()`/`validate_pid_limits()` if the field needs validation
4. Update `apply_draft()` to propagate the field from draft to applied config
5. Update `crates/daemon/src/main.rs` if a new DBus payload type is needed
6. Add backward-compatibility test: verify a config without the new field deserializes with safe defaults

## How to add a GUI page

1. Create `gui/qml/NewPage.qml` following Kirigami conventions
2. Add the QML file to `qt_add_qml_module()` in `gui/CMakeLists.txt`
3. Wire page navigation in `gui/qml/Main.qml`
4. Create or extend C++ model classes in `gui/src/models/` if needed
5. Register QML types in `gui/src/main.cpp` if introducing new model classes

## Commit conventions

- Use present tense, imperative mood: "Add feature" not "Added feature"
- Scope the change: "daemon: add sd-notify readiness support"
- Keep commits atomic: one logical change per commit

## Known technical debt

- StatusMonitor uses 250ms polling instead of reactive DBus signal subscriptions
- KF6 dev packages need proper CMake find_package support in some distros
- Tray/popover navigation stubs not fully wired
- FanDetailPage advanced tab values are hardcoded
- Lifecycle events only refresh on page load