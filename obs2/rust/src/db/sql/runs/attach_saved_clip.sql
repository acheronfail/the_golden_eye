UPDATE runs
SET clip_path = ?1,
    size_bytes = ?2,
    modified_unix = ?3,
    duration_secs = ?4,
    metadata_json = ?5,
    retention_state = ?6,
    retention_reason = ?7
WHERE run_id = ?8;
