<!-- GSD:project-start source:PROJECT.md -->
## Project

**KDE Fan Control**

KDE Fan Control is a Linux desktop fan-control application for machines where `fancontrol` is too rigid or cumbersome. It runs a privileged daemon that manages per-fan PID control using selectable temperature inputs or aggregated sensor groups, and exposes control through DBus to a KDE/Qt6 GUI, system tray app, and CLI.

**Core Value:** Users can safely and flexibly control desktop fan behavior with understandable per-fan PID policies, without losing fail-safe behavior.

### Constraints

- **Platform**: Linux desktop only — hardware control depends on Linux kernel sensor/fan interfaces
- **Privilege boundary**: Hardware control runs as a root daemon — direct fan writes require elevated privileges
- **IPC**: DBus-first architecture — CLI and GUI should use the same control surface
- **Backend stack**: Rust for the daemon and CLI — prioritize safety, concurrency control, and predictable deployment
- **UI stack**: KDE/Qt6 with QML — optimize for native Linux desktop experience
- **Persistence**: Single active daemon-owned configuration in v1 — avoid split-brain config management across clients
- **Safety**: Controlled fans must fail to high speed on service failure — prevent thermal risk
- **Compatibility**: BIOS-managed fans must remain untouched when not explicitly enrolled — avoid breaking existing system behavior
- **Hardware variability**: PWM and voltage control may vary by board or controller — implementation must tolerate partial capability exposure
<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->
## Technology Stack

