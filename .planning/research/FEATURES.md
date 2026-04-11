# Feature Landscape

**Domain:** Linux desktop fan-control software
**Project:** KDE Fan Control
**Researched:** 2026-04-10
**Overall confidence:** HIGH

## Executive Take

The ecosystem splits into three tiers:

1. **`fancontrol` / `thinkfan` style tools**: minimal daemon-or-script control over existing kernel-exposed fan channels.
2. **Laptop/vendor-specific tools** such as **NBFC-Linux**: broader hardware reach via model configs and embedded-controller access, but much higher safety and support burden.
3. **Full desktop control suites** such as **CoolerControl**: daemon + UI + reusable profiles + alerts + dashboards + GPU/AIO support.

For a product explicitly replacing `fancontrol`, users mainly expect: **reliable hardware discovery, clear mapping of sensors to controllable fans, automatic temperature-based control, boot persistence, manual/BIOS handoff, and strong safety behavior when the daemon exits or hardware is only partially supported**. Rich profile systems, remote access, GPU/AIO expansion, and advanced automation exist in the market, but they are not required for a safe useful v1.

## What Existing Linux Fan-Control Products Commonly Offer

| Product / category | Commonly offered features | What it implies for KDE Fan Control | Confidence |
|---|---|---|---|
| `fancontrol` (`lm-sensors`) | Mapping PWM outputs to temperature inputs, min/max temps, min start/stop PWM, averaging, config validation, service startup | Baseline replacement target is still very operational and sysfs-centric | HIGH |
| `thinkfan` | Lightweight daemon, temperature thresholds, service startup, hwmon + ThinkPad-specific interfaces, simple configs | Users accept lightweight daemonized control if it is predictable and safe | HIGH |
| NBFC-Linux | Per-model configs, auto/manual mode, boot service, fan sensor/source selection, avg/min/max aggregation, GUI add-ons | Sensor selection and aggregation are valuable; EC/model-config ecosystem is powerful but dangerous scope creep | HIGH |
| CoolerControl | GUI, daemon, profiles, hysteresis/functions, custom sensors, alerts, dashboards, mode switching, GPU/AIO support, Web UI/API | Rich UX and reusable profiles are differentiators, not v1 table stakes | HIGH |
| ArchWiki ecosystem overview | Vendor-specific exceptions, BIOS conflicts, suspend/resume quirks, unstable hwmon paths, partial hardware support | Safe enrollment and explicit unsupported-state UX matter as much as control features | HIGH |

## Table Stakes

Features users replacing `fancontrol` will expect. Missing these means the product feels incomplete or unsafe.

| Feature | Why Expected | Complexity | Dependencies / notes |
|---|---|---|---|
| Hardware discovery of sensors, fan tach inputs, and controllable PWM/voltage channels | `fancontrol` and related tools begin from discovered hwmon/sysfs capabilities; users expect the app to see what the kernel exposes | Med | Requires robust sysfs/hwmon scan and stable identity model |
| Capability validation before enrollment | Existing Linux tools often fail on partial hardware; users need to know which fans are actually safe to manage | High | Discovery → writeability tests → max-speed fallback validation |
| Per-fan enrollment: leave under BIOS/firmware control or hand to daemon | `pwm1_enable` style auto/manual handoff is core Linux fan-control behavior | Med | Requires clear ownership model per fan |
| Temperature-based automatic control for each enrolled fan | This is the core reason to replace `fancontrol` | High | Requires sensor mapping and control loop |
| Per-fan sensor source selection | `fancontrol` maps fans to temps; NBFC-Linux and CoolerControl both expose source selection | Med | Discovery must include temp sensors and labels |
| Sensible minimum/maximum output bounds plus safe start behavior | `fancontrol` exposes `MINSTART`, `MINSTOP`, `MINPWM`, `MAXPWM`; users expect protection against non-spinning fans | High | Requires tach-aware validation where available |
| Persistent single active configuration | Users expect the machine to come back with the same cooling behavior after reboot | Low | Depends on daemon-owned persistence |
| Boot auto-start of managed fans | `fancontrol`, `thinkfan`, and NBFC-Linux are all service-oriented | Low | Persistence → system service integration |
| Live monitoring: current temp, fan RPM, control state, target/output | Users need to verify the daemon is doing the right thing | Med | Discovery + runtime state model |
| Manual override / return-to-auto behavior | Linux users expect to be able to stop managing a fan and hand it back to firmware | Med | Per-fan ownership + safe writeback |
| Clear unsupported/partially supported hardware reporting | ArchWiki documents many cases where only some fans or sensors work; silent failure is unacceptable | Med | Capability model + UI/CLI surfacing |
| Fail-safe on daemon exit: force managed fans to safe high speed or restore known-safe mode | Replacing `fancontrol` safely requires better failure behavior, not just nicer UX | High | Enrollment validation + shutdown/error path |
| Suspend/resume recovery or explicit re-apply behavior | Existing tools commonly need restart/reapply after suspend | Med | Service lifecycle hooks |
| CLI-level inspectability | Linux users replacing config-file tools expect scriptable inspection, even if GUI is primary | Med | DBus/API boundary recommended |

