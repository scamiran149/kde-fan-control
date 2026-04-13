# Safety Model

**Controlled fans must always fail to high speed.** A fan under daemon control must never be left at a low or unknown speed when the daemon stops, crashes, or loses access to a sensor.

This document describes the four layers of defense that enforce this principle, the BIOS compatibility guarantees, and the invariants that hold across all code paths.

---

## Four Layers of Defense

### Layer 1 — Graceful Shutdown

When the daemon receives `SIGTERM` or `SIGINT`:

1. `wait_for_shutdown_signal()` returns
2. `control.stop_all()` — aborts all control-task tokio tasks
3. `write_fallback_for_owned()` — iterates all owned fans and writes:
   - `pwmN_enable = 1` (manual mode)
   - `pwmN = 255` (maximum speed)
4. Records a `FallbackIncident` (timestamp, affected fan IDs, any write failures)
5. Persists the incident to `config.toml`
6. Exits

This is the happy path. Every fan gets maximum PWM before the process exits.

### Layer 2 — Panic Fallback Hook

Installed at daemon startup via `std::panic::set_hook`:

1. Intercepts any Rust panic
2. Reads `PanicFallbackMirror` — a `StdRwLock<Vec<(fan_id, pwm_path)>>` that mirrors the owned-fan paths without needing async/await
3. Writes `pwmN_enable = 1` and `pwmN = 255` for all mirrored paths using synchronous `std::fs::write`
4. Attempts to record a `FallbackIncident` using `try_read`/`try_write` on async locks (best-effort — locks may be poisoned)
5. Calls the previous panic hook for normal abort behavior

**Why a sync mirror?** The `PanicFallbackMirror` uses `std::sync::RwLock` (not tokio async `RwLock`) so it can be read from a panic context without an async runtime. It is synced from the async `OwnedFanSet` after every ownership change.

### Layer 3 — Runtime Degradation

During normal operation, a fan can be degraded if:

- **Temperature sensor becomes unreadable** — the fan cannot compute PID → write fallback for that fan, mark degraded, stop its control task
- **PWM write fails** — write fallback, release ownership (fan becomes unmanaged), mark degraded
- **Hardware disappears** — mark degraded

Degradation is **per-fan**. Other fans continue running normally.

### Layer 4 — Boot Reconciliation (Daemon Restart)

On next daemon start:

1. Load persisted applied config
2. Discover current hardware
3. For each fan in applied config: verify it still exists, is still enrollable, control mode still supported, temp sources still present
4. Restore passing fans as managed
5. Mark failing fans as degraded (recorded in lifecycle events)
6. Persist reconciled config (only valid fans)
7. Start control loops for restored fans

If a `FallbackIncident` was persisted from a previous crash, it is loaded and shown in lifecycle events. The incident is cleared once fans are successfully restored.

### Layer 5 — ExecStopPost Fallback Helper

A standalone binary (`kde-fan-control-fallback`) configured as systemd
`ExecStopPost=`. Runs **after** the daemon process exits, regardless of exit
status (clean exit, crash, SIGKILL). Reads the persisted owned-fan list from
`/var/lib/kde-fan-control/owned-fans.json` and writes `pwmN_enable=1` and
`pwmN=255` for each fan. No async, no DBus — uses `std::fs` only.

The daemon persists the owned-fan list after every ownership change (enrollment,
removal, boot reconciliation, apply). This ensures the fallback helper always
has an up-to-date view of which fans need recovery.

---

## BIOS Compatibility

- Fans **not** explicitly enrolled in the draft/applied config are **never** touched
- The daemon only writes to fans in its `OwnedFanSet`
- When a fan is released from management, it gets a fallback write to PWM 255 **first**
- After release, the fan returns to whatever the BIOS/EC decides (typically automatic thermal control)

---

## What Happens When…

| Scenario | Behavior |
|---|---|
| **Daemon crashes (Rust panic)** | Panic hook fires → writes PWM 255 for all owned fans via `PanicFallbackMirror` → records incident → process aborts. Systemd `Restart=on-failure` restarts the daemon → boot reconciliation restores managed fans. |
| **Daemon killed (`SIGKILL`)** | No in-process cleanup possible. Systemd `ExecStopPost=` runs the fallback helper (`kde-fan-control-fallback`), which reads `/var/lib/kde-fan-control/owned-fans.json` and writes PWM 255 for each listed fan. Systemd then restarts the daemon → boot reconciliation restores managed fans. |
| **Daemon upgraded (package update)** | prerm stops daemon → graceful shutdown path → fans at PWM 255 → postinst starts new daemon → boot reconciliation. |
| **Hardware disappears mid-run** | Control task detects write failure → writes targeted fallback → releases ownership → marks degraded → other fans continue. |
| **Sensor disappears mid-run** | Sample interval fails to read temp → `degrade_and_stop()` → fan gets fallback → control task terminates. |
| **System loses power** | Hardware reverts to BIOS fan control (safe). On next boot, daemon starts and reconciles. |

---

## FallbackIncident Persistence

- Written to `config.toml` alongside draft and applied config
- Survives process exit
- Contains: timestamp, affected fan IDs, any write failures, trigger detail
- Inspectable via CLI: `kde-fan-control events`
- Inspectable via DBus: `GetLifecycleEvents()`
- Cleared when boot reconciliation successfully restores fans

---

## Invariant Summary

1. **Only owned fans receive fallback writes** — unmanaged fans are never touched
2. **Every ownership change syncs the `PanicFallbackMirror`**
3. **PWM safe maximum is always 255** (full speed)
4. **Fallback always attempts `pwm_enable=1` (manual) before `pwm=255`**
5. **Partial fallback is OK** — if some fans fail to write, others still get max
6. **Config version check rejects future schema versions** (prevents data corruption)