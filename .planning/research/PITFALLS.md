# Domain Pitfalls

**Domain:** Linux desktop fan-control daemon
**Researched:** 2026-04-10
**Overall confidence:** MEDIUM-HIGH

## Critical Pitfalls

### Pitfall 1: Relying on “clean shutdown” for fail-safe fan recovery
**What goes wrong:** The daemon crashes, deadlocks, or is watchdog-killed, but the recovery path only exists inside the daemon or in a clean-stop path. Fans stay at a stale low PWM instead of being forced safe.

**Why it happens:** systemd `ExecStop=` is not the right crash-only safety mechanism, and it only runs after a successful start. A daemon cannot recover hardware state after its own process is already wedged or gone.

**Consequences:** Thermal runaway risk; safety claims become false exactly in the failure mode that matters most.

**Warning signs:**
- Safety design says “on exit we will set max fan” but only inside the main process
- Service unit has no `WatchdogSec=` and no `Restart=on-failure`
- No out-of-process fallback helper for crash cleanup
- Testing only covers `systemctl stop`, not `kill -9`, segfault, startup failure, or watchdog timeout

**Prevention strategy:**
- Treat fail-safe recovery as a separate mechanism, not daemon business logic
- Use a supervised system service (`Type=notify` or `Type=dbus`), `Restart=on-failure`, and `WatchdogSec=`
- Add an external recovery path that can still run after unexpected exit, e.g. `ExecStopPost=` helper that forces enrolled channels to known-safe maximum/manual-safe state
- Persist the exact set of daemon-owned channels so recovery code knows what to restore
- Test at least: startup failure, unexpected crash, watchdog expiry, stop timeout, and reboot during active control

**Which phase should address it:** Phase 1 — daemon lifecycle and safety contract

### Pitfall 2: Identifying hardware by unstable `hwmonN` paths
**What goes wrong:** A saved configuration points to `hwmon2/pwm1` and next boot that path is a different device. The daemon drives the wrong controller or refuses to start.

**Why it happens:** Kernel docs explicitly warn that sysfs layout details and classification paths are not stable; `hwmonN` numbering is not a durable identity.

**Consequences:** Wrong-fan control, silent misconfiguration after reboot, or unsafe boot-time auto-management.

**Warning signs:**
- Config stores only `/sys/class/hwmon/hwmonX/...`
- No DEVPATH/name validation at load time
- Discovery code assumes `/sys/class/hwmon` numbering is persistent
- Users report config “works until reboot/kernel update”

**Prevention strategy:**
- Store stable identity using real device devpath plus chip name/labels, not transient class numbering alone
- Resolve symlinks to real `/sys/devices/...` paths before persisting identity
- On load, verify both physical device path and expected chip/controller names before reenrolling
- Refuse auto-start if identity confidence drops; fall back to BIOS/manual state instead of guessing
- Mirror `fancontrol`’s defensive idea of checking `DEVPATH`/`DEVNAME` before applying config

**Which phase should address it:** Phase 1 — hardware discovery and persistence model

### Pitfall 3: Assuming writable PWM means “safe supported hardware”
**What goes wrong:** The daemon exposes enrollment for channels that are writable but not safely controllable in practice: no trustworthy temperature source, no safe maximum path, unclear mapping, broken tach, EC/firmware override, or partial hwmon exposure.

**Why it happens:** Kernel hwmon attributes are heavily optional; motherboard wiring is inconsistent; temperature/fan numbering is not semantically standardized.

**Consequences:** Users enroll unsupported hardware, lose BIOS behavior, or gain false confidence in control that is only partially real.

**Warning signs:**
- Enrollment UI treats all discovered PWM outputs as equal
- No validation that manual control can be entered and maximum can be enforced
- No distinction between “readable”, “writable”, and “safe to own continuously”
- Fault/alarm attributes are ignored

**Prevention strategy:**
- Introduce capability tiers: observable, writable, safe-for-daemon-control, unsupported
- Require a positive safety check before enrollment: can enter manual mode, can write output, can force safe high output, can map to at least one trusted sensor path
- Consume `*_fault`, `*_alarm`, and missing-attribute states as safety signals, not cosmetic metadata
- Keep BIOS-controlled fans opt-in only; never auto-adopt them because they are writable
- For ambiguous channels, expose read-only diagnostics and explicitly refuse enrollment

**Which phase should address it:** Phase 1 — hardware support matrix and enrollment rules

### Pitfall 4: Control loop tuned faster than the hardware can report meaningful data
**What goes wrong:** PID runs at a high frequency against stale or quantized temperature readings. The derivative term amplifies noise, the integral term winds up, and PWM oscillates audibly while temperatures barely improve.

**Why it happens:** hwmon devices expose `update_interval`; thermal zones may have their own polling cadence; many sensors update slowly relative to software loops. Some temperature channels also need user-space conversion/labeling, and not all data is equally trustworthy.

**Consequences:** Fan hunting, acoustic annoyance, poor thermal stability, unstable auto-tuning, and user distrust of PID.

