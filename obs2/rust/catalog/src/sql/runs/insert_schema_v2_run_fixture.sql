INSERT INTO runs (
    run_id,
    completed_unix_micros,
    level_number,
    difficulty,
    status,
    time_seconds,
    retention_state,
    metadata_json,
    youtube_json
)
VALUES (
    'durable-id',
    10,
    8,
    ' secret AGENT ',
    'complete',
    91,
    'kept',
    ?1,
    ?2
);
