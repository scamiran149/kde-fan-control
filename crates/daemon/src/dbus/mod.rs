//! DBus interface modules for the fan-control daemon.
//!
//! Each submodule implements one of the three DBus interfaces
//! documented in `docs/dbus-api.md`:
//!
//! - `inventory`: hardware snapshot and friendly names
//! - `lifecycle`: draft/apply, degraded state, runtime state
//! - `control`: live status, auto-tune, profile mutations
//!
//! `constants` holds bus names, paths, and shared limits.
//! `auth` provides polkit/UID-0 authorization checks.
//! `signals` provides typed signal emission helpers.
//! `helpers` provides pure conversion functions shared by interface modules.

pub mod auth;
pub mod constants;
pub mod control;
pub mod helpers;
pub mod inventory;
pub mod lifecycle;
pub mod lifecycle_apply;
pub mod signals;
