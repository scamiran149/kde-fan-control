//! KDE Fan Control core crate.
//!
//! Shared types and logic for configuration, hardware inventory,
//! PID control, lifecycle management, and validation. This crate
//! must not depend on the daemon, CLI, or GUI crates; it is the
//! stable type domain that all other crates build on.

pub mod config;
pub mod control;
pub mod inventory;
pub mod lifecycle;
pub mod overview;
pub mod validation;
