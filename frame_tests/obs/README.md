# Real OBS integration tests

Run `just test-obs` on Linux with an X11 display. On a Wayland desktop, XWayland must supply `DISPLAY`.
The command builds the release plugin, stages its data, and starts the installed OBS Flatpak.
It does not install the plugin into your OBS configuration.

```sh
just test-obs
just test-obs --repeat 5
just test-obs --software-renderer
```

Prerequisites: the normal project build dependencies, the OBS Flatpak and its SDK, Node 22.18 or
newer, FFmpeg (`ffprobe`), and a working X11/OpenGL display. The software option selects Mesa's
software renderer inside Flatpak. It still needs a display.

After a successful build, repeat only the tests with:

```sh
node --experimental-strip-types frame_tests/obs/run.ts --repeat 5
```

## Coverage

The tests load the production C loader, core, Rust runtime, matcher, and replay pipeline into OBS.
A test-only Lua script creates Image and Media Sources through OBS APIs. It does not provide
capture pixels or replace any plugin functions. The runner uses the plugin's HTTP API and event
stream. It does not use OBS WebSocket control.

- A 1080p image reaches the matcher as 640×480 pixels. Removing and recreating the source at another resolution retains correct detection.
- Japanese start and stretched stats frames exercise language selection, calibration feedback, and GPU crop correction.
- The completed-run video saves Runway with a 28-second time.
- The KIA video saves a 14-second result, despite its initial incorrect time reading.
- Saved runs require a matching save-completion event, correct catalog metadata, a nonempty clip, and valid video duration from `ffprobe`.
- Every session stops monitoring and the replay buffer, then exits OBS cleanly.

## Isolation and timing

Passing scenarios share one OBS process within each repetition. Each new repetition or failure recovery starts a fresh process.
Each session uses a separate directory under
`obs2/build/real-obs-*` for OBS settings, plugin settings,
cache, temporary files, replay files, and saved clips. Flatpak replaces XDG paths during startup,
so the launcher sets them inside the sandbox before executing OBS. The runner checks the plugin's
settings path, the empty source list, and the replay output directory before sending test commands.
Before reuse, the runner checks that monitoring and the replay buffer stopped and that no sources remain.
Desktop and microphone capture are not configured, and the test denies the PulseAudio socket.

The operating system selects a separate plugin port for each session. Commands use sequence IDs
and atomic file replacement. Media sources decode their first frame and then pause before monitoring starts. Playback
restarts only after the monitor and replay buffer report readiness.

The tests wait for observed conditions with deadlines. They do not assume that a fixed startup
delay is sufficient or automatically retry failed scenarios. Media saves must finish before the
monitor stops. Playback must end before the duplicate-run check. The suite fails on unexpected
OBS exits, event-stream disconnection, timeouts, or forced shutdown.

Ctrl+C requests cleanup. A failed graceful shutdown uses the exact Flatpak instance ID for forced
cleanup. Other OBS instances are not targeted. Normal shutdown uses OBS's Linux SIGINT handler
from inside the test process, avoiding signals to Flatpak's proxy processes.

## Failure reporting and continuation

A failed scenario does not stop the suite. The runner closes that OBS session and continues with
the next scenario in a fresh process. Later repetitions also run. Steps within a failed scenario
stop immediately because later steps can depend on them.

Timeouts print the expected condition and the last observed state, including exact matcher values
when applicable. Each result preserves launch, test, and cleanup errors separately, including
nested causes. A cleanup error cannot replace the original test failure. Final shutdown errors belong to the last scenario in that session.

The final summary lists every failed scenario and counts passed, failed, and skipped cases.
The command exits with a nonzero status if any scenario fails or the user interrupts the suite.
Ctrl+C cleans up the active session and marks the remaining cases as skipped.

`just test-obs-harness` checks reporting, continuation, and interruption without OBS. These checks
also run before `just test-obs` builds and launches OBS.

## Failure artifacts

The suite prints its artifact directory. Its root `result.json` contains all scenario results and
updates after each case. Separate `case-N.json` files retain each result.
Each result links to its session directory, which can contain logs from several scenarios.
Durations include session startup or shutdown only when that case requires them. Each session directory retains:

- `obs.log`: native OBS and plugin output, including renderer details.
- `events.jsonl`: plugin snapshots and events.
- `commands.jsonl`: source-control requests and responses.
- `ge-frames-*`: actual capture frames from the production diagnostic capture path.
- `probe-*.json`, `clips/`, and `replays/`: output-file checks and media.
- `config/`: the isolated OBS and plugin configuration.

These directories are ignored by Git. `just clean` removes them with other build outputs.
The suite is separate from `just test` because it requires an OBS installation and a display.

## CI

A Linux runner with a display and compatible graphics stack can run the same command. A dedicated
runner with the OBS Flatpak and SDK preinstalled is the closest match to the local setup.

For a hosted Linux runner, the proposed setup is Xvfb, Mesa software rendering, and a D-Bus session:

```sh
dbus-run-session -- xvfb-run -a -s '-screen 0 1920x1080x24' \
  just test-obs --software-renderer --repeat 3
```

Install `xvfb`, Mesa, D-Bus, Flatpak, FFmpeg, and the usual project dependencies first. The runner
must allow Flatpak's user namespaces and sandbox setup. Provision the OBS Flatpak and matching SDK
before the test job. Pin their versions and update them deliberately. Preserve `obs2/build/real-obs-*` as CI artifacts even on failure.

The hosted-runner command is a proposal until verified on that runner. Software OpenGL covers
OBS's real rendering and capture code but cannot replace testing on physical GPU drivers. Tests
assert detection results and dimensions rather than exact pixel equality across drivers.
