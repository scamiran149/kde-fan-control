use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySnapshot {
    pub devices: Vec<HwmonDevice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwmonDevice {
    pub id: String,
    pub name: String,
    pub sysfs_path: String,
    pub stable_identity: String,
    pub temperatures: Vec<TemperatureSensor>,
    pub fans: Vec<FanChannel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureSensor {
    pub id: String,
    pub channel: u32,
    pub label: Option<String>,
    pub friendly_name: Option<String>,
    pub input_millidegrees_celsius: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanChannel {
    pub id: String,
    pub channel: u32,
    pub label: Option<String>,
    pub friendly_name: Option<String>,
    pub rpm_feedback: bool,
    pub current_rpm: Option<u64>,
    pub control_modes: Vec<ControlMode>,
    pub support_state: SupportState,
    pub support_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlMode {
    Pwm,
    Voltage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportState {
    Available,
    Partial,
    Unavailable,
}

pub fn discover() -> io::Result<InventorySnapshot> {
    discover_from(Path::new("/sys/class/hwmon"))
}

pub fn discover_from(root: &Path) -> io::Result<InventorySnapshot> {
    let mut devices = Vec::new();

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        devices.push(discover_device(&path)?);
    }

    devices.sort_by(|left, right| left.id.cmp(&right.id));

    Ok(InventorySnapshot { devices })
}

fn discover_device(hwmon_path: &Path) -> io::Result<HwmonDevice> {
    let name = read_trimmed(hwmon_path.join("name"))?.unwrap_or_else(|| "unknown".to_string());
    let stable_identity = resolve_stable_identity(hwmon_path);
    let device_id = format!(
        "hwmon-{}-{:016x}",
        sanitize(&name),
        fnv1a64(&stable_identity)
    );

    let entries = fs::read_dir(hwmon_path)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<BTreeSet<_>>();

    let temp_channels = collect_channel_numbers(&entries, "temp", Some("_input"));
    let mut fan_channels = collect_channel_numbers(&entries, "fan", Some("_input"));
    fan_channels.extend(collect_channel_numbers(&entries, "pwm", None));

    let temperatures = temp_channels
        .into_iter()
        .map(|channel| TemperatureSensor {
            id: format!("{}-temp{}", device_id, channel),
            channel,
            label: read_trimmed(hwmon_path.join(format!("temp{channel}_label"))).unwrap_or(None),
            friendly_name: None,
            input_millidegrees_celsius: read_number::<i64>(
                hwmon_path.join(format!("temp{channel}_input")),
            )
            .unwrap_or(None),
        })
        .collect();

    let fans = fan_channels
        .into_iter()
        .map(|channel| build_fan_channel(hwmon_path, &device_id, channel))
        .collect::<io::Result<Vec<_>>>()?;

    Ok(HwmonDevice {
        id: device_id,
        name,
        sysfs_path: hwmon_path.display().to_string(),
        stable_identity,
        temperatures,
        fans,
    })
}

fn build_fan_channel(hwmon_path: &Path, device_id: &str, channel: u32) -> io::Result<FanChannel> {
    let fan_input_path = hwmon_path.join(format!("fan{channel}_input"));
    let pwm_path = hwmon_path.join(format!("pwm{channel}"));
    let pwm_enable_path = hwmon_path.join(format!("pwm{channel}_enable"));

    let rpm_feedback = fan_input_path.exists();
    let has_pwm_node = pwm_path.exists();
    let pwm_writable = has_pwm_node && is_writable(&pwm_path)?;
    let pwm_enable_writable = pwm_enable_path.exists() && is_writable(&pwm_enable_path)?;

    let control_modes = if pwm_writable {
        vec![ControlMode::Pwm]
    } else {
        Vec::new()
    };

    let (support_state, support_reason) = if pwm_writable {
        (SupportState::Available, None)
    } else if has_pwm_node {
        (
            SupportState::Partial,
            Some(if pwm_enable_path.exists() && !pwm_enable_writable {
                "pwm control exists but mode switching is not writable".to_string()
            } else {
                "pwm control node exists but is not writable".to_string()
            }),
        )
    } else if rpm_feedback {
        (
            SupportState::Partial,
            Some("tach feedback exists but no writable control node was detected".to_string()),
        )
    } else {
        (
            SupportState::Unavailable,
            Some("no tach feedback or controllable output node was detected".to_string()),
        )
    };

    Ok(FanChannel {
        id: format!("{}-fan{}", device_id, channel),
        channel,
        label: read_trimmed(hwmon_path.join(format!("fan{channel}_label")))?,
        friendly_name: None,
        rpm_feedback,
        current_rpm: read_number::<u64>(fan_input_path)?,
        control_modes,
        support_state,
        support_reason,
    })
}

fn collect_channel_numbers(
    entries: &BTreeSet<String>,
    prefix: &str,
    suffix: Option<&str>,
) -> BTreeSet<u32> {
    let mut channels = BTreeSet::new();

    for entry in entries {
        let Some(rest) = entry.strip_prefix(prefix) else {
            continue;
        };

        let number_part = match suffix {
            Some(suffix) => match rest.strip_suffix(suffix) {
                Some(number) => number,
                None => continue,
            },
            None => rest,
        };

        if let Ok(channel) = number_part.parse::<u32>() {
            channels.insert(channel);
        }
    }

    channels
}

fn resolve_stable_identity(hwmon_path: &Path) -> String {
    let device_path = hwmon_path.join("device");
    let canonical = fs::canonicalize(&device_path)
        .or_else(|_| fs::canonicalize(hwmon_path))
        .unwrap_or_else(|_| PathBuf::from(hwmon_path));

    canonical.display().to_string()
}

fn sanitize(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());

    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            sanitized.push(character.to_ascii_lowercase());
        } else {
            sanitized.push('_');
        }
    }

    sanitized.trim_matches('_').to_string()
}

fn fnv1a64(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;

    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    hash
}

fn read_trimmed(path: PathBuf) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(value) => Ok(Some(value.trim().to_string())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_number<T>(path: PathBuf) -> io::Result<Option<T>>
where
    T: std::str::FromStr,
{
    match read_trimmed(path)? {
        Some(value) => Ok(value.parse::<T>().ok()),
        None => Ok(None),
    }
}

fn is_writable(path: &Path) -> io::Result<bool> {
    Ok(fs::metadata(path)?.permissions().readonly().not())
}

trait BoolExt {
    fn not(self) -> bool;
}

impl BoolExt for bool {
    fn not(self) -> bool {
        !self
    }
}
