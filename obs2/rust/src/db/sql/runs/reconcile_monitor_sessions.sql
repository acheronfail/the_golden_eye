UPDATE monitor_sessions
SET end_reason = 'interrupted'
WHERE ended_unix_micros IS NULL
  AND end_reason IS NULL;
