SELECT
    level_number,
    difficulty_number,
    MIN(time_seconds)
FROM runs
WHERE status = 'complete'
  AND level_number BETWEEN 1 AND 20
  AND difficulty_number IN (0, 1, 2)
  AND time_seconds BETWEEN 0 AND 1023
GROUP BY level_number, difficulty_number;
