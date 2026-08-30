#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct RunIdentity {
    mission: i32,
    part: i32,
    difficulty: i32,
}

impl RunIdentity {
    fn from_match(m: &LevelMatch) -> Option<Self> {
        ge::level_info(m.mission, m.part)?;
        ge::difficulty_name(m.difficulty)?;
        Some(Self { mission: m.mission, part: m.part, difficulty: m.difficulty })
    }

    fn apply_to(self, m: &mut LevelMatch) {
        m.mission = self.mission;
        m.part = self.part;
        m.difficulty = self.difficulty;
        if !m.raw_times.is_empty() {
            m.times = ge::Times::classify(self.mission, self.part, self.difficulty, &m.raw_times);
        }
    }

    fn immediately_precedes(self, next: Self) -> bool {
        let Some(current) = ge::level_info(self.mission, self.part) else {
            return false;
        };
        let Some(next) = ge::level_info(next.mission, next.part) else {
            return false;
        };
        current.number.checked_add(1) == Some(next.number)
    }
}

#[derive(Default)]
struct RunIdentityVote {
    counts: HashMap<RunIdentity, u32>,
    best_count: u32,
    winner: Option<RunIdentity>,
}

impl RunIdentityVote {
    fn record(&mut self, identity: RunIdentity) {
        let count = {
            let count = self.counts.entry(identity).or_insert(0);
            *count += 1;
            *count
        };
        if count > self.best_count {
            self.best_count = count;
            self.winner = Some(identity);
        }
    }
}

/// A scheduled save that *will* happen, captured in full when the stats screen is
/// seen. Decoupled from the active-run state: once scheduled it owns all it needs,
/// so backing out or starting another run can't drop it -- it fires on its own timer.
struct PendingSave {
    /// Core-lifetime unique id for retained replay pipeline diagnostics.
    tracking_id: u64,
    /// Identifier shared by the pending and saved WebSocket events.
    save_id: u64,
    /// When the post-run padding window elapses and we save the buffer.
    fire_at: Instant,
    /// When the run began -- the anchor for where the trimmed clip starts.
    clip_start: Instant,
    /// When the run ending was detected -- the anchor for post-run padding.
    finish_at: Instant,
    /// The final report status seen for the run (for naming/logging).
    status: RunStatus,
    /// Wall-clock time when the run ending was detected.
    completed_at: SystemTime,
    /// Game/template language active when this save was scheduled.
    game_language: String,
    /// The stats-screen match, kept for naming the output clip. Its `times` are
    /// overwritten with the per-field vote winners as stats frames arrive.
    stats: Option<LevelMatch>,
    /// Independent per-field vote over the stats times, so a look-alike-digit
    /// misread on one field (often the dimmer best-time row) can't corrupt the
    /// others. Empty for saves not scheduled off the stats screen.
    time_vote: FieldVote,
    target_vote: FieldVote,
    best_vote: FieldVote,
    /// Set once the screen leaves stats: the vote is locked so a later run's stats
    /// screen (within the padding window) can't fold into this save.
    stats_vote_closed: bool,
    /// Whether the provisional recent-run event has been sent for this save.
    /// It is refreshed only when the voted time changes.
    pending_event_sent: bool,
    /// The phase-store generation of this save's own `SavePending`/`StatsSkipped`
    /// transition, if it emitted one. Its completion/discard clears exactly that
    /// transition, not a quick-restarted run's identical-looking phase.
    phase_generation: Option<u64>,
}

/// Frame-count vote for one stats-time field. The most-seen value wins, ties
/// resolving to the newest reading, so a brief first-frame misread is outvoted
/// by the stable one.
#[derive(Default)]
struct FieldVote {
    counts: HashMap<Option<i32>, u32>,
    best_count: u32,
    winner: Option<i32>,
}

impl FieldVote {
    /// Records one reading; returns whether the winning value changed.
    fn record(&mut self, value: Option<i32>) -> bool {
        let count = {
            let c = self.counts.entry(value).or_insert(0);
            *c += 1;
            *c
        };
        if count < self.best_count {
            return false;
        }
        let changed = self.winner != value;
        self.best_count = count;
        self.winner = value;
        changed
    }
}

