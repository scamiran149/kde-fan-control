# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## v1.0.2 — 2026-04-14

Packaging follow-up release for the `v1.0.1` line.

### Fixed

- align the installed desktop icon assets with the `org.kde.fancontrol` application ID used by Plasma packaging
- refresh bundled architecture, config lifecycle, and safety diagrams with cleaner typography and spacing

## v1.0.0 — 2026-04-12

The v1.0 MVP. All 52 v1 requirements validated across 4 phases, 15 plans, 29 tasks, 77 commits.

### Added (Phase 1: Hardware Inventory & Visibility)

- hwmon device discovery from `/sys/class/hwmon/hwmon*` with stable hardware IDs (FNV-1a hash of canonical device path)
- Support state classification for fan channels (available, partial, unavailable)
- Control mode detection (PWM, voltage via writable `pwmN_mode`)
- RPM feedback detection from `fan*_input` sysfs nodes
- DBus inventory interface (`org.kde.FanControl.Inventory`) with hardware snapshot
- CLI `inventory` command with text and JSON output formats
- Friendly name persistence for sensors and fans

### Added (Phase 2: Safe Enrollment & Lifecycle Recovery)

- Versioned draft/applied config lifecycle with TOML persistence
- DBus lifecycle interface (`org.kde.FanControl.Lifecycle`) with staged draft edits and explicit apply
- Boot reconciliation: enrolled fans auto-resume daemon control on restart
- Crash/shutdown fallback: owned fans driven to PWM 255 (safe maximum)
- Panic fallback hook with `PanicFallbackMirror` for synchronous sysfs writes from panic context
- Fallback incident persistence and inspection across restarts
- Read-open/write-privileged DBus authorization (UID-0 required for writes)
- CLI lifecycle commands: enroll, unenroll, discard, validate, apply, degraded, events
- Best-effort partial apply: valid fans promoted, invalid ones reported with reasons
- Backward-compatible config deserialization (serde defaults for all new fields)

### Added (Phase 3: Temperature Control & Runtime Operations)

- Temperature-target PID control with configurable P, I, D gains
- Sensor aggregation functions: average, max, min, median
- Per-fan control loops with 3-interval model (sample, control, write)
- PWM output mapping with configurable actuator policy (min/max range, startup kick)
- Auto-tune with bounded observation window, step-response parameter derivation, softened proposals
- DBus control interface (`org.kde.FanControl.Control`) with runtime status and auto-tune
- CLI runtime commands: state, control set, auto-tune start/result/accept
- Deadband support for stable output near target temperature
- PID integral and derivative clamp limits

### Added (Phase 4: KDE GUI & Tray Experience)

- C++ DBus proxy (DaemonInterface) with async call pattern and signal forwarding
- StatusMonitor with 250ms polling and coalesced model updates
- FanListModel (severity-sorted, diff-based updates)
- SensorListModel and DraftModel for edit-state tracking
- QML pages: Overview (dashboard), Inventory (hardware browser), FanDetail (config/auto-tune/advanced tabs), Wizard (7-step guided enrollment)
- System tray via KStatusNotifierItem with status icon and popover
- Desktop notifications via KNotification (fallback-active, degraded-state, high-temp-alert events)
- QML components: StateBadge, OutputBar, TemperatureDisplay, PidField, RenameDialog
