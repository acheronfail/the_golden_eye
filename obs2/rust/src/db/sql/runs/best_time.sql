SELECT MIN(time_seconds)
FROM runs
WHERE status = 'complete'
  AND level_number = ?1
  AND difficulty_number = ?2
  AND time_seconds IS NOT NULL;
