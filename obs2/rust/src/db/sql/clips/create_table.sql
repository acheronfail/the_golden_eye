CREATE TABLE IF NOT EXISTS runs (
    run_id TEXT PRIMARY KEY NOT NULL,
    completed_unix_micros INTEGER NOT NULL,
    level_number INTEGER,
    difficulty TEXT,
    status TEXT NOT NULL,
    time_seconds INTEGER,
    retention_state TEXT NOT NULL,
    retention_reason TEXT,
    clip_path TEXT UNIQUE,
    size_bytes INTEGER,
    modified_unix INTEGER,
    duration_secs REAL,
    metadata_json TEXT NOT NULL CHECK (json_valid(metadata_json)),
    youtube_json TEXT CHECK (youtube_json IS NULL OR json_valid(youtube_json))
);
