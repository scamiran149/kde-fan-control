//! KDE Fan Control daemon crate.
//!
//! Contains the root-privileged fan control service: DBus interface
//! implementations, PID control loops, safety fallback, and startup
//! orchestration. This crate must not depend on the CLI or GUI crates.

pub mod app;
pub mod args;
pub mod control;
pub mod dbus;
pub mod safety;
pub mod state;
pub mod time;

#[cfg(test)]
mod test_support;
