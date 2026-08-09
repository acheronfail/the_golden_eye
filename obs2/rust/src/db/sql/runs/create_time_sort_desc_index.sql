CREATE INDEX IF NOT EXISTS runs_time_sort_desc_idx
ON runs(time_seconds IS NULL, time_seconds DESC, completed_unix_micros DESC, run_id DESC);
