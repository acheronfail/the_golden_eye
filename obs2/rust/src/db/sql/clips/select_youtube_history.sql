SELECT
    path,
    youtube_json
FROM clips
WHERE youtube_json IS NOT NULL
ORDER BY
    json_extract(youtube_json, '$.uploadedAt') ASC,
    json_extract(youtube_json, '$.videoId') ASC;
