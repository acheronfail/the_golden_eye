UPDATE runs
SET clip_path = NULL,
    size_bytes = NULL,
    modified_unix = NULL,
    duration_secs = NULL,
    retention_state = ?1,
    retention_reason = ?2,
    metadata_json = ?3
WHERE run_id = ?4;
