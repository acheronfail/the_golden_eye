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

# Keep both service streams in the log; preserve the test streams on separate fds.
xvfb-run -a -s '-screen 0 1920x1080x24' dbus-run-session -- \
  bash -c 'exec "$@" >&8 2>&9' bash "$@" 8>&1 9>&2 >"$desktop_log" 2>&1
