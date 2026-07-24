# Statistics route implementation plan

Status: implemented

Route: `/statistics`

API namespace: `/api/v1/statistics`

## Goal

Add a statistics page that turns the durable run catalog into four useful views:

1. Run volume overall and by level, with status and calendar-bucket breakdowns.
2. Game-reported run time over time for a comparable level/difficulty cohort.
3. The changing mix of selected outcomes, especially aborted versus failed runs.
4. Monitoring-session summaries, including attempts and outcomes by level.

The chart renderer must be a reusable, theme-aware Svelte component. Page-specific code prepares
chart data; the chart component only renders that data.

## Catalog research

### Authoritative fields

The schema in `obs2/rust/src/db/sql/runs/create_table.sql` contains:

| Field                    | Shape                     | Statistics use                                         |
| ------------------------ | ------------------------- | ------------------------------------------------------ |
| `run_id`                 | non-null text primary key | Stable identity; not needed for aggregate marks        |
| `completed_unix_micros`  | non-null integer          | Authoritative event time and calendar bucketing        |
| `level_number`           | nullable integer          | Per-level grouping and filtering                       |
| `difficulty`             | nullable text             | Current cohort field; migrate to `difficulty_number`   |
| `status`                 | non-null text             | `complete`, `failed`, `abort`, or `kia`                |
| `time_seconds`           | nullable integer          | Game-reported run time                                 |
| `retention_state`        | non-null text             | Clip retention, not run outcome                        |
| `retention_reason`       | nullable text             | Clip retention explanation                             |
| `clip_path`, file fields | nullable                  | Not relevant to statistics                             |
| `metadata_json`          | non-null JSON             | Display metadata and backward-compatible clip metadata |
| `youtube_json`           | nullable JSON             | Not relevant to statistics                             |

`ClipMetadata` also exposes the display time, level name, ROM language, source, comment, and plugin
version. Statistics should use the normalized columns above instead of extracting these values from
`metadata_json`.

### Level and difficulty normalization review

The current table is only partly numeric:

- `level_number` is a nullable SQLite `INTEGER`. Canonical game levels are 1–20.
- `difficulty` is nullable `TEXT`, populated from `ClipMetadata.difficulty`.
- `metadata_json` and clip-container tags also retain the human-readable level and difficulty labels
  for display and re-import.
- The matcher already represents difficulty with stable integers: Agent = 0, Secret Agent = 1, 00
  Agent = 2, and 007 = 3.

The existing composite indexes make exact text comparisons workable, and this is a small local
catalog, so converting difficulty is not primarily a speed optimization. The stronger reasons are
data integrity, unambiguous grouping, smaller index keys, and using one canonical representation for
joins and filters. Text otherwise permits case, spacing, spelling, or imported-value variants that
look like separate cohorts.

Schema 3 will keep `level_number INTEGER` and replace the normalized `difficulty TEXT` column with:

```sql
difficulty_number INTEGER
    CHECK (
        difficulty_number IS NULL
        OR (
            typeof(difficulty_number) = 'integer'
            AND difficulty_number BETWEEN 0 AND 3
        )
    )
```

Add a corresponding level constraint while rebuilding the table:

```sql
level_number INTEGER
    CHECK (
        level_number IS NULL
        OR (
            typeof(level_number) = 'integer'
            AND level_number BETWEEN 1 AND 20
        )
    )
```

The numeric columns are authoritative for database filtering, grouping, PB lookup, combined-best
calculations, session summaries, and statistics indexes. A single shared conversion at catalog
import/update boundaries maps between difficulty numbers and labels. Keep readable labels in
`metadata_json` and existing clip tags for UI display, backward compatibility, and re-import; do not
use them for joins. Unknown imported labels map to a null normalized difficulty and remain visible
in overall/unknown statistics.

Do not rewrite existing `run_id` values or clip tags during migration. They are durable external
identities even though historical IDs may contain a sanitized difficulty label. New run-ID
construction may continue using the label to avoid an unnecessary identity-format change; normalized
query behavior does not depend on it.

Move the current difficulty-label normalization out of the Runs HTTP route into a shared domain
helper so monitor finalization, run edits, clip imports, migrations, and statistics cannot develop
different mappings.

### Semantics that affect the page

- Every finalized run is inserted before optional clip processing, so the catalog includes runs even
  when no video was saved.
- Deleting only a video preserves its run row. Those rows must remain in statistics.
- Deleting the run and video removes the row and therefore removes it from statistics.
- Retention state describes the clip, not gameplay. It must not be treated as a run status or used
  to exclude rows.
- `time_seconds` is optional. Time charts must omit untimed rows and state how many rows were
  omitted.
- `level_number` and `difficulty_number` can be absent, particularly on imported data. Overall
  totals include these rows; per-level results place missing level numbers in an `Unknown` bucket.
  The improvement view requires known level and difficulty.
- `completed_unix_micros` is more reliable for ordering and bucketing than the duplicated timestamp
  inside `metadata_json`.
- The recorded time has whole-second precision. The UI should format it as `m:ss` and must not imply
  sub-second accuracy.
- Editing run metadata must update the normalized level, difficulty number, status, and time
  columns, so statistics reflect edits without additional work.

### Existing indexes

The schema-2 catalog has useful indexes for:

