CREATE TABLE structure_view_attempts (
    derived_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    attempt_id TEXT NOT NULL,
    input_fingerprint TEXT NOT NULL CHECK (
        length(input_fingerprint) = 64
        AND input_fingerprint NOT GLOB '*[^0-9a-f]*'
    ),
    owner_id TEXT NOT NULL,
    generation TEXT NOT NULL,
    receipt TEXT NOT NULL CHECK (
        length(receipt) = 64
        AND receipt NOT GLOB '*[^0-9a-f]*'
    ),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (derived_source_id, attempt_id)
);

CREATE INDEX idx_structure_view_attempts_current
    ON structure_view_attempts(derived_source_id, created_at);

CREATE TRIGGER structure_view_attempts_immutable
BEFORE UPDATE ON structure_view_attempts
BEGIN
    SELECT RAISE(ABORT, 'structure view attempts are immutable');
END;
