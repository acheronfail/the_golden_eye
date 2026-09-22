# Real OBS integration tests

Run `just test-obs` on macOS in a logged-in graphical session or on Linux with an X11 display.
On a Wayland desktop, XWayland must supply `DISPLAY`.
The command builds the release plugin and starts `/Applications/OBS.app` on macOS or the installed
OBS Flatpak on Linux.
It does not install the plugin into your OBS configuration.

```sh
just test-obs
just test-obs --repeat 5
just test-obs --software-renderer
```

Prerequisites: the normal project build dependencies, Node 22.18 or newer, Python 3, a C compiler (`cc`), and FFmpeg (`ffprobe`).
macOS needs OBS installed at `/Applications/OBS.app` and a graphical login; this harness has been
verified with OBS 32.2.2 on Apple Silicon. Linux needs the OBS Flatpak and its SDK plus a working
X11/OpenGL display. The software option is Linux-only and selects Mesa's software renderer inside
Flatpak. It still needs a display.

After a successful build, repeat only the tests with:

```sh
node --experimental-strip-types frame_tests/obs/run.ts --repeat 5
```

## Coverage

The tests load the production C loader, core, Rust runtime, matcher, and replay pipeline into OBS.
A test-only Lua script creates Image and Media Sources through OBS APIs. It does not provide
capture pixels. Update tests replace only the isolated core and module data. The runner uses the plugin's HTTP API and event
stream. It does not use OBS WebSocket control.

- A 1080p image reaches the matcher as 640×480 pixels. Removing and recreating the source at another resolution retains correct detection.
- Japanese start and stretched stats frames exercise language selection, calibration feedback, and GPU crop correction.
- The completed-run video saves Runway with a 28-second time.
- The KIA video saves a 14-second result, despite its initial incorrect time reading.
- Saved runs require a matching save-completion event, correct catalog metadata, a nonempty clip, and valid video duration from `ffprobe`.
- Every session stops monitoring and the replay buffer, then exits OBS cleanly.

## Updates inside OBS

The suite reuses the HTTP handler from `just simulate-update` on an automatically assigned loopback port.
Each session copies the plugin and module data into its artifact directory before OBS starts.
Updates replace these copies. Build outputs and installed plugins remain unchanged.

The default suite exercises the production updater:

- A wrong checksum rejects the download before staging. The current core stays unchanged and matching still works.
- With automatic updates enabled, a check downloads and stages the package during monitoring. Explicit application returns HTTP 409 until monitoring stops.
  The production background task then applies the update without an apply request. The test checks the new module-data marker and retained settings and runs.

After a successful reload, the runner requires a new event connection and fresh snapshot.
It checks matching again. The completed-run and KIA cases then exercise real replay saves after the update.
Unexpected disconnects outside the reload window still fail. A reload has a 75-second deadline because the production task polls every 30 seconds.
The release server closes with the OBS session, including after failures or interruption.

The successful package contains the current production core, advertised as version `999.0.0`, plus a module-data marker.
This tests download, verification, staging, native reload, data replacement, and continued operation.
It also checks the `updateApplied` notification. Use the dedicated upgrade suite below to test a compiled version change.

### Version A to B

Run `just test-obs-upgrade` for a real version transition. On Linux, add `--software-renderer` to use Mesa software rendering.
The command builds the same current source as `998.0.0` and `998.0.1` through `just make-package`.
Both builds use the normal release profile and the browser-to-Rust-to-C build chain.
The runner saves each package before the next build starts. It never installs either package into your personal OBS configuration.

The test starts OBS with A and requires A's reported version. It saves a run, enables automatic updates, and serves B on the second page of the local release history.
The first page advertises a newer release with an incompatible updater number. The plugin must select B.
After reload, it requires B's version notice, release link, and installed core checksum. The resident loader must stay unchanged.
Settings and existing runs must survive, and a new replay save must succeed.
The test then closes OBS and restarts it with the same installation and configuration.
It requires B's checksum and browser build ID, a manual-install offer for the incompatible release, the saved settings and runs, and working frame detection.
The incompatible package must never be downloaded.

Builds and their checksums, source fingerprint, and timings remain under `obs2/build/obs-upgrade-builds-*`.
A source change during the two builds fails preparation. Test logs and reports remain under `obs2/build/real-obs-upgrade-*`.
Reuse a saved pair without recompilation:

```sh
just test-obs-upgrade --reuse-build obs2/build/obs-upgrade-builds-EXAMPLE
```

Reuse requires matching source files, platform, architecture, and package checksums. It extracts fresh test inputs from the saved packages.
Rebuild after compiler or dependency changes. `--build-only` prepares a pair without launching OBS.
Normal build caches reuse unchanged dependencies. This dedicated suite remains separate from `just test` and `just test-obs`.
No CI workflow runs it yet. The version transition covers current code with two versions, not compatibility with older implementations or schemas.

### Rollback recovery