**Warning signs:**
- PID tick interval chosen arbitrarily (e.g. 50–100 ms)
- D term enabled before measurement noise is characterized
- Temperature graph shows stepwise plateaus while PWM chatters rapidly
- Integrator grows while output is already clamped at min/max

**Prevention strategy:**
- Make control cadence sensor-aware: never sample faster than hardware updates can justify
- Implement output clamps, anti-windup, deadband/hysteresis, and slew-rate limiting from day one
- Default to PI or even bounded linear control first; keep D conservative and optional
- Smooth measurements deliberately and document control latency tradeoffs
- Read back actual sensor cadence during discovery and store it with the control model

**Which phase should address it:** Phase 2 — control-loop core before auto-tuning

### Pitfall 5: Forgetting that some “temperature” channels are not millidegrees Celsius
**What goes wrong:** The daemon assumes every `temp*_input` is millidegrees C, but some chips expose thermistor-derived temperatures as millivolts and rely on user-space conversion.

**Why it happens:** hwmon mostly looks uniform, but the kernel docs explicitly call out exceptions where temperature channels are handled as voltage channels by the driver.

**Consequences:** Wildly wrong setpoints, nonsense auto-tuning, or unsafe low-fan behavior because the control loop is built on mis-scaled data.

**Warning signs:**
- All `temp*` files are parsed with one fixed unit path and no device-specific metadata
- Sensor labels and units are not surfaced in diagnostics
- Same board works under `sensors`, but daemon reports implausible temperatures

**Prevention strategy:**
- Separate raw sensor ingestion from normalized thermal signals
- Keep unit metadata explicit per sensor channel
- Compare discovered channels against known labels/conversions where available
- Refuse control on channels with ambiguous units until normalized confidently

**Which phase should address it:** Phase 1 — sensor normalization and labeling

### Pitfall 6: Writing malformed sysfs values and accidentally sending fans to 0
**What goes wrong:** Bad parsing, empty strings, locale issues, or invalid user input end up being written to sysfs. hwmon docs note that non-numeric strings are interpreted as `0` when kernel code parses them.

**Why it happens:** Developers assume sysfs writes fail cleanly on malformed input; some attributes clamp or coerce values instead.

**Consequences:** Accidental fan stop, wrong mode switch, or silent mismatch between requested and actual output.

**Warning signs:**
- UI/CLI forwards strings directly to sysfs without strict typed validation
- No read-after-write verification
- No clamping in daemon before touching hardware
- Error handling assumes `EINVAL` will always catch bad writes

**Prevention strategy:**
- Centralize all hardware writes in a typed validation layer
- Clamp and normalize values in userspace before any write
- Perform read-after-write verification for mode switches and critical outputs
- Reject partial/invalid config updates before they reach hardware

**Which phase should address it:** Phase 1 — hardware write abstraction

## Moderate Pitfalls

### Pitfall 7: Fighting the kernel thermal framework, firmware, or the EC
**What goes wrong:** The daemon’s PID loop competes with ACPI/thermal-zone policy, embedded controller logic, or chipset auto mode. Output appears to “bounce back” or temperature control is inconsistent.

**Why it happens:** Linux already models thermal zones and cooling devices; some platforms retain independent firmware behavior even when user space writes hwmon controls.

**Consequences:** Oscillation, confusing bug reports, “manual mode doesn’t stick,” and false blame on PID math.

**Warning signs:**
- PWM value changes but actual RPM or thermal behavior ignores it
- Control works briefly, then snaps back
- Same fan appears in both hwmon and thermal/cooling-device views

**Prevention strategy:**
- Detect and surface ownership conflicts during discovery
- Explicitly switch to manual mode only for enrolled fans, and confirm the switch stuck
- Document unsupported platforms where EC/firmware keeps final control
- Prefer refusal over pretending control exists

**Which phase should address it:** Phase 1 — discovery, ownership, and support-policy phase

### Pitfall 8: Boot-time auto-management before devices are actually ready
**What goes wrong:** The service starts on boot, but target hwmon nodes are not populated yet, driver load order differs, or the device graph changed. The daemon applies a partial config or fails into an unsafe half-managed state.

**Why it happens:** Boot ordering is real, and systemd only guarantees what the unit declares. Hardware discovery can lag behind service startup.

**Consequences:** Fans left in wrong mode after boot, flaky startup behavior, inconsistent bug reproduction.

**Warning signs:**
- Boot-only failures that disappear when restarting the service manually
- Missing channels on first start but present seconds later
- Auto-start logic applies what it found instead of requiring full validation

**Prevention strategy:**
- Treat boot activation as a reconciliation step, not blind config replay
- Retry discovery for a bounded window, then fail closed
- Require full identity validation before taking ownership of any enrolled fan
- If validation fails, keep or restore safe non-daemon state and log why

**Which phase should address it:** Phase 1 — boot integration and activation semantics

### Pitfall 9: Split-brain between daemon, GUI, and CLI
**What goes wrong:** Frontends cache config, write files directly, or infer hardware state independently. DBus becomes only “one way” to talk to the daemon instead of the single source of truth.

