CREATE INDEX IF NOT EXISTS clips_status_timestamp_idx ON clips (
    json_extract(metadata_json, '$.status'),
    json_extract(metadata_json, '$.timestamp') DESC
);