## Recommended Stack
- **Rust daemon + CLI** for privileged hardware control, persistence, control logic, and DBus service surface
- **DBus on the system bus** as the only control API boundary
- **systemd system service** for lifecycle, readiness, restart, watchdog, and boot integration
- **Qt 6 + Kirigami/QML GUI in C++/QML** for the KDE-native desktop app
## Prescriptive Stack
### Backend: daemon and CLI
| Technology | Version | Purpose | Why | Confidence |
|---|---:|---|---|---|
| Rust | stable 1.94.1 | daemon, CLI, core control logic | Best fit for long-running systems code, safe concurrency, strong Linux tooling | HIGH |
| Edition | 2024 | Rust language edition | Use the current edition for new code; no reason to start greenfield on 2021 | HIGH |
| Tokio | 1.51.1 | async runtime | Best-supported async runtime in Rust; pairs cleanly with `zbus` and long-running service loops | HIGH |
| zbus | 5.14.0 | DBus server/client in Rust | Modern Rust-first DBus crate, async, no libdbus C dependency, direct Tokio integration | HIGH |
| clap | 4.6.0 | CLI parsing | Standard Rust CLI parser; subcommands and completions are mature and ergonomic | HIGH |
| serde | 1.0.228 | serialization | Standard serialization layer; avoid inventing config encoding | HIGH |
| toml | 1.1.2+spec-1.1.0 | daemon-owned config format | Human-readable, stable, good fit for one active persisted config | HIGH |
| thiserror | 2.0.18 | typed errors | Keep error surfaces explicit without boilerplate | HIGH |
| tracing | 0.1.44 | structured logging/instrumentation | Better than ad-hoc logs for control loops, DBus requests, and failure analysis | HIGH |
| tracing-subscriber | 0.3.23 | log formatting/filters | Standard companion to `tracing` | HIGH |
| sd-notify | 0.5.0 | systemd readiness/watchdog | Small, practical fit for `Type=notify` services | MEDIUM |
| udev | 0.9.3 | stable hardware identity/enumeration | Useful for mapping hwmon devices to persistent identities and hotplug events | MEDIUM |
### Linux integration layer
| Integration point | Recommended approach | Why | Confidence |
|---|---|---|---|
| Sensor/fan discovery | Read `/sys/class/hwmon/hwmon*` as the authoritative hardware view | Kernel hwmon ABI is the real control surface for temp/fan/pwm nodes | HIGH |
| Writable fan control | Write sysfs `pwm*`, `pwm*_enable`, and related fan-control attributes from the root daemon only | Keeps privilege and safety logic centralized | HIGH |
| Device identity | Augment sysfs scanning with `udev` metadata | hwmon numbering is not stable enough to be your identity model | MEDIUM |
| Optional discovery assistance | Allow `lm-sensors` / `sensors-detect` as optional operator tooling, not required runtime linkage | Helpful in the field, but the daemon must understand hardware directly | MEDIUM |
| IPC contract | Define one DBus interface namespace, object tree, and stable method/property/signal contract | CLI and GUI must talk to the exact same surface | HIGH |
| Service lifecycle | Install as a **system** service, not a user service | Fan writes are privileged and must survive login state | HIGH |
### GUI: KDE-native desktop application
| Technology | Version | Purpose | Why | Confidence |
|---|---:|---|---|---|
| Qt | 6.11.x for upstream dev; keep app code compatible with 6.8+ distro floor | GUI foundation | Current Qt 6 line, strong QML tooling, current docs; 6.8 floor is the practical compatibility line for Linux distro packaging | MEDIUM |
| Qt Quick | Qt 6 | scene/UI framework | Standard QML UI layer | HIGH |
| Qt QML | Qt 6 | QML engine and module system | Official integration path for QML apps and C++ backends | HIGH |
| Qt DBus | Qt 6 | GUI-side DBus client | Native Qt-side DBus support; avoids custom IPC glue in the GUI | HIGH |
| Qt Quick Controls 2 | Qt 6 | controls/widgets for QML | Kirigami is built on top of it | HIGH |
| KDE Frameworks / Kirigami | KF6 / Kirigami 6.x | KDE-native app shell and components | Best fit for a KDE-first product; aligns with KDE HIG and desktop conventions | HIGH |
| KStatusNotifierItem | KF6 current distro package | tray/system notifier integration | Prefer KDE’s notifier model for tray behavior on Plasma | MEDIUM |
| C++20 | compiler floor for GUI glue | QML-exposed backend objects, DBus wrappers, platform glue | Qt/Kirigami’s first-class extension path is C++ + QML, not Rust bindings | HIGH |
| CMake + ECM | current distro packages | GUI build system | Standard KDE application build stack | HIGH |
## Architecture-level recommendation
### Use this split
## systemd recommendations
- `Type=notify`
- `NotifyAccess=main`
- `Restart=on-failure`
- `WatchdogSec=` enabled once the daemon loop is stable
- explicit hardening (`NoNewPrivileges=`, `ProtectSystem=`, `ProtectHome=`, narrowed writable paths)
- systemd can wait until hwmon discovery, config load, and DBus registration actually succeed
- watchdog support is straightforward
- readiness becomes explicit instead of guessed
## hwmon / Linux-specific implementation notes
### Treat these as core inputs to the stack decision
- The kernel hwmon ABI exposes standardized names like `temp*_input`, `fan*_input`, `pwm*`, and `pwm*_enable`.
- hwmon directories should be discovered via `/sys/class/hwmon/hwmon*`.
- sysfs values are fixed-point strings and vary by chip; user space owns labeling and some interpretation.
- Writable attributes must stay root-only.
### Practical implication
- writable fan-control semantics
- daemon-owned safety policy
- stable DBus-facing objects
- custom labeling and aggregation behavior
## What NOT to use
| Avoid | Why not |
|---|---|
| `dbus-rs` for new development | `zbus` is the better greenfield Rust choice for async/Tokio-heavy service code |
| Rust Qt bindings as the primary GUI strategy | Official Qt/Kirigami app guidance is centered on C++ + QML; forcing Rust into the GUI increases integration risk with little product value |
| Qt Widgets for the main app | The project explicitly wants a KDE/Qt6 QML GUI; Widgets would fight that goal |
| Direct GUI or CLI writes to `/sys` | Breaks privilege boundaries and duplicates safety logic |
| User service for the daemon | Wrong privilege/lifecycle model for hardware control |
| Required runtime dependency on `lm-sensors` | Makes the app depend on another abstraction layer for a problem it must understand itself |
| SQLite as the v1 source of truth | Overkill for one active config; TOML is simpler and easier to inspect/recover |
## Recommended dependency shape
### Rust daemon / CLI
### Qt/KDE GUI build ingredients
- Qt6 Core / Gui / Qml / Quick / QuickControls2 / DBus
- KDE Frameworks 6 Kirigami
- KDE Frameworks 6 StatusNotifierItem
- Extra CMake Modules (ECM)
## Final recommendation
- **Rust 1.94.1 + Tokio + zbus** for the daemon/CLI
- **systemd system service with `Type=notify`** for lifecycle and watchdog
- **raw hwmon sysfs + optional udev enrichment** for Linux hardware integration
- **Qt 6 + Kirigami + Qt DBus + small C++ glue layer** for the GUI
## Sources
- Rust stable channel manifest (Rust 1.94.1): https://static.rust-lang.org/dist/channel-rust-stable.toml — HIGH
- zbus docs and Tokio integration: Context7 `/dbus2/zbus` — HIGH
- Tokio docs: Context7 `/tokio-rs/tokio` — HIGH
- clap docs: Context7 `/clap-rs/clap` — HIGH
- crates.io `zbus` 5.14.0: https://crates.io/crates/zbus — HIGH
- crates.io `tokio` 1.51.1: https://crates.io/crates/tokio — HIGH
- crates.io `clap` 4.6.0: https://crates.io/crates/clap — HIGH
- crates.io `serde` 1.0.228: https://crates.io/crates/serde — HIGH
- crates.io `toml` 1.1.2+spec-1.1.0: https://crates.io/crates/toml — HIGH
- crates.io `thiserror` 2.0.18: https://crates.io/crates/thiserror — HIGH
- crates.io `tracing` 0.1.44: https://crates.io/crates/tracing — HIGH
- crates.io `tracing-subscriber` 0.3.23: https://crates.io/crates/tracing-subscriber — HIGH
- crates.io `sd-notify` 0.5.0: https://crates.io/crates/sd-notify — MEDIUM
- crates.io `udev` 0.9.3: https://crates.io/crates/udev — MEDIUM
- Qt 6.11 docs: https://doc.qt.io/qt-6/ — HIGH
- Qt release stream showing Qt 6.11 and 6.10.x: https://www.qt.io/blog/tag/releases — HIGH
- Qt DBus docs: https://doc.qt.io/qt-6/qtdbus-index.html — HIGH
- Qt QML docs: https://doc.qt.io/qt-6/qtqml-index.html — HIGH
- Qt Quick docs: https://doc.qt.io/qt-6/qtquick-index.html — HIGH
- Kirigami docs/tutorial: Context7 `/websites/develop_kde_getting-started_kirigami` — HIGH
- Kirigami README: https://invent.kde.org/frameworks/kirigami/-/raw/master/README.md — HIGH
- KStatusNotifierItem README: https://invent.kde.org/frameworks/kstatusnotifieritem/-/raw/master/README.md — MEDIUM
- Linux kernel hwmon sysfs ABI: https://www.kernel.org/doc/html/latest/hwmon/sysfs-interface.html — HIGH
- systemd service semantics: https://man7.org/linux/man-pages/man5/systemd.service.5.html — MEDIUM
- udev semantics: https://man7.org/linux/man-pages/man7/udev.7.html — MEDIUM
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, or `.github/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
