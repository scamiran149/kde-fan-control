//! Boot reconciliation, runtime ownership tracking, and fallback lifecycle.
//!
//! This module implements the safe startup, ownership, and crash-path behavior:
//!
//! - Reconcile persisted applied config against live inventory at startup
//! - Restore safe matches as managed, skip unsafe or missing fans
//! - Track which fans the daemon actually owns at runtime
//! - Provide safe-maximum fallback for owned fans on failure or shutdown

mod fallback;
mod owned;
mod reassess;
mod reconcile;
mod runtime;
mod state;
mod time;

pub use fallback::{
    FallbackResult, PWM_ENABLE_MANUAL, PWM_SAFE_MAX, lifecycle_event_from_fallback_incident,
    write_fallback_for_owned, write_fallback_single,
};
pub use owned::OwnedFanSet;
pub use reassess::{ReassessOutcome, reassess_single_fan};
pub use reconcile::{
    ReconcileOutcome, ReconcileResult, perform_boot_reconciliation, reconcile_applied_config,
};
pub use runtime::{ControlRuntimeSnapshot, FanRuntimeStatus, RuntimeState};
pub use state::{
    DegradedReason, DegradedState, LifecycleEvent, LifecycleEventLog, MAX_LIFECYCLE_EVENTS,
};
pub use time::format_iso8601_now;
