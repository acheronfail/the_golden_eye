CREATE INDEX IF NOT EXISTS clips_level_difficulty_timestamp_idx ON clips (
    json_extract(metadata_json, '$.level'),
    json_extract(metadata_json, '$.difficulty'),
    json_extract(metadata_json, '$.timestamp') DESC
);
