INSERT INTO runs (
    run_id,
    completed_unix_micros,
    level_number,
    difficulty_number,
    status,
    time_seconds,
    retention_state,
    retention_reason,
    clip_path,
    size_bytes,
    modified_unix,
    duration_secs,
    metadata_json
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
ON CONFLICT(run_id) DO UPDATE SET
    clip_path = excluded.clip_path,
    size_bytes = excluded.size_bytes,
    modified_unix = excluded.modified_unix,
    duration_secs = excluded.duration_secs,
    level_number = excluded.level_number,
    difficulty_number = excluded.difficulty_number,
    status = excluded.status,
    time_seconds = excluded.time_seconds,
    metadata_json = excluded.metadata_json;