- status plus completion timestamp;
- level, text difficulty, and completion timestamp;
- level, text difficulty, and time;
- global time ordering.

Schema 3 will recreate the two composite indexes with `difficulty_number` in place of the text
column. No statistics-specific index beyond those normalized replacements is initially necessary.
Queries can read a lean set of facts and aggregate them in Rust while holding the existing catalog
connection lock once. Session support also requires new session tables in the same explicit
migration described below.

### Monitoring-session lifecycle research

Monitoring is backend-owned and has clear lifecycle points:

- `POST /api/v1/monitor/start` validates the source, ensures the replay buffer is running, creates
  the capture context, starts the monitor worker, stores a `MonitorHandle`, and marks the retained
  snapshot as running.
- Starting the already-active source is idempotent. It must not create a second session.
- `POST /api/v1/monitor/stop` calls the shared `stop_monitor` teardown and emits a `MonitorStopped`
  event with `userStopped`.
- An unexpected OBS replay-buffer stop calls the same teardown and emits `replayBufferStopped`.
- Development hot reload explicitly stops the monitor before unloading the core.
- A normal production core unload does not currently call `stop_monitor`; OBS process shutdown and
  crashes can therefore bypass the HTTP stop path.

This is enough to store durable sessions, provided the data model permits an open or interrupted
session and startup reconciles stale rows.

Runs should be explicitly associated with their active session. Inferring membership only from
`started_at <= run_time <= ended_at` is fragile around interrupted sessions, clock boundaries,
future concurrent concepts, and missing end timestamps.

### Date-range semantics

Date range is a first-class filter shared by Overview, Improvement, and Outcomes. Session listing
uses the same range but includes sessions that overlap it.

The range control is one native select so the active choice is always visible without consuming a
row of preset buttons. Its options are:

| Label          | Local-time meaning                                     |
| -------------- | ------------------------------------------------------ |
| Today          | local midnight through now                             |
| Last 7 days    | local midnight six days ago through now                |
| Last 30 days   | local midnight 29 days ago through now                 |
| Last 12 months | same local calendar date 12 months ago through now     |
| All time       | no lower bound, through now                            |
| Custom         | inclusive local start and end dates chosen by the user |

Use unambiguous rolling labels rather than “last week” or “last month”, which can mean either a
trailing duration or the previous calendar period. The custom end date is inclusive in the UI and
converted to the exclusive start of the following local day for the API.

Persist the selected tab, preset/custom range, bucket, level, and difficulty in URL search
parameters so reload/back navigation preserves the view. Also keep a versioned browser-local
preferences object containing every route control: those URL-backed fields, both status checkbox
sets, outcome measure, level sort, and selected session. Restore it before the initial request when
the route has no explicit value for a URL-backed field; explicit URL parameters take precedence.
This state is UI-only and must never be sent to or persisted by the backend.

### Timezone and calendar buckets

The backend and browser run on the same local machine. Aggregate day, week, and month buckets in
Rust with `chrono::Local`, using historical local offsets so daylight-saving changes are handled
correctly.

- Day starts at local midnight.
- Week starts on Monday.
- Month starts on the first local calendar day.
- Return each bucket start as RFC 3339 with its offset.
- Treat the range as `[from, to)` to avoid double-counting boundary runs.

This is preferable to SQLite's UTC date functions and to sending a fixed browser offset, which would
be wrong across daylight-saving transitions.

## Recommended information architecture

The route is one responsive page with four sticky tabs: Overview, Improvement, Outcomes, and
Sessions. A compact Date range select sits below the tabs. Choosing Custom reveals native start/end
date fields on a second row. Bucket, level, difficulty, and status controls remain local to the view
they affect.

### 1. Overview

Show:

- total attempts in the selected date range;
- total monitoring-session time overlapping the selected date range;
- most-played level;
- all-time combined best time across the three standard difficulties;
- all-time combined best time for Agent, Secret Agent, and 00 Agent individually;
- horizontal stacked bars for attempts per level, split by status;
- a stacked time series of all attempts by day, week, or month.

The level chart should support mission order and total-attempt order. Default to total attempt order
so the busiest levels are immediately visible.

Total session time is the sum of each monitoring session's overlap with the selected date range.
Completed sessions use their stored end timestamp, active sessions are capped at the range end, and
interrupted sessions use their last associated run as the last trustworthy timestamp.

### Combined best-time calculation

Combined best time is an all-time personal-best metric and does not change with the page's
date-range select.

For each of the three standard difficulties—Agent, Secret Agent, and 00 Agent:

1. Create one cell for each canonical level number `1..=20`.
2. For each cell, find the minimum `time_seconds` among `status = 'complete'` rows matching that
   level and difficulty.
3. Use `1023` seconds when no qualifying completed time exists. This is GoldenEye's maximum
   representable game time.
4. Sum the 20 cells to produce the difficulty total.
5. Sum the three difficulty totals to produce the overall combined time.

The 007 custom difficulty is intentionally excluded. Unknown levels/difficulties, non-complete runs,
null times, negative times, and values greater than 1023 do not qualify. A recorded value of exactly
1023 still counts as recorded even though it equals the fallback.

Return and display coverage alongside each total, for example `18/20 levels recorded`, so a
fallback-heavy total cannot be mistaken for a complete set. Format totals as durations such as
`5:41:00`, without wrapping at 24 hours.

