#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: sudo ./scripts/dev-install.sh <install|uninstall> [--debug|--release]

Installs or removes local-testing integration files for KDE Fan Control.

Options:
  --debug    Use target/debug artifacts
  --release  Use target/release artifacts (default)
EOF
}

if [[ $# -lt 1 ]]; then
  usage
  exit 1
fi

command="$1"
shift

profile="release"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --debug)
      profile="debug"
      ;;
    --release)
      profile="release"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown option: %s\n' "$1" >&2
      usage
      exit 1
      ;;
  esac
  shift
done

if [[ "$(id -u)" -ne 0 ]]; then
  printf 'This script must run as root.\n' >&2
  exit 1
fi

repo_root="$(realpath "$(dirname "$0")/..")"
artifact_root="$repo_root/target/$profile"
gui_build_root="$repo_root/gui/build"

daemon_src="$artifact_root/kde-fan-control-daemon"
fallback_src="$artifact_root/kde-fan-control-fallback"
gui_src="$gui_build_root/kde-fan-control-gui"

polkit_src="$repo_root/packaging/polkit/org.kde.fancontrol.policy"
desktop_src="$repo_root/packaging/org.kde.fancontrol.desktop"
icon_svg_src="$repo_root/packaging/icons/hicolor/scalable/apps/org.kde.fancontrol.svg"
icon_48_src="$repo_root/packaging/icons/hicolor/48x48/apps/org.kde.fancontrol.png"
icon_128_src="$repo_root/packaging/icons/hicolor/128x128/apps/org.kde.fancontrol.png"
dbus_policy_src="$repo_root/packaging/dbus/org.kde.FanControl.conf"
dbus_service_src="$repo_root/packaging/dbus/org.kde.FanControl.service"
systemd_src="$repo_root/packaging/systemd/kde-fan-control-daemon.service"
qml_qmldir_src="$gui_build_root/org/kde/fancontrol/qmldir"
qml_qmltypes_src="$gui_build_root/org/kde/fancontrol/gui_app.qmltypes"

polkit_dest_dir="/usr/local/share/polkit-1/actions"
desktop_dest_dir="/usr/local/share/applications"
icon_theme_dir="/usr/local/share/icons/hicolor"
icon_svg_dest_dir="$icon_theme_dir/scalable/apps"
icon_48_dest_dir="$icon_theme_dir/48x48/apps"
icon_128_dest_dir="$icon_theme_dir/128x128/apps"
dbus_policy_dest_dir="/usr/local/share/dbus-1/system.d"
dbus_service_dest_dir="/usr/local/share/dbus-1/system-services"
systemd_dest_dir="/etc/systemd/system"
bin_dest_dir="/usr/local/bin"
libexec_dest_dir="/usr/local/libexec"
qml_dest_dir="/usr/lib/x86_64-linux-gnu/qt6/qml/org/kde/fancontrol"

polkit_dest="$polkit_dest_dir/org.kde.fancontrol.policy"
desktop_dest="$desktop_dest_dir/org.kde.fancontrol.desktop"
icon_svg_dest="$icon_svg_dest_dir/org.kde.fancontrol.svg"
icon_48_dest="$icon_48_dest_dir/org.kde.fancontrol.png"
icon_128_dest="$icon_128_dest_dir/org.kde.fancontrol.png"
dbus_policy_dest="$dbus_policy_dest_dir/org.kde.FanControl.conf"
dbus_service_dest="$dbus_service_dest_dir/org.kde.FanControl.service"
systemd_dest="$systemd_dest_dir/kde-fan-control-daemon.service"
daemon_dest="$libexec_dest_dir/kde-fan-control-daemon"
fallback_dest="$libexec_dest_dir/kde-fan-control-fallback"
gui_dest="$bin_dest_dir/kde-fan-control-gui"

install_file() {
  local src="$1"
  local dest="$2"
  install -Dm644 "$src" "$dest"
}

install_executable() {
  local src="$1"
  local dest="$2"
  install -Dm755 "$src" "$dest"
}

