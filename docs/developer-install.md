# Developer Install

Use `scripts/dev-install.sh` to install the local-testing integration files without doing a full system package install.

## What it installs

- polkit action: `/usr/local/share/polkit-1/actions/org.kde.fancontrol.policy`
- desktop entry: `/usr/local/share/applications/org.kde.fancontrol.desktop`
- icons:
  - `/usr/local/share/icons/hicolor/scalable/apps/org.kde.fancontrol.svg`
  - `/usr/local/share/icons/hicolor/48x48/apps/org.kde.fancontrol.png`
  - `/usr/local/share/icons/hicolor/128x128/apps/org.kde.fancontrol.png`
- DBus policy: `/usr/local/share/dbus-1/system.d/org.kde.FanControl.conf`
- DBus activation file: `/usr/local/share/dbus-1/system-services/org.kde.FanControl.service`
- systemd unit: `/etc/systemd/system/kde-fan-control-daemon.service`
- copied binaries:
  - `/usr/local/libexec/kde-fan-control-daemon`
  - `/usr/local/libexec/kde-fan-control-fallback`
  - `/usr/local/bin/kde-fan-control-gui`

The installed systemd unit is generated from `packaging/systemd/kde-fan-control-daemon.service` but rewritten to use the copied binaries above. This avoids `ProtectHome=yes` conflicts when your build tree lives under `/home/...`. The installer also refreshes the desktop and icon caches, plus `kbuildsycoca6` when available, so Plasma picks up the launcher icon quickly.

## Build first

```bash
cargo build --release
cmake -B gui/build -S gui
cmake --build gui/build
```

Use `--debug` with the script if you want it to install from `target/debug/` instead.

## Install

```bash
sudo ./scripts/dev-install.sh install --release
sudo systemctl enable --now kde-fan-control-daemon
```

Re-run the install command after rebuilding if you want to refresh the copied binaries in `/usr/local`.

## Uninstall

```bash
sudo ./scripts/dev-install.sh uninstall
```

The uninstall step stops and disables `kde-fan-control-daemon` if it is currently installed through this local-testing flow.
