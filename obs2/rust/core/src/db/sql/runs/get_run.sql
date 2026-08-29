SELECT
    run_id,
    completed_unix_micros,
    retention_state,
    retention_reason,
    clip_path,
    size_bytes,
    modified_unix,
    duration_secs,
    metadata_json,
    youtube_json
FROM runs
WHERE run_id = ?1;