## Strong v1 Differentiators Worth Building Early

These are not universal table stakes, but they materially improve the product and align with the project brief.

| Feature | Value Proposition | Complexity | Why it is worth early inclusion |
|---|---|---|---|
| Friendly names for sensors and fans | Sysfs labels are often cryptic; naming dramatically improves usability | Low | Very high UX leverage for little engineering cost |
| Sensor aggregation (`max`, `avg`, `min`, `median`) | Desktop cooling often cares about multiple heat sources, not one temp input | Med | Directly solves a real limitation of plain `fancontrol` |
| Per-fan PID control with understandable parameters | More flexible and precise than static threshold/linear curves | High | This is a genuine product differentiator if made understandable |
| Basic PID auto-tuning | Makes PID usable for non-experts | High | Valuable, but only if constrained and conservative |
| Native KDE/Qt6 GUI + tray | Most Linux fan tools are utilitarian; a polished native UI is differentiated | Med | Important to product identity |
| Unified daemon + GUI + CLI over one API | Prevents split-brain configuration and improves operability | Med | Important structural differentiator, not just UX |
| Explicit safe-enrollment workflow | A step that proves controllability and fail-safe behavior before enabling control | High | Safety feature that also differentiates trustworthiness |

## Differentiators to Defer Until After v1

These exist in the ecosystem, especially in CoolerControl, but should not be required for the first safe release.

| Feature | Why users like it | Why defer | Complexity |
|---|---|---|---|
| Multiple saved profiles / global modes (Silent, Gaming, etc.) | Convenient switching for different workloads | Adds config model complexity before the single-policy path is proven | Med |
| Rich fan-curve editor alongside PID | Familiar mental model for many users | Doubles policy surface area; v1 should make one control model trustworthy first | Med |
| Alerts / notifications for anomalies | Useful for monitoring and troubleshooting | Valuable, but secondary to core control correctness | Med |
| Dashboards / historical graphs | Nice observability | Monitoring polish, not control core | Med |
| Custom sensors from files/commands | Powerful for experts | Expands trust boundary and validation burden | Med |
| System-wide mode switching across all devices | Strong UX for advanced suites | Needs mature profile system first | Med |
| REST/Web UI / remote access | Useful for headless or remote management | Out of scope for local KDE-first desktop product | High |

## Anti-Features / Premature Complexity for a Safe Useful v1

These should be explicitly out of v1.

| Anti-Feature | Why Avoid | What to Do Instead |
|---|---|---|
| Broad vendor-specific laptop EC support (NBFC-style model database) | Massive support surface, easy to brick thermals, constant config maintenance | Focus v1 on kernel-exposed hwmon/sysfs controls only |
| GPU fan control as a primary v1 target | AMD/NVIDIA behavior differs, AMD has separate auto/manual/fan-curve semantics, and compatibility is uneven | Keep architecture open for GPU support later; ship motherboard/sysfs desktop fans first |
| Liquidctl/AIO/RGB device integration | Quickly turns fan control into a full hardware-control suite | Defer until core sysfs daemon is trusted |
| Multiple concurrent config writers (editing files directly plus GUI plus CLI) | Split-brain state is a reliability bug factory | Make daemon the only authority |
| Continuous/adaptive/fuzzy/self-learning control beyond basic auto-tuning | Hard to validate, hard to explain, risky under thermal load | Ship conservative fixed PID with optional basic guided tuning |
| Automatic enrollment of every detected fan | Unsafe on partially supported hardware | Require explicit per-fan enrollment with validation |
| “Support everything” hardware promise | Linux fan control is hardware-fragmented; false confidence is dangerous | Be explicit about supported, unsupported, and uncertain states |
| Overclocking / power / performance-tuning controls | Different product category and safety model | Stay focused on cooling control |

