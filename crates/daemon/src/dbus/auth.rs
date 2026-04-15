//! Polkit/UID-based authorization for privileged DBus methods.
//!
//! Write methods on the DBus interfaces require the caller to be
//! authorized. This module implements authorization by:
//!
//! 1. Checking polkit `org.kde.fancontrol.write-config` with the
//!    caller's UID and PID as the subject.
//! 2. Falling back to UID-0 checking when polkit is unavailable.
//!
//! This matches the privilege model documented in `docs/dbus-api.md`.

use std::collections::HashMap;

use zbus::fdo;
use zbus::zvariant::Value;

/// Polkit action ID for write operations.
pub const POLKIT_ACTION_ID: &str = "org.kde.fancontrol.write-config";

/// Deny access when the test authorization flag is false.
///
/// Used in integration tests to verify that unauthorized callers
/// are rejected without requiring a real polkit setup.
#[allow(dead_code)]
pub fn require_test_authorized(authorized: bool) -> fdo::Result<()> {
    if authorized {
        Ok(())
    } else {
        Err(fdo::Error::AccessDenied(
            "privileged operations require root access".into(),
        ))
    }
}

/// Read process start time from `/proc/<pid>/stat` and convert to
/// microseconds since boot (as required by polkit's start-time field).
/// Returns `None` if the stat file cannot be parsed.
fn get_process_start_time(pid: u32) -> Option<u64> {
    let stat_path = format!("/proc/{pid}/stat");
    let stat_content = std::fs::read_to_string(&stat_path).ok()?;
    // Field 22 (1-indexed) is starttime (clock ticks since boot).
    // Format: pid (comm) state ppid pgrp session tty_nr tpgid flags ...
    // The comm field may contain spaces, so we find the closing ')' first.
    let rest = stat_content.rsplit(')').next()?;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // After comm's closing paren, the fields are:
    //   state(0) ppid(1) pgrp(2) session(3) tty_nr(4) tpgid(5) flags(6)
    //   min_flt(7) cmin_flt(8) maj_flt(9) cmaj_flt(10) utime(11) stime(12)
    //   cutime(13) cstime(14) priority(15) nice(16) num_threads(17)
    //   itrealvalue(18) starttime(19)
    let start_time_ticks = fields.get(19)?.parse::<u64>().ok()?;
    let clk_tck = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as u64;
    if clk_tck == 0 {
        return None;
    }
    // Convert ticks to microseconds for polkit
    Some(start_time_ticks * 1_000_000 / clk_tck)
}

/// Check whether the caller of a DBus method is authorized for privileged
/// operations. Tries polkit CheckAuthorization first; falls back to UID 0
/// if the polkit authority is unavailable.
pub async fn require_authorized(
    connection: &zbus::Connection,
    header: &zbus::message::Header<'_>,
) -> fdo::Result<()> {
    let sender = header
        .sender()
        .ok_or_else(|| fdo::Error::AccessDenied("no sender in message header".into()))?;

    let dbus_proxy = fdo::DBusProxy::new(connection).await.map_err(|e| {
        fdo::Error::AccessDenied(format!(
            "could not connect to DBus daemon for auth check: {e}"
        ))
    })?;

    let bus_name = zbus::names::BusName::Unique(sender.clone());
    let uid: u32 = dbus_proxy
        .get_connection_unix_user(bus_name.clone())
        .await
        .map_err(|e| fdo::Error::AccessDenied(format!("could not resolve caller identity: {e}")))?;

    let pid: u32 = dbus_proxy
        .get_connection_unix_process_id(bus_name.clone())
        .await
        .map_err(|e| {
            fdo::Error::AccessDenied(format!("cannot determine caller process ID: {e}"))
        })?;
    if pid == 0 {
        return Err(fdo::Error::AccessDenied(
            "caller process ID is 0 (kernel process) — authorization denied".into(),
        ));
    }

    match check_polkit_authorization(uid, pid).await {
        Ok(true) => Ok(()),
        Ok(false) => {
            tracing::warn!(caller_uid = uid, "polkit authorization denied");
            Err(fdo::Error::AccessDenied("authentication required".into()))
        }
        Err(e) => {
            tracing::warn!(error = %e, caller_uid = uid, "polkit unavailable, falling back to UID-0 check");
            if uid != 0 {
                tracing::warn!(caller_uid = uid, "unauthorized write attempt (no polkit)");
                return Err(fdo::Error::AccessDenied(
                    "privileged operations require root access (polkit unavailable)".into(),
                ));
            }
            Ok(())
        }
    }
}

/// Query the polkit authority for authorization on `POLKIT_ACTION_ID`.
///
/// Opens a **new** system-bus connection for the call (the daemon may be
/// running on the session bus in dev mode).
async fn check_polkit_authorization(uid: u32, pid: u32) -> Result<bool, String> {
    // Polkit lives on the system bus. The daemon may be running on the
    // session bus (dev mode), so always open a system-bus connection
    // for the polkit call rather than reusing the daemon's connection.
    let system_bus = zbus::connection::Builder::system()
        .map_err(|e| format!("system bus builder failed: {e}"))?
        .build()
        .await
        .map_err(|e| format!("system bus connection failed: {e}"))?;

    let subject_dict: HashMap<&str, Value<'_>> = {
        let mut m = HashMap::new();
        m.insert("pid", Value::from(pid));
        m.insert("uid", Value::from(uid));
        m.insert(
            "start-time",
            Value::from(get_process_start_time(pid).unwrap_or(0u64)),
        );
        m
    };

    let reply = system_bus
        .call_method(
            Some("org.freedesktop.PolicyKit1"),
            "/org/freedesktop/PolicyKit1/Authority",
            Some("org.freedesktop.PolicyKit1.Authority"),
            "CheckAuthorization",
            &(
                ("unix-process", subject_dict),
                POLKIT_ACTION_ID,
                HashMap::<&str, &str>::new(),
                1u32,
                "",
            ),
        )
        .await
        .map_err(|e| format!("CheckAuthorization call failed: {e}"))?;

    let body = reply.body();
    let result: (bool, bool, HashMap<String, String>) = body
        .deserialize()
        .map_err(|e| format!("CheckAuthorization deserialize failed: {e}"))?;

    if result.0 {
        tracing::debug!(caller_uid = uid, "polkit authorized");
    }
    Ok(result.0)
}
