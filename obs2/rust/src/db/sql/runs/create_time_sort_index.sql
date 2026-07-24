CREATE INDEX IF NOT EXISTS runs_time_sort_idx
ON runs(time_seconds IS NULL, time_seconds, completed_unix_micros DESC, run_id DESC);
