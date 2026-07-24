SELECT
    run_id,
    completed_unix_micros,
    retention_state,
    retention_reason,
    clip_path,
    size_bytes,
    modified_unix,
    duration_secs,
    metadata_json
FROM runs
ORDER BY completed_unix_micros DESC, run_id DESC;
