//! Fan-control daemon entry point.
//!
//! Parses CLI arguments, initializes tracing, and delegates to the
//! application startup routine. This binary runs as a privileged root
//! daemon and must not be invoked directly by unprivileged users.

use clap::Parser;
use kde_fan_control_daemon::args::DaemonArgs;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = DaemonArgs::parse();
    kde_fan_control_daemon::app::startup::run(args).await
}
