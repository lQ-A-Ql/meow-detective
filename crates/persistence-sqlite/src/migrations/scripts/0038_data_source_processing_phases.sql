CREATE TABLE IF NOT EXISTS data_source_processing_phases (
    data_source_id TEXT NOT NULL
        REFERENCES data_sources(id) ON DELETE CASCADE
        CHECK (
            length(trim(data_source_id)) > 0
            AND instr(data_source_id, char(0)) = 0
        ),
    phase TEXT NOT NULL
        CHECK (
            phase IN (
                'catalog',
                'graph',
                'platform',
                'artifacts',
                'timeline',
                'search'
            )
        ),
    state TEXT NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending', 'running', 'ready', 'failed', 'deferred')),
    version INTEGER NOT NULL CHECK (version BETWEEN 1 AND 4294967295),
    input_fingerprint TEXT NOT NULL
        CHECK (
            length(input_fingerprint) = 64
            AND input_fingerprint NOT GLOB '*[^0-9a-f]*'
        ),
    owner_id TEXT
        CHECK (
            owner_id IS NULL
            OR (
                length(trim(owner_id)) > 0
                AND instr(owner_id, char(0)) = 0
            )
        ),
    attempt_id TEXT
        CHECK (
            attempt_id IS NULL
            OR (
                length(trim(attempt_id)) > 0
                AND instr(attempt_id, char(0)) = 0
            )
        ),
    stats_json TEXT NOT NULL DEFAULT '{}'
        CHECK (json_valid(stats_json) AND json_type(stats_json) = 'object'),
    last_error TEXT
        CHECK (
            last_error IS NULL
            OR (
                length(trim(last_error)) > 0
                AND instr(last_error, char(0)) = 0
            )
        ),
    started_at TEXT
        CHECK (
            started_at IS NULL
            OR (
                length(trim(started_at)) > 0
                AND instr(started_at, char(0)) = 0
            )
        ),
    completed_at TEXT
        CHECK (
            completed_at IS NULL
            OR (
                length(trim(completed_at)) > 0
                AND instr(completed_at, char(0)) = 0
            )
        ),
    heartbeat_at TEXT
        CHECK (
            heartbeat_at IS NULL
            OR (
                length(trim(heartbeat_at)) > 0
                AND instr(heartbeat_at, char(0)) = 0
            )
        ),
    lease_expires_at TEXT
        CHECK (
            lease_expires_at IS NULL
            OR (
                length(trim(lease_expires_at)) > 0
                AND instr(lease_expires_at, char(0)) = 0
            )
        ),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        CHECK (
            length(trim(updated_at)) > 0
            AND instr(updated_at, char(0)) = 0
        ),
    PRIMARY KEY (data_source_id, phase),
    CHECK (
        (
            state = 'pending'
            AND owner_id IS NULL
            AND attempt_id IS NULL
            AND started_at IS NULL
            AND completed_at IS NULL
            AND heartbeat_at IS NULL
            AND lease_expires_at IS NULL
            AND last_error IS NULL
        )
        OR (
            state = 'running'
            AND owner_id IS NOT NULL
            AND attempt_id IS NOT NULL
            AND started_at IS NOT NULL
            AND completed_at IS NULL
            AND heartbeat_at IS NOT NULL
            AND lease_expires_at IS NOT NULL
            AND last_error IS NULL
        )
        OR (
            state = 'ready'
            AND owner_id IS NOT NULL
            AND attempt_id IS NOT NULL
            AND started_at IS NOT NULL
            AND completed_at IS NOT NULL
            AND heartbeat_at IS NOT NULL
            AND lease_expires_at IS NULL
            AND last_error IS NULL
        )
        OR (
            state = 'failed'
            AND owner_id IS NOT NULL
            AND attempt_id IS NOT NULL
            AND started_at IS NOT NULL
            AND completed_at IS NOT NULL
            AND heartbeat_at IS NOT NULL
            AND lease_expires_at IS NULL
            AND last_error IS NOT NULL
        )
        OR (
            state = 'deferred'
            AND owner_id IS NOT NULL
            AND attempt_id IS NOT NULL
            AND completed_at IS NOT NULL
            AND heartbeat_at IS NOT NULL
            AND lease_expires_at IS NULL
        )
    )
);

CREATE INDEX IF NOT EXISTS idx_data_source_processing_phases_state
ON data_source_processing_phases(state, phase, data_source_id);
