UPDATE monitor_sessions
SET ended_unix_micros = ?1,
    end_reason = ?2
WHERE session_id = ?3
  AND end_reason IS NULL;