### 2. Improvement

Show a time-spaced point/line chart of `time_seconds` over `completed_unix_micros`.

Required controls:

- one level;
- one difficulty;
- any combination of run statuses;
- date range.

Level and difficulty are required because raw times from different missions or difficulties are not
comparable. An "all levels" line would suggest a trend that can instead be caused by changing
mission mix.

Presentation:

- points use stable status colors and distinct point shapes;
- the thin connecting line follows chronological attempts;
- an optional running-best line is shown for completed runs;
- the tooltip shows local date/time, status, game time, level, and difficulty;
- untimed runs are not plotted, with an adjacent `N untimed runs omitted` note;
- a single timed run renders as a point without a misleading line;
- equal timestamps remain stable by `run_id`.

Do not average game times by day/week/month in the first release. Individual attempts are the useful
improvement signal, while an average can hide personal bests and outliers. If dense histories later
need a trend, add a rolling median as a separately labeled series.

### 3. Outcomes

Show a grouped bar chart by day, week, or month with a configurable set of statuses. Each calendar
bucket is one group and each selected status is a separate side-by-side bar. Default to `abort` and
`failed`.

Provide a `Share / Count` measure toggle:

- `Share` is the default and expresses each selected status as its percentage of the selected-status
  total within that bucket. The grouped bars share a 0–100% y-axis. This answers whether aborts are
  becoming less common even when play volume changes.
- `Count` shows the absolute workload and prevents a small sample from looking more important than
  it is.

The tooltip always includes both count and percentage. Empty calendar buckets remain visible with
zero-valued groups so gaps in play are not compressed away. Reduce x-axis tick density before
narrowing bars beyond legibility; do not horizontally scroll the chart.

### 4. Sessions

Show sessions that overlap the selected date range, newest first. The initial selection is the
newest session. Each session summary shows:

- local start and end time, duration, and stop reason;
- source name;
- total attempts and counts by status;
- completion percentage;
- most-played level;
- per-level and per-difficulty attempts, status counts, average timed run, and best completed time;
- attempts over elapsed session time, using the generic chart.

Do not show an overall average run time across mixed levels or difficulties. Averages are only
meaningful inside a level/difficulty cohort. Label the denominator, for example
`average of 18 timed attempts`.

An active session can appear with `In progress` instead of an end time. An interrupted session
appears with an unknown duration rather than inventing an end timestamp.

Post-monitoring-session presentation is intentionally out of scope. The stored `session_id`,
explicit run associations, and end reason are sufficient for a future `MonitorStopped { sessionId }`
event or redirect to a session detail without another schema change.

## Design direction

The accompanying design exploration demonstrates the four tabs, compact range select/custom fields,
and session summary. The final page should use the existing OBS-like application theme:

- page width and heading treatment match the Runs route;
- panels use `obs-panel`;
- grid and axes use muted border tokens;
- labels use `obs-muted` and `obs-dim`;
- chart colors come from named theme variables, never hardcoded component colors;
- every status-bearing chart maps complete runs to `--obs-success` and failed runs to
  `--obs-danger`;
- abort and KIA are negative outcomes in the same danger family, so they also use `--obs-danger` and
  are distinguished non-chromatically;
- attempts-by-level bars reuse that existing success and danger palette;
- complete uses a bright `--obs-success` border over `--obs-success-surface`;
- failed, aborted, and KIA all use a bright `--obs-danger` border over `--obs-danger-surface`;
- failed has the plain solid danger surface, aborted adds repeating diagonal lines, and KIA adds a
  repeating dot field;
- the same patterns appear in the legend and tooltip labels remain explicit, so color is never the
  only signal.

The attempts-by-level pattern mapping is fixed:

| Status   | Border          | Base fill               | Non-color encoding  |
| -------- | --------------- | ----------------------- | ------------------- |
| Complete | `--obs-success` | `--obs-success-surface` | plain               |
| Failed   | `--obs-danger`  | `--obs-danger-surface`  | plain solid surface |
| Abort    | `--obs-danger`  | `--obs-danger-surface`  | 45° repeating lines |
| KIA      | `--obs-danger`  | `--obs-danger-surface`  | repeating dots      |

Use only those existing application tokens. Do not introduce new green/red literals or separate hues
for the three danger outcomes. SVG patterns use `userSpaceOnUse` so their scale stays consistent
across differently sized segments. Recommended starting density is a 6 px tile, 2 px diagonal line,
and 1.25 px dots; verify at narrow widths and adjust density rather than color.

