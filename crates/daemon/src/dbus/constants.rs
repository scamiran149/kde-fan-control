//! DBus bus names, object paths, and shared constants.
//!
//! Centralizes the DBus surface identifiers used by all interface modules
//! and by signal emission helpers.

pub const BUS_NAME: &str = "org.kde.FanControl";
pub const BUS_PATH_INVENTORY: &str = "/org/kde/FanControl";
pub const BUS_PATH_LIFECYCLE: &str = "/org/kde/FanControl/Lifecycle";
pub const BUS_PATH_CONTROL: &str = "/org/kde/FanControl/Control";

/// Maximum allowed length for DBus name parameters.
pub const MAX_NAME_LENGTH: usize = 128;
