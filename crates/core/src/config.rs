use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub friendly_names: FriendlyNames,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FriendlyNames {
    #[serde(default)]
    pub sensors: HashMap<String, String>,
    #[serde(default)]
    pub fans: HashMap<String, String>,
}

impl AppConfig {
    pub fn load() -> io::Result<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(&path)?;
        toml::from_str(&contents).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn save(&self) -> io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&path, contents)
    }

    pub fn set_sensor_name(&mut self, id: &str, name: String) {
        self.friendly_names.sensors.insert(id.to_string(), name);
    }

    pub fn set_fan_name(&mut self, id: &str, name: String) {
        self.friendly_names.fans.insert(id.to_string(), name);
    }

    pub fn remove_sensor_name(&mut self, id: &str) {
        self.friendly_names.sensors.remove(id);
    }

    pub fn remove_fan_name(&mut self, id: &str) {
        self.friendly_names.fans.remove(id);
    }

    pub fn sensor_name(&self, id: &str) -> Option<&str> {
        self.friendly_names.sensors.get(id).map(|s| s.as_str())
    }

    pub fn fan_name(&self, id: &str) -> Option<&str> {
        self.friendly_names.fans.get(id).map(|s| s.as_str())
    }
}

fn config_path() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("/var/lib"))
        .join("kde-fan-control")
        .join("config.toml")
}
