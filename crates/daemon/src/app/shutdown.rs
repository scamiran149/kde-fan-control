//! Graceful daemon shutdown.
//!
//! Waits for a termination signal (SIGTERM or Ctrl-C on Unix) and then
//! performs an orderly shutdown: stops control loops, writes fallback
//! state, and logs the result.

use std::collections::HashSet;
use std::sync::Arc;

use kde_fan_control_core::config::{AppConfig, LifecycleEventLog};
use kde_fan_control_core::lifecycle::{FallbackResult, OwnedFanSet};
use sd_notify::NotifyState;
use tokio::sync::RwLock;

use crate::control::supervisor::ControlSupervisor;
use crate::safety::fallback::run_fallback_recorder;

#[cfg(unix)]
use tokio::signal::unix::{SignalKind, signal};

async fn wait_for_shutdown_signal() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        let mut sigterm = signal(SignalKind::terminate())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result?,
            _ = sigterm.recv() => {}
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}

pub async fn wait_and_shutdown(
    control: ControlSupervisor,
    owned: Arc<RwLock<OwnedFanSet>>,
    config: Arc<RwLock<AppConfig>>,
    events: Arc<RwLock<LifecycleEventLog>>,
    fallback_fan_ids: Arc<RwLock<HashSet<String>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    wait_for_shutdown_signal().await?;
    let _ = sd_notify::notify(&[NotifyState::Stopping]);
    tracing::info!("shutting down — driving owned fans to safe maximum");

    control.stop_all().await;

    let fallback_result: FallbackResult = run_fallback_recorder(
        &owned,
        &config,
        &events,
        &fallback_fan_ids,
        "ctrl-c shutdown".to_string(),
    )
    .await;

    if !fallback_result.succeeded.is_empty() {
        tracing::info!(
            fans = ?fallback_result.succeeded,
            "fallback: set fans to safe maximum"
        );
    }
    if !fallback_result.failed.is_empty() {
        tracing::error!(
            failures = ?fallback_result.failed,
            "fallback: some fans could NOT be set to safe maximum"
        );
    }

    tracing::info!("shutdown complete");
    Ok(())
}
