CREATE TABLE IF NOT EXISTS clips (
    path TEXT PRIMARY KEY NOT NULL,
    size_bytes INTEGER NOT NULL,
    modified_unix INTEGER,
    duration_secs REAL,
    metadata_json TEXT NOT NULL CHECK (json_valid(metadata_json)),
    youtube_json TEXT CHECK (youtube_json IS NULL OR json_valid(youtube_json))
);
