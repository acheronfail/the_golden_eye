SELECT
    run_id,
    completed_unix_micros,
    level_number,
    difficulty_number,
    status,
    time_seconds
FROM runs
WHERE (?1 IS NULL OR completed_unix_micros >= ?1)
  AND completed_unix_micros < ?2
ORDER BY completed_unix_micros, run_id;
