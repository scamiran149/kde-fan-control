//! Command-line arguments for the fan-control daemon.

use std::path::PathBuf;

use clap::Parser;

/// Command-line arguments for the fan-control daemon.
#[derive(Parser)]
#[command(name = "kde-fan-control-daemon")]
#[command(about = "Daemon for KDE Fan Control")]
pub struct DaemonArgs {
    /// Root path for hardware discovery (overrides default sysfs scan).
    #[arg(long)]
    pub root: Option<PathBuf>,

    /// Connect to the session bus instead of the system bus (for development).
    #[arg(long, default_value_t = false)]
    pub session_bus: bool,
}
