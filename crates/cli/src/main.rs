//! KDE Fan Control CLI.
//!
//! Command-line client that talks to the fan-control daemon over
//! DBus. Defines proxy traits for the three DBus interfaces
//! (Inventory, Lifecycle, Control) and dispatches subcommands
//! to the `commands` module.

mod commands;

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use zbus::proxy;

use commands::{control, friendly, inventory, lifecycle, status};

const TEMPERATURE_TARGET_PID_NOTE: &str =
    "v1 control is temperature-target PID, not RPM-target tracking.";

#[derive(Parser)]
#[command(name = "kde-fan-control")]
#[command(about = "Inspect and manage fan-control hardware and lifecycle")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Inventory {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        #[arg(long)]
        root: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        direct: bool,
    },
    Rename {
        id: String,
        name: String,
        #[arg(long, default_value_t = false)]
        fan: bool,
    },
    Unname {
        id: String,
        #[arg(long, default_value_t = false)]
        fan: bool,
    },
    /// Show the current draft configuration.
    Draft {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Show the current applied configuration.
    Applied {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Show the current degraded-state summary.
    Degraded {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Show recent lifecycle events.
    Events {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    /// Stage a fan enrollment change in the draft configuration.
    Enroll {
        /// Stable fan ID to enroll.
        fan_id: String,
        /// Whether the fan should be managed by the daemon.
        #[arg(long, default_value_t = true)]
        managed: bool,
        /// Control mode for the fan (pwm, voltage, or empty for none).
        #[arg(long, default_value = "none")]
        control_mode: String,
        /// Temperature source IDs for this fan's control loop.
        #[arg(long, num_args = 0.., value_delimiter = ',')]
        temp_sources: Vec<String>,
    },
    /// Remove a fan from the draft configuration.
    Unenroll {
        /// Stable fan ID to remove from the draft.
        fan_id: String,
    },
    /// Discard the entire draft configuration.
    Discard,
    /// Validate the current draft without applying it.
    Validate,
    /// Apply the current draft configuration.
    Apply,
    /// Show the current runtime state (managed, degraded, fallback, unmanaged).
    State {
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        detail: bool,
    },
    /// Stage PID control profile changes in the draft configuration.
    Control {
        #[command(subcommand)]
        command: ControlCommand,
    },
    /// Start, inspect, or accept auto-tune proposals.
    AutoTune {
        #[command(subcommand)]
        command: AutoTuneCommand,
    },
    /// Check authorization for privileged operations. Triggers polkit auth if available.
    Auth,
}

#[derive(Subcommand)]
enum ControlCommand {
    /// Stage PID control settings for a managed fan.
    Set {
        fan_id: String,
        #[arg(long)]
        target_temp: f64,
        #[arg(long, value_enum)]
        aggregation: AggregationArg,
        #[arg(long)]
        kp: f64,
        #[arg(long)]
        ki: f64,
        #[arg(long)]
        kd: f64,
        #[arg(long)]
        sample_ms: u64,
        #[arg(long)]
        control_ms: u64,
        #[arg(long)]
        write_ms: u64,
        #[arg(long)]
        deadband_mc: Option<i64>,
    },
}

#[derive(Subcommand)]
enum AutoTuneCommand {
    /// Start a bounded auto-tune run for a managed fan.
    Start { fan_id: String },
    /// Inspect the latest auto-tune result for a fan.
    Result { fan_id: String },
    /// Accept the latest completed auto-tune proposal into draft config.
    Accept { fan_id: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum AggregationArg {
    Average,
    Max,
    Min,
    Median,
}

impl AggregationArg {
    fn as_wire_value(self) -> &'static str {
        match self {
            Self::Average => "average",
            Self::Max => "max",
            Self::Min => "min",
            Self::Median => "median",
        }
    }
}

#[proxy(
    interface = "org.kde.FanControl.Inventory",
    default_path = "/org/kde/FanControl",
    default_service = "org.kde.FanControl"
)]
trait InventoryProxy {
    fn snapshot(&self) -> zbus::Result<String>;
    fn set_sensor_name(&self, id: &str, name: &str) -> zbus::Result<()>;
    fn set_fan_name(&self, id: &str, name: &str) -> zbus::Result<()>;
    fn remove_sensor_name(&self, id: &str) -> zbus::Result<()>;
    fn remove_fan_name(&self, id: &str) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.kde.FanControl.Lifecycle",
    default_path = "/org/kde/FanControl/Lifecycle",
    default_service = "org.kde.FanControl"
)]
trait LifecycleProxy {
    fn get_draft_config(&self) -> zbus::Result<String>;
    fn get_applied_config(&self) -> zbus::Result<String>;
    fn get_degraded_summary(&self) -> zbus::Result<String>;
    fn get_lifecycle_events(&self) -> zbus::Result<String>;
    fn get_runtime_state(&self) -> zbus::Result<String>;
    fn set_draft_fan_enrollment(
        &self,
        fan_id: &str,
        managed: bool,
        control_mode: &str,
        temp_sources: &[&str],
    ) -> zbus::Result<String>;
    fn remove_draft_fan(&self, fan_id: &str) -> zbus::Result<()>;
    fn discard_draft(&self) -> zbus::Result<()>;
    fn validate_draft(&self) -> zbus::Result<String>;
    fn apply_draft(&self) -> zbus::Result<String>;
    fn request_authorization(&self) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.kde.FanControl.Control",
    default_path = "/org/kde/FanControl/Control",
    default_service = "org.kde.FanControl"
)]
trait ControlProxy {
    fn get_control_status(&self) -> zbus::Result<String>;
    fn start_auto_tune(&self, fan_id: &str) -> zbus::Result<()>;
    fn get_auto_tune_result(&self, fan_id: &str) -> zbus::Result<String>;
    fn accept_auto_tune(&self, fan_id: &str) -> zbus::Result<String>;
    fn set_draft_fan_control_profile(
        &self,
        fan_id: &str,
        profile_json: &str,
    ) -> zbus::Result<String>;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Inventory {
            format,
            root,
            direct,
        } => inventory::run(format, &root, direct)?,
        Command::Rename { id, name, fan } => friendly::run_rename(&id, &name, fan)?,
        Command::Unname { id, fan } => friendly::run_unname(&id, fan)?,
        Command::Draft { format } => lifecycle::run_draft(format)?,
        Command::Applied { format } => lifecycle::run_applied(format)?,
        Command::Degraded { format } => lifecycle::run_degraded(format)?,
        Command::Events { format } => lifecycle::run_events(format)?,
        Command::Enroll {
            fan_id,
            managed,
            control_mode,
            temp_sources,
        } => lifecycle::run_enroll(&fan_id, managed, &control_mode, &temp_sources)?,
        Command::Unenroll { fan_id } => lifecycle::run_unenroll(&fan_id)?,
        Command::Discard => lifecycle::run_discard()?,
        Command::Validate => lifecycle::run_validate()?,
        Command::Apply => lifecycle::run_apply()?,
        Command::State { format, detail } => status::run(format, detail)?,
        Command::Control { command } => match command {
            ControlCommand::Set {
                fan_id,
                target_temp,
                aggregation,
                kp,
                ki,
                kd,
                sample_ms,
                control_ms,
                write_ms,
                deadband_mc,
            } => control::run_control_set(
                &fan_id,
                target_temp,
                aggregation,
                kp,
                ki,
                kd,
                sample_ms,
                control_ms,
                write_ms,
                deadband_mc,
            )?,
        },
        Command::AutoTune { command } => match command {
            AutoTuneCommand::Start { fan_id } => control::run_auto_tune_start(&fan_id)?,
            AutoTuneCommand::Result { fan_id } => control::run_auto_tune_result(&fan_id)?,
            AutoTuneCommand::Accept { fan_id } => control::run_auto_tune_accept(&fan_id)?,
        },
        Command::Auth => lifecycle::run_auth()?,
    }

    Ok(())
}

fn run_async<F, R>(future: F) -> Result<R, Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = Result<R, zbus::Error>>,
{
    let rt = tokio::runtime::Runtime::new()?;
    Ok(rt.block_on(future).map_err(|e| {
        let msg = format!("{}", e);
        if msg.contains("AccessDenied")
            || msg.contains("Access denied")
            || msg.contains("privileged")
            || msg.contains("authentication required")
            || msg.contains("root")
        {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Access denied: authentication required. Run with sudo or authenticate via polkit.",
            )) as Box<dyn std::error::Error>
        } else {
            Box::new(e) as Box<dyn std::error::Error>
        }
    })?)
}

async fn connect_dbus() -> zbus::Result<zbus::Connection> {
    match zbus::connection::Builder::system()?.build().await {
        Ok(c) => Ok(c),
        Err(_) => {
            // System bus is the normal daemon location. Fall back to session bus
            // for local development runs that explicitly use `--session-bus`.
            zbus::connection::Builder::session()?.build().await
        }
    }
}
