CREATE TABLE IF NOT EXISTS monitor_sessions (
    session_id TEXT PRIMARY KEY NOT NULL,
    started_unix_micros INTEGER NOT NULL,
    ended_unix_micros INTEGER,
    source_name TEXT NOT NULL,
    initial_cv_language TEXT,
    plugin_version TEXT NOT NULL,
    end_reason TEXT CHECK (
        end_reason IS NULL
        OR end_reason IN (
            'userStopped',
            'replayBufferStopped',
            'obsShutdown',
            'coreReload',
            'interrupted'
        )
    ),
    CHECK (
        ended_unix_micros IS NULL
        OR ended_unix_micros >= started_unix_micros
    )
);

CREATE TABLE IF NOT EXISTS run_sessions (
    session_id TEXT NOT NULL
        REFERENCES monitor_sessions(session_id) ON DELETE CASCADE,
    run_id TEXT NOT NULL
        REFERENCES runs(run_id) ON DELETE CASCADE,
    PRIMARY KEY (session_id, run_id),
    UNIQUE (run_id)
);

CREATE INDEX IF NOT EXISTS monitor_sessions_started_idx
ON monitor_sessions(started_unix_micros DESC);

CREATE INDEX IF NOT EXISTS monitor_sessions_ended_idx
ON monitor_sessions(ended_unix_micros);
