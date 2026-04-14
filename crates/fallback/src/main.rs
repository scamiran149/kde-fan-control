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
            std::process::exit(1);
        }
    };

    if owned.fans.is_empty() {
        eprintln!("kde-fan-control-fallback: no owned fans, nothing to do");
        std::process::exit(0);
    }

    let mut succeeded = 0u32;
    let mut failed = 0u32;

    for fan in &owned.fans {
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
