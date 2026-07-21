INSERT INTO clips (
    path,
    size_bytes,
    modified_unix,
    duration_secs,
    metadata_json
)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(path) DO UPDATE SET
    size_bytes = excluded.size_bytes,
    modified_unix = excluded.modified_unix,
    duration_secs = excluded.duration_secs,
    metadata_json = excluded.metadata_json;
