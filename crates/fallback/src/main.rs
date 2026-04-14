use std::fs;
use std::path::Path;

const STATE_DIR: &str = "/var/lib/kde-fan-control";
const OWNED_FANS_FILE: &str = "owned-fans.json";
const PWM_SAFE_MAX: u32 = 255;
const PWM_ENABLE_MANUAL: u32 = 1;

#[derive(serde::Deserialize)]
struct OwnedFanEntry {
    fan_id: String,
    sysfs_path: String,
}

#[derive(serde::Deserialize)]
struct OwnedFansFile {
    fans: Vec<OwnedFanEntry>,
}

fn is_valid_sysfs_pwm_path(path: &str) -> bool {
    if !path.starts_with("/sys/class/hwmon/hwmon") {
        return false;
    }
    if path.contains("..") {
        return false;
    }
    let file_name = path.rsplit('/').next().unwrap_or("");
    if !file_name.starts_with("pwm") {
        return false;
    }
    let digits = &file_name[3..];
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn emergency_fallback_all_hwmon() {
    let hwmon_entries: Vec<_> = match fs::read_dir("/sys/class/hwmon") {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            eprintln!("kde-fan-control-fallback: cannot read /sys/class/hwmon: {e}");
            std::process::exit(1);
        }
    };

    for hwmon in &hwmon_entries {
        let hwmon_path = hwmon.path();
        let entries: Vec<_> = match fs::read_dir(&hwmon_path) {
            Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
            Err(_) => Vec::new(),
        };
        for entry in &entries {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("pwm")
                && name_str.chars().skip(3).all(|c| c.is_ascii_digit())
                && !name_str.contains("_")
            {
                let pwm_path = entry.path();
                let enable_path = hwmon_path.join(format!("{}_enable", name_str));
                let _ = fs::write(&enable_path, PWM_ENABLE_MANUAL.to_string());
                let _ = fs::write(&pwm_path, PWM_SAFE_MAX.to_string());
            }
        }
    }
}

fn main() {
    eprintln!("kde-fan-control-fallback: restoring fans to safe maximum");

    let path = Path::new(STATE_DIR).join(OWNED_FANS_FILE);
    let data = match fs::read_to_string(&path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("kde-fan-control-fallback: no owned-fans file ({e}), nothing to do");
            std::process::exit(0);
        }
    };

    let owned: OwnedFansFile = match serde_json::from_str(&data) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("kde-fan-control-fallback: failed to parse owned-fans.json: {e}");
            eprintln!(
                "kde-fan-control-fallback: attempting emergency fallback to all hwmon PWM outputs"
            );
            emergency_fallback_all_hwmon();
            std::process::exit(0);
        }
    };

    if owned.fans.is_empty() {
        eprintln!("kde-fan-control-fallback: no owned fans, nothing to do");
        std::process::exit(0);
    }

    let mut succeeded = 0u32;
    let mut failed = 0u32;

    for fan in &owned.fans {
        if !is_valid_sysfs_pwm_path(&fan.sysfs_path) {
            eprintln!(
                "kde-fan-control-fallback: skipping invalid sysfs_path for {}: {}",
                fan.fan_id, fan.sysfs_path
            );
            continue;
        }

        let pwm_enable_path = format!("{}_enable", fan.sysfs_path);

        if let Err(e) = fs::write(&pwm_enable_path, PWM_ENABLE_MANUAL.to_string()) {
            eprintln!(
                "kde-fan-control-fallback: failed to write pwm_enable for {}: {}",
                fan.fan_id, e
            );
        }

        if let Err(e) = fs::write(&fan.sysfs_path, PWM_SAFE_MAX.to_string()) {
            eprintln!(
                "kde-fan-control-fallback: FAILED to set {} to max PWM: {}",
                fan.fan_id, e
            );
            failed += 1;
        } else {
            eprintln!(
                "kde-fan-control-fallback: set {} to max PWM (255)",
                fan.fan_id
            );
            succeeded += 1;
        }
    }

    eprintln!(
        "kde-fan-control-fallback: done ({} succeeded, {} failed)",
        succeeded, failed
    );

    if failed > 0 {
        std::process::exit(1);
    }
}
