//! CLI command implementations.
//!
//! Each submodule targets one DBus interface area:
//! - `inventory` → `org.kde.FanControl.Inventory`
//! - `lifecycle` → `org.kde.FanControl.Lifecycle`
//! - `control`   → `org.kde.FanControl.Control`
//! - `friendly`  → `org.kde.FanControl.Inventory` (rename/unname)
//! - `status`    → Lifecycle + Control (merged runtime view)

pub mod control;
pub mod friendly;
pub mod inventory;
pub mod lifecycle;
pub mod status;
