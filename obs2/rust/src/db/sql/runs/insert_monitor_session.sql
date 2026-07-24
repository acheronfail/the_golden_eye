INSERT INTO monitor_sessions (
    session_id,
    started_unix_micros,
    ended_unix_micros,
    source_name,
    initial_cv_language,
    plugin_version,
    end_reason
)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);
