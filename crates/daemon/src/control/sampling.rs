//! Hardware sensor sampling and PWM write helpers.
//!
//! Pure functions for resolving temperature sources, fan RPM paths,
//! and writing PWM values to sysfs. These have no dependency on the
//! running daemon state and can be tested in isolation.

use std::fs;
use std::path::PathBuf;

use kde_fan_control_core::inventory::InventorySnapshot;

/// Resolve temperature source IDs to `(temp_id, sysfs_path)` pairs.
///
/// For each ID in `temp_sources`, looks up the matching sensor in the
/// inventory snapshot and returns its absolute `temp{N}_input` path.
/// Unresolvable IDs are silently dropped (the caller handles missing
/// data at the control-loop level).
pub fn resolve_temp_sources(
    snapshot: &InventorySnapshot,
    temp_sources: &[String],
) -> Vec<(String, PathBuf)> {
    temp_sources
        .iter()
        .filter_map(|temp_id| {
            snapshot.devices.iter().find_map(|device| {
                device
                    .temperatures
                    .iter()
                    .find(|sensor| &sensor.id == temp_id)
                    .map(|sensor| {
                        (
                            temp_id.clone(),
                            PathBuf::from(&device.sysfs_path)
                                .join(format!("temp{}_input", sensor.channel)),
                        )
                    })
            })
        })
        .collect()
}

/// Resolve a fan ID to its RPM feedback sysfs path.
///
/// Returns `None` if the fan is not found or has no RPM feedback.
pub fn resolve_fan_rpm_path(snapshot: &InventorySnapshot, fan_id: &str) -> Option<PathBuf> {
    snapshot.devices.iter().find_map(|device| {
        device
            .fans
            .iter()
            .find(|fan| fan.id == fan_id && fan.rpm_feedback)
            .map(|fan| PathBuf::from(&device.sysfs_path).join(format!("fan{}_input", fan.channel)))
    })
}

/// Write a PWM value to a sysfs path, first setting the channel to manual mode.
///
/// If setting `_enable` to manual mode fails, a warning is logged but the
/// write still proceeds — the kernel may already be in manual mode.
pub fn write_pwm_value(pwm_path: &str, pwm_value: u16) -> std::io::Result<()> {
    let pwm_enable_path = format!("{pwm_path}_enable");
    if let Err(error) = fs::write(
        &pwm_enable_path,
        kde_fan_control_core::lifecycle::PWM_ENABLE_MANUAL.to_string(),
    ) {
        tracing::warn!(path = %pwm_enable_path, error = %error, "failed to set pwm channel to manual mode before write");
    }
    fs::write(pwm_path, pwm_value.to_string())
}
