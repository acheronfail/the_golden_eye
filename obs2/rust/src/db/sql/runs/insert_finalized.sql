INSERT INTO runs (
    run_id,
    completed_unix_micros,
    level_number,
    difficulty_number,
    status,
    time_seconds,
    retention_state,
    retention_reason,
    metadata_json
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);