/// Record one stats reading, voting each time field independently, and refresh
/// the stored match with the per-field winners. Returns whether any voted field
/// changed (so the provisional recent-run row can be refreshed).
fn record_stats_vote(pending: &mut PendingSave, m: &LevelMatch) -> bool {
    let times = m.times;
    let mut changed = pending.time_vote.record(times.map(|t| t.time));
    changed |= pending.target_vote.record(times.and_then(|t| t.target_time));
    changed |= pending.best_vote.record(times.and_then(|t| t.best_time));
    // Identity and diagnostics stay anchored to the run's canonical match; only
    // the independently voted time fields are refined by later stats frames.
    if let Some(stats) = pending.stats.as_mut() {
        stats.times = pending.time_vote.winner.map(|time| crate::ge::Times {
            time,
            target_time: pending.target_vote.winner,
            best_time: pending.best_vote.winner,
        });
    }
    changed
}

fn run_status_from_failure_screen(screen: Screen) -> Option<RunStatus> {
    match screen {
        Screen::Failed => Some(RunStatus::Failed),
        Screen::Abort => Some(RunStatus::Abort),
        Screen::Kia => Some(RunStatus::Kia),
        Screen::Unknown
        | Screen::Start
        | Screen::Stats
        | Screen::Complete
        | Screen::Opts007
        | Screen::Select
        | Screen::Levels => None,
    }
}

fn recording_save_pending_event(
    save_id: u64,
    save_delay: Duration,
    estimated_duration_secs: f64,
    status: RunStatus,
    stats: Option<&LevelMatch>,
) -> RecordingSavePending {
    let level_info = stats.and_then(|m| ge::level_info(m.mission, m.part));
    let times = stats.and_then(|m| m.times);

    RecordingSavePending {
        save_id,
        save_in_secs: save_delay.as_secs_f64(),
        estimated_duration_secs,
        failed: status.is_failed(),
        status: status.as_str().to_owned(),
        level: level_info.map(|info| info.name.to_owned()).unwrap_or_else(|| "unknown".to_owned()),
        level_number: level_info.map(|info| info.number),
        difficulty: stats.and_then(|m| ge::difficulty_name(m.difficulty)).map(str::to_owned),
        time_secs: times.map(|t| t.time),
        target_time_secs: times.and_then(|t| t.target_time),
        best_time_secs: times.and_then(|t| t.best_time),
        stats: stats.cloned(),
    }
}

/// Build the provisional run event, reading `save_in_secs` as the time remaining
/// until it fires. Re-sent when the voted time is refined.
fn save_pending_event(pending: &PendingSave, options: &RecordingOptions, now: Instant) -> RecordingSavePending {
    let run_length_secs = pending.finish_at.saturating_duration_since(pending.clip_start).as_secs_f64();
    let estimated_duration_secs = run_length_secs + options.pre_run_padding_secs() + options.post_run_padding_secs();
    recording_save_pending_event(
        pending.save_id,
        pending.fire_at.saturating_duration_since(now),
        estimated_duration_secs,
        pending.status,
        pending.stats.as_ref(),
    )
}

/// Metadata shared by runs finalized during one monitoring session.
pub struct RecordingSessionContext {
    source_name: String,
    game_language: String,
    monitor_session_id: Option<String>,
}

impl RecordingSessionContext {
    pub fn new(source_name: String, game_language: String, monitor_session_id: Option<String>) -> Self {
        Self { source_name, game_language, monitor_session_id }
    }
}