install_systemd_unit() {
  install -d "$systemd_dest_dir"
  sed \
    -e "s|^ExecStart=.*$|ExecStart=$daemon_dest|" \
    -e "s|^ExecStopPost=.*$|ExecStopPost=$fallback_dest|" \
    "$systemd_src" > "$systemd_dest"
}

remove_if_exists() {
  local path="$1"
  if [[ -e "$path" || -L "$path" ]]; then
    rm -f "$path"
  fi
}

install_assets() {
  for required in \
    "$daemon_src" \
    "$fallback_src" \
    "$gui_src" \
    "$polkit_src" \
    "$desktop_src" \
    "$icon_svg_src" \
    "$icon_48_src" \
    "$icon_128_src" \
    "$dbus_policy_src" \
    "$dbus_service_src" \
    "$systemd_src" \
    "$qml_qmldir_src" \
    "$qml_qmltypes_src"; do
    if [[ ! -e "$required" ]]; then
      printf 'Missing required file: %s\n' "$required" >&2
      exit 1
    fi
  done

  install_file "$polkit_src" "$polkit_dest"
  install_file "$desktop_src" "$desktop_dest"
  install_file "$icon_svg_src" "$icon_svg_dest"
  install_file "$icon_48_src" "$icon_48_dest"
  install_file "$icon_128_src" "$icon_128_dest"
  install_file "$dbus_policy_src" "$dbus_policy_dest"
  install_file "$dbus_service_src" "$dbus_service_dest"
  install_file "$qml_qmldir_src" "$qml_dest_dir/qmldir"
  install_file "$qml_qmltypes_src" "$qml_dest_dir/gui_app.qmltypes"
  install_systemd_unit
  install_executable "$daemon_src" "$daemon_dest"
  install_executable "$fallback_src" "$fallback_dest"
  install_executable "$gui_src" "$gui_dest"

  systemctl daemon-reload
  update-desktop-database "$desktop_dest_dir" >/dev/null 2>&1 || true
  gtk-update-icon-cache -q -t "$icon_theme_dir" >/dev/null 2>&1 || true
  kbuildsycoca6 >/dev/null 2>&1 || true

  printf 'Installed developer integration files using %s artifacts.\n' "$profile"
  printf 'Start the daemon with: systemctl start kde-fan-control-daemon\n'
}

uninstall_assets() {
  local was_active="false"
  if systemctl is-active --quiet kde-fan-control-daemon; then
    was_active="true"
    systemctl stop kde-fan-control-daemon
  fi

  if systemctl is-enabled --quiet kde-fan-control-daemon >/dev/null 2>&1; then
    systemctl disable kde-fan-control-daemon >/dev/null 2>&1 || true
  fi

  remove_if_exists "$systemd_dest"
  remove_if_exists "$dbus_service_dest"
  remove_if_exists "$dbus_policy_dest"
  remove_if_exists "$polkit_dest"
  remove_if_exists "$desktop_dest"
  remove_if_exists "$icon_svg_dest"
  remove_if_exists "$icon_48_dest"
  remove_if_exists "$icon_128_dest"
  remove_if_exists "$daemon_dest"
  remove_if_exists "$fallback_dest"
  remove_if_exists "$gui_dest"
  rm -rf "$qml_dest_dir"

  systemctl daemon-reload
  update-desktop-database "$desktop_dest_dir" >/dev/null 2>&1 || true
  gtk-update-icon-cache -q -t "$icon_theme_dir" >/dev/null 2>&1 || true
  kbuildsycoca6 >/dev/null 2>&1 || true

  if [[ "$was_active" == "true" ]]; then
    printf 'Stopped running kde-fan-control-daemon instance before uninstall.\n'
  fi
  printf 'Removed developer integration files.\n'
}

case "$command" in
  install)
    install_assets
    ;;
  uninstall)
    uninstall_assets
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    printf 'Unknown command: %s\n' "$command" >&2
    usage
    exit 1
    ;;
esac
