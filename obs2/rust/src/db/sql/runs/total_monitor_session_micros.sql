WITH session_bounds AS (
    SELECT
        s.started_unix_micros AS started,
        CASE
            WHEN s.ended_unix_micros IS NOT NULL
            THEN s.ended_unix_micros
            WHEN s.end_reason IS NULL
            THEN ?2
            ELSE COALESCE(MAX(r.completed_unix_micros), s.started_unix_micros)
        END AS effective_end
    FROM monitor_sessions s
    LEFT JOIN run_sessions rs ON rs.session_id = s.session_id
    LEFT JOIN runs r ON r.run_id = rs.run_id
    GROUP BY s.session_id
)
SELECT CAST(
    COALESCE(
        SUM(
            CASE
                WHEN MIN(effective_end, ?2) > MAX(started, COALESCE(?1, started))
                THEN MIN(effective_end, ?2) - MAX(started, COALESCE(?1, started))
                ELSE 0
            END
        ),
        0
    ) AS INTEGER
)
FROM session_bounds;
