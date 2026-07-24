SELECT
    r.run_id,
    r.completed_unix_micros,
    r.level_number,
    r.difficulty_number,
    r.status,
    r.time_seconds
FROM runs r
INNER JOIN run_sessions rs ON rs.run_id = r.run_id
WHERE rs.session_id = ?1
ORDER BY r.completed_unix_micros, r.run_id;