/// Tracks one recording session as it moves through the on-screen states, and
/// drives replay-buffer saves when runs finish. Fed via [`RecordingState::on_frame`].
pub struct RecordingState {
    /// When the currently-active run began, or `None` when no run is in
    /// progress. A scheduled save lives in `pending` instead, so it survives the
    /// active run ending.
    clip_start: Option<Instant>,
    /// The final report status seen during the active run. Tracked for
    /// naming/logging; the clip is saved either way.
    status: Option<RunStatus>,
    /// The post-mission report screen (Complete/Failed/Abort/KIA) match, or `None`
    /// if not reached. Presence means the run finished (so backing out still saves);
    /// absence means abandoned. Also names the clip when the stats screen is skipped.
    report: Option<LevelMatch>,
    /// Majority identity observed on the active run's start/007-options screen.
    /// This is the canonical level signal used to validate later report/stats frames.
    identity_vote: RunIdentityVote,
    /// A scheduled save in flight, if any. Independent of the active run: once
    /// set it is always saved when its timer elapses, even if the user backs out
    /// or starts another run in the meantime.
    pending: Option<PendingSave>,
    /// Monotonic id linking the provisional recent-run row to its finalized run.
    next_save_id: u64,
    /// Broadcasts an [`AppEvent::RecordingSaved`] to event-stream clients once a
    /// clip is written. Cloned into each save thread.
    event_tx: broadcast::Sender<AppEvent>,
    /// Retained recorder phase reported in app snapshots.
    recording_state: RecordingStateStore,
    /// Retained replay-buffer save pipeline diagnostics.
    replay_saves: ReplaySaveStateStore,
    /// Recording/output options fixed for this monitor session.
    options: RecordingOptions,
    /// The retention count is the one recording option that can change while a
    /// monitor is running.
    recent_run_limit: Arc<AtomicUsize>,
    /// OBS source this monitor session records from, stored in clip metadata.
    source_name: String,
    /// Game/template language this monitor session matches, stored in clip metadata.
    game_language: String,
    /// Index of saved run clips, updated after successful trims.
    run_catalog: Arc<RunCatalog>,
    /// Durable monitoring session associated with finalized runs from this worker.
    monitor_session_id: Option<String>,
}

impl RecordingState {
    pub fn new(
        event_tx: broadcast::Sender<AppEvent>,
        recording_state: RecordingStateStore,
        replay_saves: ReplaySaveStateStore,
        options: RecordingOptions,
        session: RecordingSessionContext,
        run_catalog: Arc<RunCatalog>,
    ) -> Self {
        let recent_run_limit = Arc::new(AtomicUsize::new(options.recent_run_limit.clamp(1, MAX_RECENT_RUN_LIMIT)));
        RecordingState {
            clip_start: None,
            status: None,
            report: None,
            identity_vote: RunIdentityVote::default(),
            pending: None,
            next_save_id: 1,
            event_tx,
            recording_state,
            replay_saves,
            options,
            recent_run_limit,
            source_name: session.source_name,
            game_language: session.game_language,
            run_catalog,
            monitor_session_id: session.monitor_session_id,
        }
    }

    pub fn set_recent_run_limit_source(&mut self, source: Arc<AtomicUsize>) {
        self.recent_run_limit = source;
    }

    /// Publish a recorder state transition to the backend-retained phase store
    /// Event-stream clients see it in the next app snapshot.
    /// For `SavePending`/`StatsSkipped`, records the generation on the pending
    /// save so its completion/discard can clear that exact transition later.
    fn emit(&mut self, status: RecordingStatus) {
        let generation = self.recording_state.set(status);
        if matches!(status, RecordingStatus::SavePending | RecordingStatus::StatsSkipped)
            && let Some(pending) = self.pending.as_mut()
        {
            pending.phase_generation = Some(generation);
        }
    }

    /// Update the game/template language attached to future clip metadata. Used
    /// when monitor language auto-correction detects the other game language.
    pub fn set_game_language(&mut self, game_language: String) {
        if self.game_language != game_language {
            tracing::info!(from = %self.game_language, to = %game_language, "recording game language changed");
        }
        self.game_language = game_language;
    }

    fn canonicalize_match(&self, mut m: LevelMatch) -> LevelMatch {
        if let Some(identity) = self.identity_vote.winner {
            let observed = RunIdentity::from_match(&m);
            if observed != Some(identity) {
                tracing::info!(?identity, ?observed, "using start-screen identity for completed run");
                identity.apply_to(&mut m);
            }
        }
        m
    }

