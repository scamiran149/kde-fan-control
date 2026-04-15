//! Control-loop infrastructure.
//!
//! This module groups the daemon's fan-control subsystem:
//!
//! - **supervisor**: runtime manager for per-fan PID loops, auto-tune, and re-assessment
//! - **sampling**: hardware sensor resolution and PWM write helpers
//! - **helpers**: config-to-runtime snapshot conversions and auto-tune math

pub mod autotune;
pub mod fan_loop;
pub mod helpers;
pub mod recovery;
pub mod sampling;
pub mod supervisor;
