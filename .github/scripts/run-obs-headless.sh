#!/usr/bin/env bash
set -euo pipefail

desktop_log="$1"
shift
mkdir -p "$(dirname "$desktop_log")"

show_desktop_log() {
  status=$?
  echo "::group::Desktop service diagnostics ($desktop_log)"
  cat "$desktop_log"
  echo "::endgroup::"
  exit "$status"
}
trap show_desktop_log EXIT

# Keep service stderr in its own log while test stderr stays visible in Actions.
xvfb-run -a -s '-screen 0 1920x1080x24' dbus-run-session -- \
  bash -c 'exec "$@" 2>&3' bash "$@" 3>&2 2>"$desktop_log"
