# Automatic run monitoring

This capability watches GoldenEye gameplay, recognizes runs, estimates in-game time, and captures
clips through OBS's replay buffer.

Start with [session.rs](session.rs). `RunSession::process_match` feeds every raw reading to
recording, then smooths the displayed result and updates the timer. `stop` finishes pending
recording work before freezing the clocks. One worker owns this state; the session has no mutex and
receives no application-wide dependency container.

## Where to look

| Question                                                           | Owner                                                                                                   |
| ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------- |
| What makes a run start, fail, finish, or get cancelled?            | [run_detection.rs](run_detection.rs), `RunDetection::on_frame`, near the top of the file                |
| What starts, pauses, resumes, or stops the in-game timer?          | [in_game_timer.rs](in_game_timer.rs), `InGameTimer`                                                     |
| How do observations affect the current session?                    | [session.rs](session.rs), `RunSession`                                                                  |
| What happens when monitoring starts, fails to start, or stops?     | [lifecycle.rs](lifecycle.rs), `RunMonitor::start` and `RunMonitor::stop`                                |
| How are detected run decisions turned into scheduled saves?        | [recording.rs](recording.rs), `RunRecorder::on_frame`                                                   |
| How is a replay identified, trimmed, named, and cataloged?         | [clip_saving.rs](clip_saving.rs), with filename rules in [clip_output.rs](clip_output.rs)               |
| How do OBS callbacks complete a save or restart the replay buffer? | [replay_buffer.rs](replay_buffer.rs)                                                                    |
| Why did the displayed recording status disappear?                  | [recording_status.rs](recording_status.rs), including expiry and stale-completion checks                |
| How are frames acquired and interpreted?                           | [worker.rs](worker.rs), using [OBS frame capture](../obs/frame_capture.rs) and [matcher.rs](matcher.rs) |

## Reading the workflow

`lifecycle.rs` owns `RunMonitor`, its private active handle, and its diagnostic flag. Application
startup constructs it with settings, catalog, and publication dependencies. HTTP routes, OBS
callbacks, and update checks call its operations without accessing its lock or worker resources.

`RunMonitor::start` creates capture resources, the recorder, the session, and the worker. It waits
for the worker to publish the running state before acknowledging startup. `stop` owns capture
teardown, joining the worker, and catalog-session finalization. The worker receives only its own
dependencies, so it does not keep the application container alive.

The existing lock scope is preserved: startup holds the handle lock, and shutdown takes the handle
before awaiting teardown. Consequently `is_active` becomes false before shutdown finishes, and
concurrent start/stop requests are not serialized through all cleanup. This extraction does not
introduce a new lifecycle state or change that concurrency behavior.

`worker.rs` deals with OBS frames and CV. It passes matched observations to the session, followed by
black-frame and watch observations. It wakes for pending-save deadlines even when frames stop
arriving. Its dependencies are specific: the matcher, diagnostic flag, and output channel.

`session.rs` expresses what the capability does with those observations. `recording.rs` applies
run-detection decisions and executes saves. `run_detection.rs` contains the actual run transitions,
followed by voting and bookkeeping helpers. Pending saves remain independent of the active run
because one run can start while an earlier run is being saved.

`clocks.rs` combines session time and timer output into a display snapshot. The timer's GoldenEye
rules remain in `in_game_timer.rs`; detection algorithms and shared game rules remain in `ge_cv` and
`ge_game`.

HTTP routes call the capability's start/stop operations. OBS replay callbacks go directly to its
replay coordinator. Cross-thread replay completion and recording-status expiry retain their own
synchronization. Ordinary per-frame processing uses direct calls, without translating between
internal event enums.

## Review checkpoint

Compared with `fae15db`, this reorganization removes `MonitorEvent` and `RecordingEvent`, groups the
capability into one directory, moves the run transition function from line 313 to near the top of
`run_detection.rs`, and removes `AppState` from the frame worker and lifecycle owner. It preserves
the single worker owner, startup acknowledgement, shutdown flush ordering, and replay permit.

The ordinary call path still has meaningful stages: worker → session → recorder → run detection. The
improvement is a visible workflow and explicit responsibilities, not a claim that every
investigation now requires only one file. The event dispatch stages have become direct calls with
ordinary argument types.

Publication now lives in `app`, with monitor payloads and publication rules in
[publication.rs](publication.rs). The updater receives its recording-status reader during
construction. Feature workflows have no `AppState` or HTTP dependency.

`RunMonitor` owns the replay-stop latch, typed stop reasons, and stopped notifications as well as
capture resources. The lifecycle remains expressed through explicit operations and the active
handle; this work does not redesign it or run detection into new state enums.

See the [whole-codebase ownership review](../OWNERSHIP_REVIEW.md) for current size comparisons,
validation, and the behavior changes introduced alongside the moves.
