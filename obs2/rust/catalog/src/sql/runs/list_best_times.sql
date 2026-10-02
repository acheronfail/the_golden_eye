WITH ranked AS (
    SELECT run_id,
        ROW_NUMBER() OVER (
            PARTITION BY level_number, difficulty_number
            ORDER BY time_seconds, completed_unix_micros DESC, run_id DESC
        ) AS rank
    FROM runs
    WHERE status = 'complete'
      AND level_number BETWEEN 1 AND 20
      AND difficulty_number BETWEEN 0 AND 2
      AND time_seconds >= 0
)
SELECT runs.run_id, completed_unix_micros, retention_state, retention_reason,
    clip_path, size_bytes, modified_unix, duration_secs, metadata_json, youtube_json
FROM runs JOIN ranked ON runs.run_id = ranked.run_id
WHERE ranked.rank = 1
ORDER BY level_number, difficulty_number;
