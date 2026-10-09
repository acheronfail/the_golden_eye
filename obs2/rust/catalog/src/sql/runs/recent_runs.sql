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
WHERE coalesce(retention_reason, '') NOT IN ('theElite', 'manualEntry')
    -- History origins remain identifiable after clip deletion changes the retention reason.
    AND run_id NOT GLOB 'the-elite-*'
    AND coalesce(json_extract(metadata_json, '$.sourceName'), '') != 'Manual entry'
ORDER BY completed_unix_micros DESC, run_id DESC
LIMIT ?1;
