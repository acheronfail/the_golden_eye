SELECT path
FROM clips
WHERE json_extract(metadata_json, '$.status') IN ('failed', 'abort', 'kia')
ORDER BY
    json_extract(metadata_json, '$.timestamp') DESC,
    path DESC;