The referenced
[Float View `Chart.svelte`](https://github.com/acheronfail/float-view/blob/master/src/components/Chart.svelte)
is useful inspiration for:

- sizing the SVG from its measured pixel width;
- time-proportional x positions;
- selecting the nearest point from pointer/touch input;
- clamping the tooltip to the chart bounds;
- deriving readable y-axis ticks;
- reducing the number of rendered points only when density exceeds available pixels.

Adapt those ideas rather than copying the component. The Golden Eye chart needs theme tokens,
multiple chart forms, keyboard interaction, empty states, reduced-motion behavior, and status-aware
accessibility.

## Backend design

### Database layer

Keep SQL in `obs2/rust/src/db/runs.rs`, following the repository convention.

Add a private lean row type:

```rust
pub struct RunStatisticFact {
    pub run_id: String,
    pub completed_unix_micros: i64,
    pub level_number: Option<i32>,
    pub difficulty_number: Option<i32>,
    pub status: RunStatus,
    pub time_seconds: Option<i32>,
}
```

Add a query that selects only those columns for an optional `[from, to)` range, ordered by
completion timestamp and `run_id`.

Add one grouped all-time query for combined bests:

```sql
SELECT level_number, difficulty_number, MIN(time_seconds)
FROM runs
WHERE status = 'complete'
  AND level_number BETWEEN 1 AND 20
  AND difficulty_number IN (0, 1, 2)
  AND time_seconds BETWEEN 0 AND 1023
GROUP BY level_number, difficulty_number;
```

Initialize the 60-cell matrix to the 1023-second fallback, then replace returned cells. Track
coverage separately so a real 1023-second completion is distinguishable from a missing cell. Do not
issue 60 individual `best_time` queries.

Add `RunCatalog::statistics(...)` in `run_catalog.rs`. It should:

1. lock the single connection once;
2. load the lean facts;
3. compute totals and status counts;
4. group known and unknown levels;
5. create contiguous local calendar buckets;
6. create the selected level/difficulty time series;
7. calculate all-time combined bests;
8. return domain statistics, not chart-renderer data.

This keeps aggregation testable in Rust and avoids shipping the complete catalog or `metadata_json`
to the browser.

### Session schema

Add two tables. Keep their SQL and queries in `db/runs.rs` to preserve the repository's SQL
ownership convention.

```sql
CREATE TABLE monitor_sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    started_unix_micros INTEGER NOT NULL,
    ended_unix_micros INTEGER,
    source_name TEXT NOT NULL,
    initial_cv_language TEXT,
    plugin_version TEXT NOT NULL,
    end_reason TEXT,
    CHECK (
        ended_unix_micros IS NULL
        OR ended_unix_micros >= started_unix_micros
    )
);

CREATE TABLE run_sessions (
    session_id TEXT NOT NULL
        REFERENCES monitor_sessions(session_id) ON DELETE CASCADE,
    run_id TEXT NOT NULL
        REFERENCES runs(run_id) ON DELETE CASCADE,
    PRIMARY KEY (session_id, run_id),
    UNIQUE (run_id)
);

CREATE INDEX monitor_sessions_started_idx
    ON monitor_sessions(started_unix_micros DESC);
CREATE INDEX monitor_sessions_ended_idx
    ON monitor_sessions(ended_unix_micros);
```

The join table is preferable to inferring membership by timestamps or adding a nullable session
foreign key to `runs`. A run belongs to at most one session. Imported historical runs remain
unassociated.

Session IDs use the high-precision start timestamp with collision suffixing under the catalog lock,
matching the existing stable run-ID approach.

Supported end reasons:

- `userStopped`;
- `replayBufferStopped`;
- `obsShutdown`;
- `coreReload`;
- `interrupted`.

`ended_unix_micros` remains null for an interrupted session because the exact end is unknown. On
catalog startup, mark stale open rows as `interrupted` without fabricating an end timestamp. A
genuinely active session is created only after startup reaches the monitor lifecycle; no session
survives across a core process restart.

### Schema migration

This changes the catalog from schema 2 to schema 3. Schema 2 contains durable run history and must
not be dropped.

Replace the current “any mismatch means reset” behavior with explicit version handling:

1. New database: create schema 3 directly.
2. Schema 1: preserve the documented pre-release reset/reseed behavior, then create schema 3.
3. Schema 2: transactionally rebuild `runs` with constrained `level_number` and `difficulty_number`,
   map recognized labels to 0–3, preserve all other logical values and JSON verbatim, recreate its
   indexes, create the session tables/indexes, and update the metadata version to 3.
4. Schema 3: initialize idempotently.
5. Newer/unknown schema: refuse to open with a clear error rather than destructively downgrading it.

The migration trims and compares labels case-insensitively, mapping Agent → 0, Secret Agent → 1, 00
Agent → 2, and 007 → 3. Unknown or null text values become null. Migration tests must prove every
other run field, `metadata_json`, `youtube_json`, clip association, and run ID remains byte-for-byte
equivalent while the normalized difficulty receives the expected number.

### Session writes

At monitor start:

1. Preserve the current idempotency check; restarting the same active source returns success without
   creating a row.
2. After replay-buffer and matcher/capture initialization succeed, create the session before the
   worker can finalize a run.
3. Pass `session_id` into `RecordingState` and retain it in `MonitorHandle`.
4. If worker creation fails, remove the empty provisional session.
5. If catalog session creation fails, log it and continue monitoring unassociated, matching the
   existing resilience of untracked finalized runs.

When a run is finalized, insert the run and its `run_sessions` association in the same SQLite
transaction. A failed association must not leave a partially inserted mapping.

Change the shared monitor teardown to accept an end reason. Record the end timestamp when teardown
begins, before waiting for the worker join, then use:

- `userStopped` from the HTTP stop route;
- `replayBufferStopped` from the OBS replay-buffer callback;
- `coreReload` for development reload;
- `obsShutdown` from graceful production core unload.

Crashes and forced termination remain open rows and are reconciled to `interrupted` on next startup.

### HTTP route

Create `obs2/rust/src/http/routes/statistics.rs` and register:

```text
GET /api/v1/statistics
GET /api/v1/statistics/sessions
GET /api/v1/statistics/sessions/{sessionId}
```

Query parameters:

| Parameter          | Values                      | Default              |
| ------------------ | --------------------------- | -------------------- |
| `from`             | RFC 3339 instant            | beginning of catalog |
| `to`               | RFC 3339 instant, exclusive | now                  |
| `bucket`           | `day`, `week`, `month`      | `week`               |
| `levelNumber`      | `1` through `20`            | no selected cohort   |
| `difficultyNumber` | integer `0` through `3`     | no selected cohort   |

Presets are resolved in the frontend to exact RFC 3339 `from` and `to` values. This keeps the API
deterministic and makes custom and preset ranges share one contract.

Status filters are intentionally absent from the request. The response contains all four status
counts, letting checkboxes update charts instantly without network calls.

Validate:

- `from < to`;
- valid bucket value;
- level range;
- a bounded maximum returned time-point count.

For an exceptionally large selected cohort, preserve first/last points and extrema in pixel-oriented
downsampling only after a measured need. Do not silently truncate the first implementation.

The sessions list accepts `from` and `to` and returns sessions whose intervals overlap the requested
range. An open session overlaps through now; an interrupted session with an unknown end is included
when its start is inside the range or an associated run is inside the range. Session detail is
addressed by stable `sessionId` and always returns the complete session, even when it crosses the
active date-range boundary.

### Response contract

```ts
type RunStatus = "complete" | "failed" | "abort" | "kia";

interface StatusCounts {
  total: number;
  complete: number;
  failed: number;
  abort: number;
  kia: number;
}

interface StatisticsResponse {
  range: {
    from: string | null;
    to: string;
    bucket: "day" | "week" | "month";
    timeZone: string;
  };
  summary: {
    counts: StatusCounts;
    totalSessionSeconds: number;
    timedRuns: number;
    untimedRuns: number;
    firstRunAt: string | null;
    lastRunAt: string | null;
    combinedBestTimes: {
      fallbackSeconds: 1023;
      overallSeconds: number;
      recordedCells: number;
      totalCells: 60;
      byDifficulty: Array<{
        difficultyNumber: 0 | 1 | 2;
        totalSeconds: number;
        recordedLevels: number;
        totalLevels: 20;
      }>;
    };
  };
  byLevel: Array<{
    levelNumber: number | null;
    counts: StatusCounts;
  }>;
  overallBuckets: Array<{
    start: string;
    end: string;
    counts: StatusCounts;
  }>;
  selectedCohort: {
    levelNumber: number;
    difficultyNumber: 0 | 1 | 2 | 3;
    counts: StatusCounts;
    buckets: Array<{
      start: string;
      end: string;
      counts: StatusCounts;
    }>;
    runTimes: Array<{
      runId: string;
      completedAt: string;
      status: RunStatus;
      timeSeconds: number;
    }>;
    untimedRuns: number;
  } | null;
}

interface MonitoringSessionSummary {
  sessionId: string;
  startedAt: string;
  endedAt: string | null;
  sourceName: string;
  initialCvLanguage: string | null;
  pluginVersion: string;
  endReason:
    | "userStopped"
    | "replayBufferStopped"
    | "obsShutdown"
    | "coreReload"
    | "interrupted"
    | null;
  counts: StatusCounts;
  distinctLevels: number;
}

interface MonitoringSessionDetail extends MonitoringSessionSummary {
  levels: Array<{
    levelNumber: number | null;
    difficultyNumber: 0 | 1 | 2 | 3 | null;
    counts: StatusCounts;
    timedRuns: number;
    averageTimeSeconds: number | null;
    bestCompletedTimeSeconds: number | null;
  }>;
  attempts: Array<{
    runId: string;
    completedAt: string;
    elapsedSeconds: number;
    levelNumber: number | null;
    difficultyNumber: 0 | 1 | 2 | 3 | null;
    status: RunStatus;
    timeSeconds: number | null;
  }>;
}
```

Return zero-filled status keys so the frontend does not need to distinguish absent from zero. Use
camelCase serialization.

### Error behavior

- `400` for invalid filters or ranges;
- `500` with a stable public message for catalog/query failures;
- `200` with zero counts and empty arrays for an empty catalog or empty range.

Run aggregation inside `spawn_blocking`, matching existing catalog routes.

## Frontend design

### Navigation and routing

- Add `{ href: '/statistics', label: 'Statistics' }` after Runs in the layout menu.
- Add `/statistics` to the replay-buffer-unavailable exemption.
- Add `obs2/browser/src/routes/statistics/+page.svelte`.
- Set `<title>Statistics</title>`.
- Match the Runs route's responsive `max-w-3xl` page shell.

### API client

Extend `obs2/browser/src/lib/api.ts` with:

- the response and filter types above;
- `backend.getStatistics(filters, { signal })`;
- `backend.getStatisticsSessions(range, { signal })`;
- `backend.getStatisticsSession(sessionId, { signal })`;
- query serialization that omits unset optional filters.
- one exhaustive numeric-difficulty-to-label mapping for controls and display.

Keep the existing Runs API's human-readable metadata contract for compatibility. The new Statistics
API uses `difficultyNumber` because its normalized DB cohort is numeric; frontend labels are
presentation, not identifiers.

Abort an in-flight request when filters change or the route unmounts. Show the existing error-alert
treatment on failure and preserve the last successful result while a refresh is pending.

### Components

Create only components that have independent behavior or a meaningful story:

1. `Chart.svelte`
   - generic SVG renderer;
   - no API calls, run types, status names, or statistics-specific transformations.
2. `DateRangeSelect.svelte`
   - one native select containing Today, Last 7 days, Last 30 days, Last 12 months, All time, and
     Custom;
   - the closed select always shows the active range;
   - custom inclusive start/end date validation.
3. `StatisticsFilters.svelte`
   - bucket, level, and difficulty controls below the shared date range.
4. `StatusSeriesPicker.svelte`
   - accessible multi-select checkboxes for chart-local status visibility.
5. `SessionStatistics.svelte`
   - session selection, high-level metrics, cohort table, and attempt chart.
6. `CombinedBestTimes.svelte`
   - overall total plus the three standard-difficulty totals and coverage;
   - compact vertical layout that remains readable at the dock's minimum width.
7. `StatisticsDashboard.svelte`
   - composition used by the route and Storybook.

Keep summary stat markup inside `StatisticsDashboard` unless repetition proves it needs its own
component.

### Generic chart contract

Put renderer types beside the component in `Chart.ts`:

```ts
type ChartKind = "line" | "stackedBar" | "groupedBar" | "horizontalStackedBar";
type XValue = number | string;

interface ChartPoint {
  x: XValue;
  y: number;
  label?: string;
  detail?: string;
}

interface ChartSeries {
  id: string;
  label: string;
  points: ChartPoint[];
  color: string;
  surfaceColor?: string;
  pattern?: "plain" | "diagonal" | "dots";
  shape?: "circle" | "square" | "diamond" | "triangle";
}

interface ChartData {
  kind: ChartKind;
  series: ChartSeries[];
  xType: "time" | "category";
  valueMode?: "absolute" | "percent";
}
```

Other props cover title, accessible description, x/y labels, value formatters, empty message, and
optional selected point. Keep series colors and surfaces as CSS custom-property values; Tailwind
utilities still own stroke/fill/layout properties. `pattern` is renderer-generic and can be reused
by any categorical bar chart.

Page-specific pure functions in `statisticsView.ts` produce:

- attempts-by-level chart data;
- overall-attempt chart data;
- run-time chart data;
- outcome-mix chart data;
- session attempts-over-elapsed-time chart data;
- combined-duration and coverage labels;
- formatted summary values and level labels.

This is the boundary that makes `Chart.svelte` reusable.

### Chart rendering details

- Use inline SVG and a measured pixel-sized `viewBox`.
- Reserve fixed logical margins for axes after measuring the longest tick labels.
- Use `ResizeObserver`; redraw derived geometry when width changes.
- Use a linear time scale for timestamps and a band scale for categories.
- Generate sensible ticks in local time and format run durations as `m:ss`.
- Render neutral grid lines before marks.
- Render bars, lines, and points from the same prepared series contract.
- For `groupedBar`, allocate one band per x value and one equal-width sub-band per visible series.
  Removing a status redistributes the available group width.
- Define SVG pattern IDs with a per-chart unique prefix so multiple charts cannot collide in the
  document.
- Render a bar's bright series color as its 2 px stroke. Fill it with the surface color directly for
  `plain`, or with a `userSpaceOnUse` SVG pattern whose base is the surface and whose lines/dots use
  the same bright series color.
- Keep patterns stationary during resize/filter transitions; only bar geometry moves.
- Use an overlay hit region to find the nearest point for mouse and touch.
- Support left/right arrow navigation between points and Escape to clear selection.
- Clamp the tooltip to the plot bounds.
- Put `<title>` and `<desc>` in every SVG.
- Provide a concise visible selected-point summary for keyboard and touch users.
- Do not animate initial render. Animate filter transitions only when point identity can be
  preserved, and disable motion under `prefers-reduced-motion`.
- Keep empty, loading, one-point, all-zero, and all-hidden states explicit.

### Theme additions

Reuse the existing status variables in `layout.css` directly:

```css
--obs-success
--obs-success-surface
--obs-danger
--obs-danger-surface
--obs-border-muted
--obs-text-dim
```

These are already used by the monitor UI, alerts, recent-run statuses, and run-list status styling.
Do not add chart-specific aliases unless a later theme requires a separate semantic layer.

Use Tailwind v4 utilities for component styling. Runtime SVG coordinates and the selected series
token may travel through CSS custom properties, as permitted by the repository convention. Do not
add component `<style>` blocks.

### Responsive behavior

- Desktop/tablet: filters share a wrapping row; charts use the full content width.
- The date-range select remains one compact field; Custom fields occupy a second row.
- Narrow layout: filters stack; legends wrap; summary values use an auto-fit grid.
- Horizontal level bars keep labels readable without horizontal page scrolling.
- Session cohort columns collapse to the essential level, attempts, outcomes, and average fields on
  narrow screens; full detail remains available through row labels.
- Reduce tick count before reducing font size.
- Never clip tooltips or axis labels.

## Storybook plan

Every new component receives a story under `obs2/browser/src/stories/`.

### `Chart.stories.svelte`

- line chart with irregular timestamps and two status series;
- one-point line data;
- dense time data;
- stacked absolute bars;
- grouped absolute bars;
- grouped percentage bars with two and four visible statuses;
- horizontal stacked level bars with complete/failed/abort/KIA pattern mapping;
- adjacent narrow danger segments proving solid/diagonal/dot patterns stay distinct;
- empty and all-zero data;
- long category labels;
- narrow responsive viewport.

### `StatisticsFilters.stories.svelte`

- default bucket/level filters;
- selected level/difficulty;
- narrow wrapping layout.

### `DateRangeSelect.stories.svelte`

- each option selected and visible in the closed control;
- valid custom range;
- invalid/reversed custom range;
- narrow wrapping layout.

### `StatusSeriesPicker.stories.svelte`

- abort/failed default;
- all statuses;
- one remaining status;
- disabled/loading state if needed.

### `StatisticsDashboard.stories.svelte`

- representative populated catalog;
- empty catalog;
- selected cohort with untimed rows;
- loading;
- API error;
- unknown-level data;
- Sessions tab with a completed session;
- active and interrupted sessions;
- session with no attempts;
- narrow responsive viewport.

### `SessionStatistics.stories.svelte`

- representative multi-level session;
- one-level session;
- active session;
- interrupted/unknown-duration session;
- empty session and loading detail;

### `CombinedBestTimes.stories.svelte`

- all 60 level/difficulty cells recorded;
- partially recorded with per-difficulty coverage;
- empty catalog using all 1023-second fallbacks;
- a real recorded 1023-second completion;
- totals longer than 24 hours;
- narrow responsive viewport.

Use deterministic fixture dates and data. Do not derive Storybook fixtures from the user's local
catalog.

## Test plan

### Rust database and aggregation tests

- empty catalog;
- totals across all four statuses;
- rows without clips remain included;
- retention state does not change totals;
- unknown level and difficulty behavior;
- timed versus untimed counts;
- edited normalized metadata is reflected;
- day, Monday-based week, and month boundaries;
- daylight-saving boundary in the local aggregation helper;
- explicit `[from, to)` boundaries;
- Today, rolling 7/30-day, rolling 12-month, all-time, and custom range conversion;
- zero-filled missing buckets;
- stable ordering when completion timestamps match;
- selected level/difficulty cohort;
- empty combined-best matrix totals 1023 seconds for each of 60 cells;
- combined best chooses the minimum completed time per level/difficulty;
- failed, abort, KIA, 007, unknown, null, negative, and greater-than-1023 rows do not qualify;
- a real 1023-second completion increments coverage;
- partial coverage totals and the overall sum of three difficulty totals;
- combined best is all-time and unaffected by the selected statistics date range;
- schema-2-to-3 migration preserves every run field except the intentional normalized
  text-difficulty-to-number conversion;
- all four canonical difficulty mappings and unknown/null imported-label handling;
- level and difficulty check constraints reject out-of-range normalized values;
- PB, combined-best, cohort, and session queries use numeric difficulty;
- session start/end and every end reason;
- idempotent monitor start creates one session;
- explicit run/session association in the finalized-run transaction;
- imported historical runs remain unassociated;
- stale open sessions become interrupted without a fabricated end;
- run deletion cascades only the association, not the session;
- session deletion would remove associations but preserve runs.

Make bucket-boundary helpers accept an explicit timezone/date input where necessary so tests are
deterministic even though production uses `chrono::Local`.

### HTTP route tests

- defaults and valid query parsing;
- every invalid query case returns `400`;
- empty response is `200`;
- response uses camelCase and stable status keys;
- session overlap filtering and complete session detail;
- active/interrupted session serialization;
- unknown session returns `404`;
- catalog failure maps to the public `500` response.

### Frontend unit/component tests

- each chart-data builder produces correct totals and ordering;
- grouped Share bars normalize selected statuses only;
- hidden statuses do not affect the denominator;
- grouped bars use equal sub-bands and reclaim width when statuses are hidden;
- combined best formats durations beyond 24 hours without wrapping;
- combined best displays overall and Agent/Secret Agent/00 Agent totals with accurate coverage;
- empty combined-best data displays fallback totals without presenting them as recorded;
- attempts-by-level series map complete to the success tokens and all three negative outcomes to the
  danger tokens;
- failed uses plain danger fill, abort uses diagonal lines, and KIA uses dots;
- SVG pattern IDs remain unique across multiple chart instances;
- legend swatches use the same fill/border/pattern as their marks;
- patterns remain legible in narrow segments and do not rely on color;
- run time formatting and local date labels;
- unknown level label;
- date preset boundaries and custom inclusive-end conversion;
- range select displays the active option, hides custom fields for presets, and reveals them only
  for Custom;
- range state round-trips through URL search parameters;
- all route controls round-trip through versioned browser storage, reject malformed values, and
  defer to explicit URL parameters;
- chart empty/one-point/all-zero rendering;
- status picker never permits an inaccessible unlabeled control;
- filter changes abort stale requests;
- Sessions tab lazily loads the list and selected detail;
- session-level averages never combine different level/difficulty cohorts;
- active/interrupted/empty session states;
- error and loading states;
- menu link active state;
- `/statistics` remains reachable without replay-buffer availability.

### Verification commands

Run focused tests during development, then:

```sh
cd obs2/browser
npm run check
npm run lint
npm run test:unit

cd ../..
just test-rust
```

If route behavior touches integration coverage, also run `just test-integration`.

## Confirmed product decisions

The plan has no remaining open product decisions:

- Improvement requires one level and difficulty. It does not mix incomparable raw times across
  cohorts.
- Session averages include every timed attempt within the level/difficulty cohort, label the
  timed-attempt count, and show best completed time separately.
- The initial range is Last 30 days, the initial bucket is Week, and Improvement initially selects
  the most-played known level/difficulty cohort in that range.
- Complete uses `--obs-success`; failed uses `--obs-danger`.
- Abort and KIA also use the danger family.
- Failed is the plain danger fill, abort uses diagonal lines, and KIA uses dots.
- Outcomes uses grouped bars, not stacked bars.
- Outcome Share mode defaults to abort plus failed.
- Combined best is all-time, covers only Agent/Secret Agent/00 Agent, and substitutes 1023 seconds
  for each missing level/difficulty cell.
- A completed-running-best line is visible by default with no first-release toggle.
- Sessions are included when they overlap the selected range.
- Zero-attempt sessions are stored and shown.
- Catalog level and difficulty dimensions are numeric: levels 1–20 and difficulties 0–3.
  Human-readable labels remain in metadata/tags and are mapped at boundaries.
- The schema-2-to-3 migration performs the text-to-number difficulty conversion transactionally
  while preserving durable run IDs, JSON, and clip associations.

## Implementation sequence

1. Add shared difficulty conversion, explicit schema migration support, numeric difficulty storage,
   and session-capable schema 3, with preservation tests.
2. Add session create/end/reconcile/association operations in `db/runs.rs`.
3. Thread the optional session ID through monitor lifecycle and finalized-run transactions, with
   lifecycle tests.
4. Add statistics domain types and lean normalized queries.
5. Add aggregation, date-range, calendar-bucket, and session-summary helpers.
6. Add statistics/session HTTP routes, validation, serialization, and route tests.
7. Add frontend API types/client methods, URL range state, and fixture builders.
8. Build the generic `Chart.svelte` and its stories/tests before page composition.
9. Build date range, statistics filters, status picker, and session components.
10. Build `StatisticsDashboard` from prepared chart-data functions and add all states.
11. Add `/statistics`, navigation, replay-buffer exemption, and page tests.
12. Run formatting, focused tests, full frontend checks, and Rust tests.
13. Add versioned browser-only route preference restoration with URL precedence and validation.
14. Manually inspect narrow and wide Storybook views, pointer/touch selection, keyboard navigation,
    tooltip clamping, reduced motion, and empty data.

## Acceptance criteria

- Statistics is visible in the primary menu and `/statistics` remains reachable when the replay
  buffer is unavailable.
- One compact select always shows the active range and offers Today, Last 7 days, Last 30 days, Last
  12 months, All time, or a validated custom inclusive date range.
- Date filters persist in the URL and use local calendar boundaries.
- Tab, range, grouping, cohort, status checkboxes, chart modes/sorts, and session selection restore
  from browser-local storage without backend persistence; explicit URL state wins.
- All durable catalog rows are counted regardless of clip presence or retention state.
- Overview shows the all-time combined best across 60 standard level/difficulty cells and separate
  Agent, Secret Agent, and 00 Agent totals.
- Every missing combined-best cell contributes exactly 1023 seconds, while coverage makes recorded
  and fallback cells distinguishable.
- Users can view overall and per-level attempts with day/week/month buckets and status splits.
- Users can inspect game time over chronological attempts for one level/difficulty and filter the
  visible statuses.
- Users can compare any chosen outcome statuses as side-by-side grouped bars in Count or Share mode
  over time.
- Every successfully tracked monitoring session has durable start metadata, an explicit run
  association, and a normal end timestamp/reason when available.
- Users can select sessions and inspect total and per-level/difficulty statistics.
- Interrupted sessions remain truthful about their unknown duration.
- Untimed and unknown-dimension data are disclosed rather than silently discarded.
- Statistics grouping, joins, and filters use constrained numeric level/difficulty columns; labels
  remain presentation and import/export metadata.
- All charts use one generic responsive SVG component and stable theme tokens.
- Attempts-by-level uses existing success/danger tokens with plain, diagonal, and dot patterns that
  remain distinguishable without color.
- Every new component has materially distinct Storybook states.
- Empty, loading, error, narrow, and accessibility states are tested.
- Schema 2 upgrades to schema 3 without losing or reseeding durable run history.

## Deliberate first-release limits

- No cross-player or cloud statistics; the page reflects the local catalog only.
- No automatic post-monitoring-session modal, notification, or redirect. The data model
  intentionally supports that future feature.
- No comparison of raw run times across different levels/difficulties.
- No mean-time aggregation until a rolling-median or percentile design is validated.
- No clip-retention or YouTube metrics.
- No chart library dependency unless the generic SVG implementation proves unmaintainable during
  implementation.
