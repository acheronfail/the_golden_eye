DELETE FROM monitor_sessions
WHERE NOT EXISTS (
    SELECT 1
    FROM run_sessions
    WHERE run_sessions.session_id = monitor_sessions.session_id
);

UPDATE monitor_sessions
SET end_reason = 'interrupted'
WHERE ended_unix_micros IS NULL
  AND end_reason IS NULL;
