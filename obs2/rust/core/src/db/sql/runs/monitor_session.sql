SELECT
    session_id,
    started_unix_micros,
    ended_unix_micros,
    source_name,
    initial_cv_language,
    plugin_version,
    end_reason
FROM monitor_sessions
WHERE session_id = ?1;
