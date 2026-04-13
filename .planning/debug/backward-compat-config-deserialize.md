---
status: diagnosed
trigger: "Phase 2 config with enrolled fans but no Phase 3 fields like target_temp_millidegrees fails TOML parse: missing field `target_temp_millidegrees`; also 4 dead_code warnings in main.rs"
created: 2026-04-11T00:00:00Z
updated: 2026-04-11T00:00:00Z
---

## Current Focus

hypothesis: AppliedFanEntry has required (non-optional) Phase 3 fields that break backward compat when deserializing Phase 2 configs
test: structural code analysis — confirmed by reading config.rs
expecting: identified all required fields missing from Phase 2 configs
next_action: return structured diagnosis

## Symptoms

expected: Daemon boots and restores previously enrolled/managed fans from config on cold start
actual: TOML parse error "missing field `target_temp_millidegrees`" causes config load failure; enrolled fans silently fall back to unmanaged
errors: "missing field `target_temp_millidegrees`" TOML parse error; 4 dead_code compiler warnings in main.rs
reproduction: Boot daemon with Phase 2 config file that has enrolled/managed fans but no Phase 3 fields
started: Since Phase 3 fields were added to AppliedFanEntry

## Eliminated

(none needed — root cause confirmed by code analysis)

## Evidence

- timestamp: 2026-04-11T00:00:00Z
  checked: crates/core/src/config.rs lines 141-163 (AppliedFanEntry struct)
  found: AppliedFanEntry has 8 fields, 7 of which are required (no Option, no serde default). Only temp_sources has #[serde(default)]
  implication: A Phase 2 config file missing any of these 7 required fields fails to deserialize

- timestamp: 2026-04-11T00:00:00Z
  checked: crates/core/src/config.rs lines 86-121 (DraftFanEntry struct)
  found: DraftFanEntry correctly uses Option<T> and #[serde(default)] for all Phase 3 fields — DraftFanEntry is already backward-compatible
  implication: The backward-compat issue is isolated to AppliedFanEntry, not DraftFanEntry

- timestamp: 2026-04-11T00:00:00Z
  checked: crates/core/src/config.rs lines 129-138 (AppliedConfig struct)
  found: AppliedConfig.fans is a required HashMap (no serde default) and applied_at is Option<String> with #[serde(default)]
  implication: applied_at is fine; fans is required which is correct since AppliedConfig shouldn't exist without fans

- timestamp: 2026-04-11T00:00:00Z
  checked: crates/daemon/src/main.rs lines 176, 759, 1123, 1132
  found: dead_code warnings on set_auto_tune_observation_window_ms (line 176), require_test_authorized (line 759), accept_auto_tune_for_test (line 1123), set_draft_fan_control_profile_for_test (line 1132)
  implication: require_test_authorized is only called from accept_auto_tune_for_test and set_draft_fan_control_profile_for_test — all three are test-only helpers that bypass DBus auth. set_auto_tune_observation_window_ms is called only from test code (auto_tune_test_harness line 2608).

## Resolution

root_cause: AppliedFanEntry (config.rs:142-163) exposes Phase 3 control-profile fields as required non-optional types (e.g. target_temp_millidegrees: i64 instead of Option<i64>). A Phase 2 config file that has an applied section with enrolled fans but was written before these fields existed lacks the TOML keys, causing serde to fail with "missing field" errors during AppConfig::load(). Since main.rs (line 1899-1908) catches the load error and falls back to AppConfig::default(), previously managed fans are silently dropped to unmanaged, which is a safety concern.

fix: Make AppliedFanEntry's Phase 3 fields either Option<T> with #[serde(default)] or provide serde(default = "...") defaults. Apply #[cfg(test)] to the 3 test-only helpers in main.rs.

verification: Write a test that deserializes a Phase 2-style TOML config (applied section with control_mode and temp_sources only) and confirms it succeeds with sensible defaults.

files_changed: []