The default suite includes rollback recovery. A fixture passes the loader's symbol checks but rejects core startup.
The test requires the original core and templates, preserved settings and runs, and a fresh source snapshot.
It then matches a frame, saves a replay, and stops monitoring and the replay buffer.
No successful-update notice may appear during recovery. The next case applies a valid update in the same OBS session.

The fixture fails before Rust starts. Smaller loader and runtime tests cover replacement failure, provisional startup, and commit-gated notices.
The u2 loader contract fixes this regression and requires a full manual installation for existing u1 users.
See [the migration guide](../../docs/dev/auto-update.md) for details.
The recovery cases have been verified on macOS 15.6 (Apple Silicon) with OBS 32.2.2.

## Isolation and timing

Passing scenarios share one OBS process within each repetition. Each new repetition or failure recovery starts a fresh process.
Each session uses a separate directory under
`obs2/build/real-obs-*` for OBS settings, plugin settings,
cache, temporary files, replay files, and saved clips. Flatpak replaces XDG paths during startup,
so the launcher sets them inside the sandbox before executing OBS. The runner checks the plugin's
settings path, the empty source list, and the replay output directory before sending test commands.
Before reuse, the runner checks that monitoring and the replay buffer stopped and that no sources remain.
Desktop and microphone capture are not configured. Linux denies the PulseAudio socket.
On macOS, the child process gets a separate `HOME` and `CFFIXED_USER_HOME`: the plugin uses the
former and OBS's Foundation paths use the latter. Settings live under that home's
`Library/Application Support`. The test profile skips OBS's optional first-run permissions dialog;
no screen, camera, microphone, or input-monitoring permissions are required or granted.
The launcher clears `GE_CORE_LIB` so a development override cannot replace the built core.

The operating system selects a separate plugin port for each session. Commands use sequence IDs
and atomic file replacement. Media sources decode their first frame and then pause before monitoring starts. Playback
restarts only after the monitor and replay buffer report readiness.

The tests wait for observed conditions with deadlines. They do not assume that a fixed startup
delay is sufficient or automatically retry failed scenarios. Media saves must finish before the
monitor stops. Playback must end before the duplicate-run check. The suite fails on unexpected
OBS exits, unexpected event-stream disconnection, timeouts, or forced shutdown.

Ctrl+C requests cleanup. A failed graceful shutdown uses the exact Flatpak instance ID for forced
cleanup. Other OBS instances are not targeted. Normal shutdown uses OBS's SIGINT handler from inside the test process on both platforms,
avoiding signals to Flatpak's proxy processes. Forced cleanup on macOS targets only the detached
test process group. Forced shutdown still fails the scenario.

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
- `release-server.log`, `release/`: local release requests, packages, and the failed core fixture.
- `plugins/`, `plugins-data/` (Linux): disposable plugin binaries and module data.
- `events.jsonl`: plugin snapshots and events.
- `commands.jsonl`: source-control requests and responses.
- `ge-frames-*`: actual capture frames from the production diagnostic capture path.
- `probe-*.json`, `clips/`, and `replays/`: output-file checks and media.
- `config/` (Linux) or `home/Library/Application Support/` (macOS): isolated OBS and plugin configuration.

These directories are ignored by Git. `just clean` removes them with other build outputs.
The suite is separate from `just test` because it requires an OBS installation and a display.

## CI

A Linux runner with a display and compatible graphics stack can run the same command. A dedicated
runner with the OBS Flatpak and SDK preinstalled is the closest match to the local setup.

The Linux GitHub Actions job uses Xvfb, Mesa software rendering, and a D-Bus session:

```sh
xvfb-run -a -s '-screen 0 1920x1080x24' dbus-run-session -- \
  just test-obs --software-renderer --repeat 3
```

CI separates harness checks and build preparation from execution so test steps show scenario
results without compiler output. It builds upgrade fixtures with `just test-obs-upgrade --build-only`
and runs `upgrade.ts` under the same display wrapper after uploading the normal plugin package,
so synthetic upgrade versions cannot replace that artifact.
The real OBS suites run only in the Linux job; macOS and Windows keep their existing test suites.

Install `xvfb`, `xauth`, Mesa, D-Bus, Flatpak, FFmpeg, and the usual project dependencies first. The runner
must allow Flatpak's user namespaces and sandbox setup. Provision the OBS Flatpak and matching SDK
before the test job. CI creates a private, user-owned `XDG_RUNTIME_DIR` and starts Xvfb before
D-Bus so activated services inherit the display.
Desktop-service stderr appears in a collapsed Actions group and a retained log; test stderr
remains visible alongside scenario results, and the wrapper preserves the test exit status.

CI uploads reports, logs, event traces, captured frames, clips, isolated configuration, and the
upgrade build manifest even on failure, with seven-day retention. It excludes disposable plugin
copies, template copies, caches, and update archives. Build preparation has a 30-minute limit;
the repeated suite and upgrade execution have 15-minute and 5-minute limits respectively.

Software OpenGL covers
OBS's real rendering and capture code but cannot replace testing on physical GPU drivers. Tests
assert detection results and dimensions rather than exact pixel equality across drivers.
