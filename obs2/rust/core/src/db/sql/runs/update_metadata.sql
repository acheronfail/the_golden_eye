UPDATE runs
SET level_number = ?1,
    difficulty_number = ?2,
    status = ?3,
    time_seconds = ?4,
    metadata_json = ?5
WHERE run_id = ?6;
