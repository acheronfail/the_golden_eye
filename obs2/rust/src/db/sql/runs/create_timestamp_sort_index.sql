CREATE INDEX IF NOT EXISTS runs_timestamp_sort_idx
ON runs(completed_unix_micros DESC, run_id DESC);