    /// Schedule the replay-buffer save for a finished run, ending report tracking.
    /// `stats` names the clip (stats-screen match, or report-screen when skipped).
    /// Any earlier pending save is flushed first so it isn't dropped.
    fn schedule_save(&mut self, now: Instant, clip_start: Instant, stats: Option<LevelMatch>) -> bool {
        self.flush_pending(now);
        let stats = stats.map(|m| self.canonicalize_match(m));
        let status = self.status.unwrap_or(RunStatus::Complete);
        let save_delay = self.options.save_delay();
        let save_id = self.next_save_id;
        self.next_save_id = self.next_save_id.saturating_add(1).max(1);
        let pending = PendingSave {
            tracking_id: next_replay_tracking_id(),
            save_id,
            fire_at: now + save_delay,
            clip_start,
            finish_at: now,
            status,
            completed_at: SystemTime::now(),
            game_language: self.game_language.clone(),
            stats,
            time_vote: FieldVote::default(),
            target_vote: FieldVote::default(),
            best_vote: FieldVote::default(),
            stats_vote_closed: false,
            pending_event_sent: false,
            phase_generation: None,
        };
        self.pending = Some(pending);
        self.sync_pending_event(now, true);
        self.status = None;
        self.report = None;
        tracing::info!(?save_delay, "recording save scheduled");
        true
    }

    /// Show the provisional row once and refresh it when the voted time changes.
    fn sync_pending_event(&mut self, now: Instant, time_changed: bool) {
        let Some(pending) = self.pending.as_ref() else {
            return;
        };
        if !pending.pending_event_sent || time_changed {
            let event = save_pending_event(pending, &self.options, now);
            self.replay_saves.schedule(ReplaySaveStatus {
                tracking_id: pending.tracking_id,
                save_id: event.save_id,
                stage: ReplaySaveStage::Scheduled,
                level: event.level.clone(),
                difficulty: event.difficulty.clone(),
                run_status: event.status.clone(),
                estimated_duration_secs: event.estimated_duration_secs,
                error: None,
            });
            let _ = self.event_tx.send(AppEvent::RecordingSavePending(event));
            self.pending.as_mut().unwrap().pending_event_sent = true;
        }
    }

    /// Build a save+trim job for the pending clip, if any, anchored to `now` as
    /// the save moment (the saved file ends at ~now, so the run is its final
    /// `elapsed` seconds). A no-op when nothing is pending.
    fn take_pending_job(&mut self, now: Instant) -> Option<SaveAndTrimJob> {
        let pending = self.pending.take()?;
        self.replay_saves.transition(pending.tracking_id, ReplaySaveStage::WaitingForReplaySave);

        let metadata = clip_metadata(
            pending.status,
            pending.completed_at,
            pending.stats.as_ref(),
            &self.source_name,
            &pending.game_language,
        );
        let (finalized, tracked) = match self.run_catalog.create_finalized_run_in_session(
            pending.completed_at,
            metadata.clone(),
            self.monitor_session_id.as_deref(),
        ) {
            Ok(run) => (run, true),
            Err(err) => {
                tracing::warn!(
                    "failed to record finalized run before saving clip; continuing with tagged clip: {err:#}"
                );
                (RunCatalog::untracked_finalized_run(pending.completed_at, metadata), false)
            }
        };
        if tracked {
            let _ = self.event_tx.send(AppEvent::RunCatalogChanged {
                run_id: Some(finalized.run_id.clone()),
                save_id: Some(pending.save_id),
            });
        }

        let start_before_save_secs =
            now.saturating_duration_since(pending.clip_start).as_secs_f64() + self.options.pre_run_padding_secs();
        let finish_before_save_secs = now.saturating_duration_since(pending.finish_at).as_secs_f64();
        let trim_tail_secs = (finish_before_save_secs - self.options.post_run_padding_secs()).max(0.0);
        Some(SaveAndTrimJob {
            tracking_id: pending.tracking_id,
            save_id: pending.save_id,
            start_before_save_secs,
            trim_tail_secs,
            status: pending.status,
            completed_at: pending.completed_at,
            stats: pending.stats,
            metadata: finalized.metadata,
            options: self.options.clone(),
            recent_run_limit: self.recent_run_limit.clone(),
            event_tx: self.event_tx.clone(),
            recording_state: self.recording_state.clone(),
            replay_saves: self.replay_saves.clone(),
            run_catalog: self.run_catalog.clone(),
            phase_generation: pending.phase_generation,
        })
    }

    /// Save and trim the pending clip asynchronously, if any.
    fn flush_pending(&mut self, now: Instant) {
        if let Some(job) = self.take_pending_job(now) {
            spawn_save_and_trim(job);
        }
    }