**Why it happens:** It is tempting to let each client manipulate persistence or discover hardware locally for convenience.

**Consequences:** Conflicting state, stale UI, race conditions during edits, and support headaches.

**Warning signs:**
- GUI and CLI can disagree until refresh
- More than one component writes persistent config
- Clients refer to hwmon paths directly instead of opaque daemon-owned IDs

**Prevention strategy:**
- Make the daemon authoritative for discovery, persistence, validation, and runtime state
- Expose opaque IDs and versioned DBus interfaces/object paths; do not leak raw sysfs assumptions into client contracts
- Send change notifications/signals for every state transition the UI must reflect
- Model edits as explicit transactions or replace-the-active-config operations

**Which phase should address it:** Phase 2 — DBus API and client contract

### Pitfall 10: Auto-tuning without hard thermal guardrails
**What goes wrong:** Auto-tune experiments drive fans too low or excite the system aggressively while trying to identify response curves.

**Why it happens:** Thermal systems are slow, noisy, and hardware-specific. “Basic auto-tune” sounds simple but is where safe operating envelopes are easiest to violate.

**Consequences:** Dangerous overshoot, user distrust, and support burden on edge-case hardware.

**Warning signs:**
- Auto-tune described as a generic one-click action
- No max temperature abort, max duration, or min PWM floor during tuning
- Tuning runs on every boot or in background continuously

**Prevention strategy:**
- Ship manual/assisted tuning first; keep auto-tune opt-in
- Enforce hard abort thresholds, bounded dwell times, and minimum safe outputs during tuning
- Restrict v1 auto-tune to hardware that already passed safety validation
- Log every tuning step so failures are diagnosable

**Which phase should address it:** Phase 3 — tuning and advanced control research

## Minor Pitfalls

### Pitfall 11: No output deadband or minimum dwell time
**What goes wrong:** Small temperature noise causes frequent tiny PWM changes that users hear immediately even if thermals are fine.

**Prevention strategy:**
- Add deadband, output quantization, and minimum time between materially different fan commands
- Optimize for acoustics and stability, not only temperature error minimization

**Warning signs:**
- Logs show constant 1–2 PWM-step changes
- Users complain about “nervous” fans while graphs look thermally stable

**Which phase should address it:** Phase 2 — control-loop polish

### Pitfall 12: Unversioned DBus API names and paths
**What goes wrong:** The first shipped DBus contract becomes hard to change without breaking GUI/CLI compatibility.

**Prevention strategy:**
- Follow D-Bus naming conventions with reverse-DNS names and an explicit major version in interface/object-path naming from day one

**Warning signs:**
- API names look like `org.example.FanControl` with no versioning plan
- Client code depends on ad-hoc method shapes not intended as stable API

**Which phase should address it:** Phase 2 — DBus API design

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Hardware discovery | Misidentifying devices via `hwmonN` | Persist real devpath + chip identity; verify before reapply |
| Enrollment UX | Offering unsupported channels as controllable | Capability tiers; explicit refusal for unsafe hardware |
| Safe writes | Invalid sysfs write becomes `0` | Typed validation + clamp + read-back verification |
| Service lifecycle | Crash leaves fans slow | External recovery helper + watchdog + restart policy |
| Boot auto-start | Partial discovery on first boot | Bounded retry and fail-closed ownership acquisition |
| PID loop | Sampling faster than sensor update cadence | Sensor-aware polling, anti-windup, deadband |
| Auto-tune | Thermal overshoot during identification | Hard abort limits, safe floors, opt-in only |
| DBus API | Split-brain and future breakage | Daemon-owned state, signals, versioned interfaces |

## Recommended Roadmap Flags

- **Must research deeply before implementation:** crash-safe failover, hardware enrollment criteria, boot-time activation semantics, PID sampling strategy
- **Can implement with standard patterns:** daemon-owned persistence, versioned DBus API, client synchronization via signals
- **Should be deferred until core loop is proven:** aggressive derivative defaults, generic auto-tuning, fuzzy/self-tuning control

## Sources

### HIGH confidence
- Linux kernel hwmon sysfs interface: https://www.kernel.org/doc/html/latest/hwmon/sysfs-interface.html
- Linux kernel sysfs rules: https://www.kernel.org/doc/html/latest/admin-guide/sysfs-rules.html
- Linux kernel thermal sysfs API: https://www.kernel.org/doc/html/latest/driver-api/thermal/sysfs-api.html
- systemd service semantics: https://man7.org/linux/man-pages/man5/systemd.service.5.html
- D-Bus specification: https://dbus.freedesktop.org/doc/dbus-specification.html

### MEDIUM confidence
- `fancontrol(8)` manual, useful for defensive config validation patterns and smoothing concepts: https://man.archlinux.org/man/fancontrol.8.en

### LOW confidence / expert-judgment synthesis
- PID-specific recommendations here (anti-windup, deadband, conservative D-term use, guarded auto-tuning) are based on control-systems best practice synthesized against the documented Linux sensor/update constraints above, not on a single official Linux fan-control spec.
