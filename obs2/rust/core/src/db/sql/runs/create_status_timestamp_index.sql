CREATE INDEX IF NOT EXISTS runs_status_timestamp_idx ON runs(status, completed_unix_micros DESC);
