//! Run detection and stats voting; returns transitions for recording to apply.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime};

use ge_clip::RunStatus;

use super::RecordingStatus;
use crate::cv::{LevelMatch, Screen};
use crate::ge;

static NEXT_REPLAY_TRACKING_ID: AtomicU64 = AtomicU64::new(1);

fn next_replay_tracking_id() -> u64 {
    NEXT_REPLAY_TRACKING_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RunTrackerPolicy {
    pub(super) pre_run_padding_secs: f64,
    pub(super) post_run_padding_secs: f64,
}

impl RunTrackerPolicy {
    fn save_delay(self) -> Duration {
        Duration::from_secs_f64(self.post_run_padding_secs)
    }
}

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
pub(super) struct PendingSave {
    /// Core-lifetime unique id for retained replay pipeline diagnostics.
    pub(super) tracking_id: u64,
    /// Identifier shared by the pending and saved WebSocket events.
    pub(super) save_id: u64,
    /// When the post-run padding window elapses and we save the buffer.
    pub(super) fire_at: Instant,
    /// When the run began -- the anchor for where the trimmed clip starts.
    pub(super) clip_start: Instant,
    /// When the run ending was detected -- the anchor for post-run padding.
    pub(super) finish_at: Instant,
    /// The final report status seen for the run (for naming/logging).
    pub(super) status: RunStatus,
    /// Wall-clock time when the run ending was detected.
    pub(super) completed_at: SystemTime,
    /// Game/template language active when this save was scheduled.
    pub(super) game_language: String,
    /// The stats-screen match, kept for naming the output clip. Its `times` are
    /// overwritten with the per-field vote winners as stats frames arrive.
    pub(super) stats: Option<LevelMatch>,
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
    pub(super) pending_event_sent: bool,
    /// The phase-store generation of this save's own `SavePending`/`StatsSkipped`
    /// transition, if it emitted one. Its completion/discard clears exactly that
    /// transition, not a quick-restarted run's identical-looking phase.
    pub(super) phase_generation: Option<u64>,
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

#[derive(Default)]
pub(super) struct TrackerUpdate {
    pub(super) ensure_replay_buffer: bool,
    pub(super) pending_changed: bool,
    pub(super) phase: Option<RecordingStatus>,
    pub(super) ready: Vec<PendingSave>,
}

/// Pure run-detection state. It translates matched screens into domain
/// transitions; [`super::RecordingState`] applies OBS, catalog, and UI side effects.
pub(super) struct RunTracker {
    clip_start: Option<Instant>,
    status: Option<RunStatus>,
    report: Option<LevelMatch>,
    identity_vote: RunIdentityVote,
    pub(super) pending: Option<PendingSave>,
    next_save_id: u64,
    game_language: String,
}

impl RunTracker {
    pub(super) fn new(game_language: String) -> Self {
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

    pub(super) fn set_game_language(&mut self, game_language: String) {
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

    pub(super) fn schedule_save(
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

    pub(super) fn on_frame(
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

#[cfg(test)]
#[path = "tests/tracker.rs"]
mod tests;
