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
fn save_pending_event(pending: &PendingSave, policy: RunTrackerPolicy, now: Instant) -> RecordingSavePending {
    let run_length_secs = pending.finish_at.saturating_duration_since(pending.clip_start).as_secs_f64();
    let estimated_duration_secs = run_length_secs + policy.pre_run_padding_secs + policy.post_run_padding_secs;
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

#[derive(Default)]
struct TrackerUpdate {
    ensure_replay_buffer: bool,
    pending_changed: bool,
    phase: Option<RecordingStatus>,
    ready: Vec<PendingSave>,
}

/// Pure run-detection state. It translates matched screens into domain
/// transitions; [`RecordingState`] applies OBS, catalog, and UI side effects.
struct RunTracker {
    clip_start: Option<Instant>,
    status: Option<RunStatus>,
    report: Option<LevelMatch>,
    identity_vote: RunIdentityVote,
    pending: Option<PendingSave>,
    next_save_id: u64,
    game_language: String,
}

impl RunTracker {
    fn new(game_language: String) -> Self {
        Self {
            clip_start: None,
            status: None,
            report: None,
            identity_vote: RunIdentityVote::default(),
            pending: None,
            next_save_id: 1,
            game_language,
        }
    }

    fn set_game_language(&mut self, game_language: String) {
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

    fn schedule_save(
        &mut self,
        now: Instant,
        completed_at: SystemTime,
        clip_start: Instant,
        stats: Option<LevelMatch>,
        policy: RunTrackerPolicy,
        update: &mut TrackerUpdate,
    ) {
        if let Some(previous) = self.pending.take() {
            update.ready.push(previous);
        }
        let stats = stats.map(|m| self.canonicalize_match(m));
        let status = self.status.unwrap_or(RunStatus::Complete);
        let save_delay = policy.save_delay();
        let save_id = self.next_save_id;
        self.next_save_id = self.next_save_id.saturating_add(1).max(1);
        self.pending = Some(PendingSave {
            tracking_id: next_replay_tracking_id(),
            save_id,
            fire_at: now + save_delay,
            clip_start,
            finish_at: now,
            status,
            completed_at,
            game_language: self.game_language.clone(),
            stats,
            time_vote: FieldVote::default(),
            target_vote: FieldVote::default(),
            best_vote: FieldVote::default(),
            stats_vote_closed: false,
            pending_event_sent: false,
            phase_generation: None,
        });
        update.pending_changed = true;
        self.status = None;
        self.report = None;
        tracing::info!(?save_delay, "recording save scheduled");
    }

    fn refine_stats_vote(&mut self, m: &LevelMatch) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if pending.time_vote.counts.is_empty() || pending.stats_vote_closed {
            return false;
        }
        let expected = pending.stats.as_ref().and_then(RunIdentity::from_match);
        let incoming = RunIdentity::from_match(m);
        if let Some(expected) = expected {
            let Some(incoming) = incoming else {
                return false;
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
                return false;
            }
            if incoming != expected {
                tracing::debug!(?expected, ?incoming, "ignoring mismatched stats identity");
                return false;
            }
        }
        record_stats_vote(pending, m)
    }

    fn on_frame(
        &mut self,
        now: Instant,
        completed_at: SystemTime,
        m: &LevelMatch,
        policy: RunTrackerPolicy,
    ) -> TrackerUpdate {
        let mut update = TrackerUpdate::default();
        match m.screen {
            Screen::Start | Screen::Opts007 => {
                if self.clip_start.is_none() {
                    self.clip_start = Some(now);
                    self.status = None;
                    self.report = None;
                    self.identity_vote = RunIdentityVote::default();
                    update.ensure_replay_buffer = true;
                    update.phase = Some(RecordingStatus::Started);
                    tracing::info!("recording session started");
                }
                if let Some(identity) = RunIdentity::from_match(m) {
                    self.identity_vote.record(identity);
                }
            }
            Screen::Levels => {
                if let Some(start) = self.clip_start.take() {
                    if let Some(report) = self.report.take() {
                        let status = self.status.unwrap_or(RunStatus::Complete);
                        tracing::info!("stats screen skipped (report -> level select)");
                        self.schedule_save(now, completed_at, start, Some(report), policy, &mut update);
                        update.phase = Some(if status.is_failed() {
                            RecordingStatus::SavePending
                        } else {
                            RecordingStatus::StatsSkipped
                        });
                    } else {
                        self.status = None;
                        self.identity_vote = RunIdentityVote::default();
                        update.phase = Some(RecordingStatus::Cancelled);
                        tracing::info!("recording session abandoned (returned to level select)");
                    }
                }
            }
            Screen::Failed | Screen::Abort | Screen::Kia => {
                if self.clip_start.is_some() {
                    let report = self.canonicalize_match(m.clone());
                    self.report.get_or_insert(report);
                    if !self.status.is_some_and(RunStatus::is_failed) {
                        self.status = run_status_from_failure_screen(m.screen);
                        update.phase = Some(match m.screen {
                            Screen::Failed => RecordingStatus::Failed,
                            Screen::Abort => RecordingStatus::Aborted,
                            Screen::Kia => RecordingStatus::Kia,
                            _ => unreachable!("failure-screen branch received {:?}", m.screen),
                        });
                    }
                }
            }
            Screen::Complete => {
                if self.clip_start.is_some() {
                    let first_report = self.report.is_none();
                    let report = self.canonicalize_match(m.clone());
                    self.report.get_or_insert(report);
                    if first_report || self.status.is_some_and(RunStatus::is_failed) {
                        self.status = Some(RunStatus::Complete);
                        update.phase = Some(RecordingStatus::Complete);
                    }
                }
            }
            Screen::Stats => {
                if let Some(start) = self.clip_start.take() {
                    tracing::info!("stats detected");
                    self.schedule_save(now, completed_at, start, Some(m.clone()), policy, &mut update);
                    if let Some(pending) = self.pending.as_mut() {
                        let initial = pending.stats.clone().expect("stats save retains its match");
                        record_stats_vote(pending, &initial);
                    }
                    update.phase = Some(RecordingStatus::SavePending);
                } else {
                    update.pending_changed = self.refine_stats_vote(m);
                }
            }
            Screen::Select => {
                if self.report.is_none() && self.clip_start.take().is_some() {
                    self.status = None;
                    self.identity_vote = RunIdentityVote::default();
                    update.phase = Some(RecordingStatus::Cancelled);
                    tracing::info!("recording session cancelled (returned to difficulty selection)");
                }
            }
            Screen::Unknown => {}
        }

        if m.screen != Screen::Stats
            && let Some(pending) = self.pending.as_mut()
        {
            pending.stats_vote_closed = true;
        }
        update
    }
}

/// Tracks one recording session as it moves through the on-screen states, and
/// drives replay-buffer saves when runs finish. Fed via [`RecordingState::on_frame`].
pub struct RecordingState {
    tracker: RunTracker,
    /// Normalized timing policy fixed for this monitor session.
    tracker_policy: RunTrackerPolicy,
    save_pipeline: SavePipeline,
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
        let tracker_policy = options.tracker_policy();
        let output_policy = options.output_policy();
        let tracker = RunTracker::new(session.game_language.clone());
        RecordingState {
            tracker,
            tracker_policy,
            save_pipeline: SavePipeline::new(
                event_tx,
                recording_state,
                replay_saves,
                output_policy,
                options.recent_run_limit,
                session,
                run_catalog,
            ),
        }
    }

    pub fn set_recent_run_limit_source(&mut self, source: Arc<AtomicUsize>) {
        self.save_pipeline.set_recent_run_limit_source(source);
    }

    /// Publish a recorder state transition to the backend-retained phase store
    /// Event-stream clients see it in the next app snapshot.
    /// For `SavePending`/`StatsSkipped`, records the generation on the pending
    /// save so its completion/discard can clear that exact transition later.
    fn emit(&mut self, status: RecordingStatus) {
        let generation = self.save_pipeline.recording_state.set(status);
        if matches!(status, RecordingStatus::SavePending | RecordingStatus::StatsSkipped)
            && let Some(pending) = self.tracker.pending.as_mut()
        {
            pending.phase_generation = Some(generation);
        }
    }

    /// Update the game/template language attached to future clip metadata. Used
    /// when monitor language auto-correction detects the other game language.
    pub fn set_game_language(&mut self, game_language: String) {
        self.tracker.set_game_language(game_language);
    }

    #[cfg(test)]
    fn schedule_save(&mut self, now: Instant, clip_start: Instant, stats: Option<LevelMatch>) -> bool {
        let mut update = TrackerUpdate::default();
        self.tracker.schedule_save(
            now,
            SystemTime::now(),
            clip_start,
            stats,
            self.tracker_policy,
            &mut update,
        );
        for pending in update.ready {
            self.flush_ready(pending, now);
        }
        self.sync_pending_event(now, update.pending_changed);
        true
    }

    /// Show the provisional row once and refresh it when the voted time changes.
    fn sync_pending_event(&mut self, now: Instant, time_changed: bool) {
        let Some(pending) = self.tracker.pending.as_ref() else {
            return;
        };
        if !pending.pending_event_sent || time_changed {
            self.save_pipeline.publish_pending(pending, self.tracker_policy, now);
            self.tracker.pending.as_mut().unwrap().pending_event_sent = true;
        }
    }

    /// Build a save+trim job for the pending clip, if any, anchored to `now` as
    /// the save moment (the saved file ends at ~now, so the run is its final
    /// `elapsed` seconds). A no-op when nothing is pending.
    #[cfg(test)]
    fn take_pending_job(&mut self, now: Instant) -> Option<SaveAndTrimJob> {
        let pending = self.tracker.pending.take()?;
        Some(self.save_pipeline.job(pending, now, self.tracker_policy))
    }

    /// Save and trim the pending clip asynchronously, if any.
    fn flush_pending(&mut self, now: Instant) {
        if let Some(pending) = self.tracker.pending.take() {
            self.save_pipeline.spawn(pending, now, self.tracker_policy);
        }
    }

    fn flush_ready(&self, pending: PendingSave, now: Instant) {
        self.save_pipeline.spawn(pending, now, self.tracker_policy);
    }

    /// When the in-flight save is due to fire, or `None` when nothing is pending.
    /// The monitor loop waits on this so the save fires on time even if captured
    /// frames stop arriving (e.g. a paused source).
    pub fn pending_fire_at(&self) -> Option<Instant> {
        self.tracker.pending.as_ref().map(|pending| pending.fire_at)
    }

    /// Fire the scheduled save once its post-run padding window has elapsed. Safe
    /// to call on any tick (frame or idle wakeup); a no-op until then.
    pub fn poll_pending(&mut self, now: Instant) {
        if self.tracker.pending.as_ref().is_some_and(|pending| now >= pending.fire_at) {
            self.flush_pending(now);
        }
    }

    /// Save and trim the pending clip synchronously during shutdown, preserving
    /// the scheduled post-run padding window before OBS is asked to save.
    #[cfg(not(test))]
    fn flush_pending_on_shutdown(&mut self) {
        if let Some(pending) = self.tracker.pending.take() {
            self.save_pipeline.flush_on_shutdown(pending, Instant::now(), self.tracker_policy);
        }
    }

    #[cfg(test)]
    fn flush_pending_on_shutdown_with(
        &mut self,
        now: Instant,
        sleep: impl FnOnce(Duration),
        save: impl FnOnce(SaveAndTrimJob),
    ) {
        let Some(pending) = self.tracker.pending.take() else {
            return;
        };
        self.save_pipeline.flush_on_shutdown_with(pending, now, self.tracker_policy, sleep, save);
    }

    /// Feed the latest matched frame (and the current time). Called once per
    /// captured frame, so it also polls the pending-save timer.
    pub fn on_frame(&mut self, now: Instant, m: &LevelMatch) {
        let update = self.tracker.on_frame(now, SystemTime::now(), m, self.tracker_policy);
        if update.ensure_replay_buffer {
            ensure_replay_buffer_running_for_recording();
        }
        for pending in update.ready {
            self.flush_ready(pending, now);
        }
        self.sync_pending_event(now, update.pending_changed);
        if let Some(phase) = update.phase {
            self.emit(phase);
        }
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
        assert!(self.tracker.pending.is_none(), "test dropped RecordingState with a pending save");
    }
}
