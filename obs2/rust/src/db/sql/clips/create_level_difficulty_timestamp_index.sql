CREATE INDEX IF NOT EXISTS runs_level_difficulty_timestamp_idx
ON runs(level_number, difficulty, completed_unix_micros DESC);
