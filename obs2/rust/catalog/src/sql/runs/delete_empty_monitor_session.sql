DELETE FROM monitor_sessions
WHERE session_id = ?1
  AND NOT EXISTS (
      SELECT 1
      FROM run_sessions
      WHERE session_id = ?1
  );