    /// When the in-flight save is due to fire, or `None` when nothing is pending.
    /// The monitor loop waits on this so the save fires on time even if captured
    /// frames stop arriving (e.g. a paused source).
    pub fn pending_fire_at(&self) -> Option<Instant> {
        self.pending.as_ref().map(|pending| pending.fire_at)
    }

    /// Fire the scheduled save once its post-run padding window has elapsed. Safe
    /// to call on any tick (frame or idle wakeup); a no-op until then.
    pub fn poll_pending(&mut self, now: Instant) {
        if self.pending.as_ref().is_some_and(|pending| now >= pending.fire_at) {
            self.flush_pending(now);
        }
    }

    /// Fold another stats reading into the in-flight save and reconcile the pending
    /// row when its displayed time changes. No-op for closed votes or non-stats saves.
    fn refine_stats_vote(&mut self, now: Instant, m: &LevelMatch) {
        let time_changed = {
            let Some(pending) = self.pending.as_mut() else {
                return;
            };
            if pending.time_vote.counts.is_empty() || pending.stats_vote_closed {
                return;
            }
            let expected = pending.stats.as_ref().and_then(RunIdentity::from_match);
            let incoming = RunIdentity::from_match(m);
            if let Some(expected) = expected {
                let Some(incoming) = incoming else {
                    return;
                };
                if expected.immediately_precedes(incoming) {
                    tracing::info!(
                        from_mission = expected.mission,
                        from_part = expected.part,
                        to_mission = incoming.mission,
                        to_part = incoming.part,
                        "next level header appeared before stats screen cleared; closing stats vote"
                    );
                    pending.stats_vote_closed = true;
                    return;
                }
                if incoming != expected {
                    tracing::debug!(?expected, ?incoming, "ignoring mismatched stats identity");
                    return;
                }
            }
            record_stats_vote(pending, m)
        };
        self.sync_pending_event(now, time_changed);
    }

    /// Save and trim the pending clip synchronously during shutdown, preserving
    /// the scheduled post-run padding window before OBS is asked to save.
    #[cfg(not(test))]
    fn flush_pending_on_shutdown(&mut self) {
        self.flush_pending_on_shutdown_with(Instant::now(), std::thread::sleep, save_and_trim);
    }

    fn flush_pending_on_shutdown_with(
        &mut self,
        now: Instant,
        sleep: impl FnOnce(Duration),
        save: impl FnOnce(SaveAndTrimJob),
    ) {
        let Some(fire_at) = self.pending.as_ref().map(|pending| pending.fire_at) else {
            return;
        };

        let save_at = if fire_at > now {
            sleep(fire_at.duration_since(now));
            fire_at
        } else {
            now
        };

        if let Some(job) = self.take_pending_job(save_at) {
            save(job);
        }
    }

