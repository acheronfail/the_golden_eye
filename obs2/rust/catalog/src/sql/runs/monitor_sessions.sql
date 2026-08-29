SELECT
    session_id,
    started_unix_micros,
    ended_unix_micros,
    source_name,
    initial_cv_language,
    plugin_version,
    end_reason
FROM monitor_sessions
WHERE EXISTS (
    SELECT 1
    FROM run_sessions
    WHERE run_sessions.session_id = monitor_sessions.session_id
)
ORDER BY started_unix_micros DESC, session_id DESC;
