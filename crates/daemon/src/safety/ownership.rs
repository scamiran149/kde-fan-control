//! Owned-fan persistence helpers.
//!
//! When the daemon takes ownership of a fan (switching it from BIOS control to
//! manual PWM), it must record that fact durably. If the daemon crashes or the
//! machine loses power, a subsequent boot must know which fans were under
//! daemon control so that the safety fallback layer can drive them to a safe
//! maximum speed.
//!
//! The persistence file lives under the application state directory and is
//! written atomically (write-to-temp + rename) to avoid partial-write
//! corruption. File permissions are set to 0600 because the file contents
//! reflect hardware control state that only root should inspect or modify.

use std::fs;
use std::path::PathBuf;

use kde_fan_control_core::config::app_state_dir;
use kde_fan_control_core::lifecycle::OwnedFanSet;

/// Returns the daemon state directory.
///
/// Wraps [`app_state_dir`] from the core crate so that the persistence layer
/// doesn't need to know the base path itself.
pub fn state_dir() -> PathBuf {
    app_state_dir()
}

/// Returns the path to the owned-fans JSON file.
///
/// The file is always located at `<state_dir>/owned-fans.json`.
pub fn owned_fans_path() -> PathBuf {
    state_dir().join("owned-fans.json")
}

/// Persists the current set of owned fans to disk.
///
/// Serialises the fan IDs and their associated sysfs paths into a JSON
/// document and writes it atomically:
///
/// 1. Serialise the `OwnedFanSet` to pretty-printed JSON.
/// 2. Write to a `.tmp` sidecar file.
/// 3. `rename(2)` the temp file over the real path.
/// 4. Set the resulting file's mode to `0600`.
///
/// Failures at any stage are logged but **not** propagated—ownership
/// persistence is best-effort and must not crash the daemon.
pub fn persist_owned_fans(owned: &OwnedFanSet) {
    let path = owned_fans_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let fans: Vec<serde_json::Value> = owned
        .owned_fan_ids()
        .filter_map(|fan_id| {
            owned.sysfs_path(fan_id).map(|p| {
                serde_json::json!({
                    "fan_id": fan_id,
                    "sysfs_path": p,
                })
            })
        })
        .collect();
    let doc = serde_json::json!({ "fans": fans });
    match serde_json::to_string_pretty(&doc) {
        Ok(json) => {
            let tmp_path = path.with_extension("json.tmp");
            if let Err(e) = fs::write(&tmp_path, &json) {
                tracing::warn!(path = %tmp_path.display(), error = %e, "failed to write owned-fans list (temp)");
                return;
            }
            if let Err(e) = fs::rename(&tmp_path, &path) {
                tracing::warn!(from = %tmp_path.display(), to = %path.display(), error = %e, "failed to rename owned-fans list");
                let _ = fs::remove_file(&tmp_path);
            } else if let Err(e) =
                fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
            {
                tracing::warn!(path = %path.display(), error = %e, "failed to set permissions on owned-fans list");
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize owned-fans list");
        }
    }
}
