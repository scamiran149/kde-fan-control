//! Safety-critical fallback and ownership persistence.
//!
//! This module group handles fan safety guarantees:
//! - **ownership**: persisting the set of owned fans so fallback can target them
//! - **fallback**: graceful and panic-time fallback to PWM 255
//! - **panic_hook**: installing a process-level panic handler that drives fans
//!   to safe maximum before the process terminates

pub mod fallback;
pub mod ownership;
pub mod panic_hook;