    /// Feed the latest matched frame (and the current time). Called once per
    /// captured frame, so it also polls the pending-save timer.
    pub fn on_frame(&mut self, now: Instant, m: &LevelMatch) {
        match m.screen {
            // A run begins at the level-start briefing or the 007-options screen.
            // A pending save from a previous run is left alone -- it fires on its
            // own timer -- so a new run can start without disturbing it.
            Screen::Start | Screen::Opts007 => {
                if self.clip_start.is_none() {
                    self.clip_start = Some(now);
                    self.status = None;
                    self.report = None;
                    self.identity_vote = RunIdentityVote::default();
                    ensure_replay_buffer_running_for_recording();
                    tracing::info!("recording session started");
                    self.emit(RecordingStatus::Started);
                }
                if let Some(identity) = RunIdentity::from_match(m) {
                    self.identity_vote.record(identity);
                }
            }
            // Returning to the mission grid. Meaning depends on whether the run
            // reached its report screen. A pending save from an earlier run is
            // untouched either way -- it fires on its own timer below.
            Screen::Levels => {
                if let Some(start) = self.clip_start.take() {
                    if let Some(report) = self.report.take() {
                        // Report shown, then user pressed B to the grid, bypassing stats.
                        // Run still finished, so save on the same padding timer, named from
                        // the report. Capture `status` first: `schedule_save` clears it.
                        let status = self.status.unwrap_or(RunStatus::Complete);
                        tracing::info!("stats screen skipped (report -> level select)");
                        if self.schedule_save(now, start, Some(report)) {
                            // Backing out to the grid is the *normal* ending for a failed
                            // run, so don't flag "skipped stats". Only a completed run whose
                            // stats screen was bypassed counts as skipped.
                            self.emit(if status.is_failed() {
                                RecordingStatus::SavePending
                            } else {
                                RecordingStatus::StatsSkipped
                            });
                        }
                    } else {
                        // No report screen was seen: the run was abandoned mid-play,
                        // so there's nothing worth saving.
                        self.status = None;
                        self.identity_vote = RunIdentityVote::default();
                        tracing::info!("recording session abandoned (returned to level select)");
                        self.emit(RecordingStatus::Cancelled);
                    }
                }
            }
            // Failure report screens flag the active run and mark it reached its
            // report screen. Emit only on the first failure frame (the screen lingers)
            // so clients see one transition; the screen picks the status/why it ended.
            Screen::Failed | Screen::Abort | Screen::Kia => {
                if self.clip_start.is_some() {
                    let report = self.canonicalize_match(m.clone());
                    self.report.get_or_insert(report);
                    if !self.status.is_some_and(RunStatus::is_failed) {
                        self.status = run_status_from_failure_screen(m.screen);
                        self.emit(match m.screen {
                            Screen::Failed => RecordingStatus::Failed,
                            Screen::Abort => RecordingStatus::Aborted,
                            Screen::Kia => RecordingStatus::Kia,
                            _ => unreachable!("failure-screen branch received {:?}", m.screen),
                        });
                    }
                }
            }
            // The mission-complete report screen: also marks the run as reaching its
            // report screen. Emit `Complete` once -- first clean report frame, or when
            // it clears an earlier failure flag. Later lingering frames don't re-emit.
            Screen::Complete => {
                if self.clip_start.is_some() {
                    let first_report = self.report.is_none();
                    let report = self.canonicalize_match(m.clone());
                    self.report.get_or_insert(report);
                    if first_report || self.status.is_some_and(RunStatus::is_failed) {
                        self.status = Some(RunStatus::Complete);
                        self.emit(RecordingStatus::Complete);
                    }
                }
            }
            // The stats screen ends the run: hand it to a pending save scheduled a
            // few seconds out (so the clip captures the overlay). Taking `clip_start`
            // ends the run; later stats frames refine the time but don't re-schedule.
            Screen::Stats => {
                if let Some(start) = self.clip_start.take() {
                    tracing::info!("stats detected");
                    if self.schedule_save(now, start, Some(m.clone())) {
                        // Seed the vote with this first reading; later stats frames
                        // refine `stats` toward the most-seen time.
                        if let Some(pending) = self.pending.as_mut() {
                            let initial = pending.stats.clone().expect("stats save retains its match");
                            record_stats_vote(pending, &initial);
                        }
                        self.emit(RecordingStatus::SavePending);
                    }
                } else {
                    // Still on the stats screen with the save in flight: keep voting
                    // the whole window so a multi-frame first misread is outvoted by
                    // the stable reading, updating the provisional row when it changes.
                    self.refine_stats_vote(now, m);
                }
            }
            Screen::Select => {
                // Leaving a launch screen for difficulty selection abandons the
                // provisional run; a later launch must get a fresh anchor/identity.
                if self.report.is_none() && self.clip_start.take().is_some() {
                    self.status = None;
                    self.identity_vote = RunIdentityVote::default();
                    tracing::info!("recording session cancelled (returned to difficulty selection)");
                    self.emit(RecordingStatus::Cancelled);
                }
            }
            Screen::Unknown => {}
        }

        // Leaving the stats screen locks the vote: any later run's stats screen
        // within the padding window must not fold into this save.
        if m.screen != Screen::Stats
            && let Some(pending) = self.pending.as_mut()
        {
            pending.stats_vote_closed = true;
        }

        // Fire the scheduled save once its post-run padding window elapses,
        // regardless of the current screen, so a pending save completes even after
        // the user backs out or starts another run.
        self.poll_pending(now);
    }
}

#[cfg(not(test))]
impl Drop for RecordingState {
    fn drop(&mut self) {
        self.flush_pending_on_shutdown();
    }
}

#[cfg(test)]
impl Drop for RecordingState {
    fn drop(&mut self) {
        assert!(self.pending.is_none(), "test dropped RecordingState with a pending save");
    }
}