## Feature Dependencies

```text
Hardware discovery
  → Stable device identity
  → Capability validation
  → Safe enrollment

Safe enrollment
  → Per-fan BIOS/daemon ownership
  → Automatic control enablement
  → Fail-safe exit behavior

Temperature discovery
  → Sensor naming
  → Per-fan source selection
  → Sensor aggregation

Per-fan source selection + safe output bounds
  → PID control loop
  → Basic auto-tuning

Daemon-owned runtime state
  → Persistence
  → Boot auto-start
  → GUI/CLI parity via DBus

Suspend/resume handling
  → Re-apply managed state after wake
```

## Recommended v1 Scope

Prioritize:

1. **Discovery + safe enrollment**
   - Detect sensors/fans/controls
   - Show unsupported and partially supported hardware clearly
   - Prove safe max-output fallback before enrollment
2. **One reliable automatic control path**
   - Per-fan sensor selection
   - Sensor aggregation (`avg`, `max`, `min`, `median`)
   - Per-fan target temperature + PID loop
   - Conservative min/max output limits
3. **Daemon-owned persistence and lifecycle**
   - Single active config
   - Boot auto-start
   - Suspend/resume re-apply
   - Fail-safe on daemon crash/exit
4. **Usable control surfaces**
   - KDE GUI + tray
   - CLI for inspection/configuration
   - Friendly labels and clear runtime state

## Explicitly Deferred for v1

Defer these even if they are attractive:

- Multiple profiles / mode switching
- GPU fan control
- AIO / liquidctl integration
- Web UI / remote access
- Alerts, dashboards, and historical graphs
- Arbitrary file/command-based custom sensors
- NBFC-style embedded-controller laptop coverage
- Advanced adaptive/fuzzy/self-learning control

## MVP Recommendation

**Ship a trustworthy daemonized replacement for `fancontrol`, not a Linux-wide cooling super-suite.**

That means v1 should be judged by these questions:

1. Can it discover and explain what hardware is controllable?
2. Can it safely enroll only the fans it can actually protect?
3. Can it control each enrolled fan from a sensible temp source or aggregate?
4. Can it survive reboot/suspend/crash without leaving the machine unsafe?
5. Can a Linux desktop user understand and verify what it is doing?

If the answer is yes, v1 is useful. If the product instead chases GPU support, profiles, remote UI, and vendor-specific laptop hacks before those five are solid, it will feel ambitious but not trustworthy.

## Sources

- `fancontrol(8)` Arch manual page — automated software-based fan speed regulation, config variables, validation behavior. HIGH. https://man.archlinux.org/man/fancontrol.8.en
- Linux kernel hwmon sysfs interface — canonical attributes for fans, PWM, temperatures, labels, auto points. HIGH. https://docs.kernel.org/hwmon/sysfs-interface.html
- CoolerControl getting started / feature docs — current feature set for a full Linux cooling suite. HIGH. https://coolercontrol.org/getting-started.html
- thinkfan README — lightweight daemon positioning and current release context. HIGH. https://raw.githubusercontent.com/vmatare/thinkfan/master/README.md
- NBFC-Linux README — model-config workflow, sensor selection, aggregation, auto/manual behavior, fail-safe design claims. HIGH. https://raw.githubusercontent.com/nbfc-linux/nbfc-linux/master/README.md
- ArchWiki fan speed control — ecosystem overview, vendor-specific paths, suspend/path instability issues, CoolerControl mention. MEDIUM-HIGH. https://wiki.archlinux.org/title/Fan_speed_control
- Linux kernel AMDGPU thermal docs — shows why GPU fan support is a separate capability surface and should be deferred unless intentionally targeted. HIGH. https://docs.kernel.org/gpu/amdgpu/thermal.html
