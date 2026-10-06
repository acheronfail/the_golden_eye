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
    metadata_json,
    youtube_json
)
SELECT
    run_id,
    completed_unix_micros,
    CASE
        WHEN typeof(level_number) = 'integer' AND level_number BETWEEN 1 AND 20
        THEN level_number
        ELSE NULL
    END,
    CASE lower(trim(difficulty))
        WHEN 'agent' THEN 0
        WHEN 'secret agent' THEN 1
        WHEN '00 agent' THEN 2
        WHEN '007' THEN 3
        ELSE NULL
    END,
    status,
    time_seconds,
    retention_state,
    retention_reason,
    clip_path,
    size_bytes,
    modified_unix,
    duration_secs,
    metadata_json,
    youtube_json
FROM runs_v2;

DROP TABLE runs_v2;
