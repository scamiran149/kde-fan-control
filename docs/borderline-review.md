# Borderline File Review: overview.rs and inventory.rs

Assessment of two files hovering just above the 400-line threshold.

---

## 1. `crates/core/src/overview.rs` — 512 lines

### Structure map

| Lines | Item | Kind |
|-------|------|------|
| 1–6 | imports | — |
| 7–27 | `OverviewStructureSnapshot`, `OverviewStructureRow` | structs |
| 29–48 | `OverviewTelemetryBatch`, `OverviewTelemetryRow` | structs |
| 50–128 | `ordering_bucket`, `bucket_sort_key`, `state_text`, `state_icon_name`, `state_color`, `fan_display_name`, `format_temp`, `format_rpm`, `format_output` | free helper fns |
| 130–188 | `OverviewStructureSnapshot::build` | impl |
| 190–261 | `OverviewTelemetryBatch::build` | impl |
| 263–512 | `#[cfg(test)] mod tests` | tests |

### Domain analysis

The file has **two clear domains**:

1. **Structure snapshot** — types + builder for the structural/visual row data (fan ordering, state icons, colors, display names). Lines 7–27 + 50–112 + 130–188 = ~160 lines of production code.
2. **Telemetry batch** — types + builder for the live numeric readouts (temps, RPM, output %). Lines 29–48 + 114–128 + 190–261 = ~105 lines of production code.

The shared helpers (`fan_display_name`, `format_temp`, `format_rpm`, `format_output`) bridge both domains but are tiny; any split would duplicate or re-export them.

The **test module is 250 lines** — half the file. The tests are tightly coupled to the production code they exercise and each domain has its own test fixtures.

### Split estimate

| File | Approximate lines (prod) | Approximate lines (tests) |
|------|--------------------------|---------------------------|
| `overview_structure.rs` | ~165 | ~120 |
| `overview_telemetry.rs` | ~105 | ~130 |
| (shared: `format_*` helpers stay in whichever file, or a small `overview/mod.rs`) | — | — |

### Recommendation: **Leave as-is**

Both domains are pure data-to-UI projection functions with no mutable state, no complex logic, and no independent error paths. They share helper functions and the same set of imports. Splitting would create two ~230-line files plus a module boilerplate file for a net increase in total lines and navigation overhead. The file is over 400 lines only because of the test module; the production code is ~265 lines. Not worth splitting.

---

## 2. `crates/core/src/inventory.rs` — 493 lines

### Structure map

| Lines | Item | Kind |
|-------|------|------|
| 1–6 | imports | — |
| 8–11 | `InventorySnapshot` | struct |
| 13–24 | `InventorySnapshot::update_fan_rpm` | impl method |
| 26–34 | `HwmonDevice` | struct |
| 36–43 | `TemperatureSensor` | struct |
| 45–56 | `FanChannel` | struct |
| 58–71 | `ControlMode`, `SupportState` | enums |
| 73–93 | `discover`, `discover_from` | public discovery fns |
| 95–140 | `discover_device` | private discovery fn |
| 142–188 | `build_fan_channel` | private fn |
| 190–211 | `detect_control_modes` | private fn |
| 213–239 | `collect_channel_numbers` | private fn |
| 241–248 | `resolve_stable_identity` | private fn |
| 250–298 | `sanitize`, `fnv1a64`, `read_trimmed`, `read_number`, `is_writable` | utility fns |
| 300–308 | `BoolExt` trait + impl | utility trait |
| 310–493 | `#[cfg(test)] mod tests` | tests |

### Domain analysis

The file has **one cohesive domain**: hardware discovery. All production functions contribute to building an `InventorySnapshot` from sysfs. The types (`HwmonDevice`, `FanChannel`, `TemperatureSensor`, `ControlMode`, `SupportState`) and the discovery logic are inseparable — they define the same abstraction.

Within that domain there are two sub-concerns:
1. **Type definitions** (lines 8–71) — ~65 lines of pure data types.
2. **Discovery logic** (lines 73–308) — ~235 lines of sysfs traversal and parsing.

The utility layer (`sanitize`, `fnv1a64`, `read_trimmed`, `read_number`, `is_writable`, `BoolExt`) is ~60 lines and is only used by discovery.

The **test module is 183 lines** — fixture infrastructure (`HwmonFixture`, `HwmonDir`) accounts for ~80 lines of that.

### Split estimate

| File | Approximate lines (prod) | Approximate lines (tests) |
|------|--------------------------|---------------------------|
| `inventory_types.rs` | ~70 | 0 |
| `inventory_discover.rs` (or keep as `inventory.rs`) | ~240 | ~185 |
| Total across 2 files | ~310 | ~185 |

### Recommendation: **Leave as-is**

This file is one domain — the types and the discovery function are tightly coupled (`discover_from` is the only producer of these types). Extracting just the type definitions would create a 70-line file that every consumer still needs, adding import noise without reducing cognitive load. The utility functions are trivial and inventory-specific. The production code is ~305 lines, which is well within reason for a single module. Not worth splitting.

---

## Summary

| File | Lines | Prod lines | Test lines | Distinct domains? | Recommendation |
|------|-------|------------|------------|-------------------|----------------|
| `overview.rs` | 512 | ~265 | ~250 | 2, but tightly coupled | **Leave as-is** |
| `inventory.rs` | 493 | ~305 | ~185 | 1 | **Leave as-is** |

Both files exceed 400 lines primarily because of their test modules. The production code in each file is modest and cohesive. Splitting would distribute the same amount of logic across more files, add module boilerplate, and increase navigation cost without a meaningful reduction in per-file complexity. A future refactoring pass could consider splitting `overview.rs` into structure + telemetry only if either domain gains significant logic (e.g., more complex visual-state derivation, filtering, or pagination), but the current code doesn't justify it.