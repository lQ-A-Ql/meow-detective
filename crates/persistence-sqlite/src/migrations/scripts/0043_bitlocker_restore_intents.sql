-- BitLocker key material remains exclusively in the operating system's secure
-- credential store. This table records only which verified volume may be
-- restored when the case is opened again.
CREATE TABLE bitlocker_restore_intents (
    data_source_id TEXT NOT NULL
        REFERENCES data_sources(id) ON DELETE CASCADE,
    partition_index INTEGER NOT NULL CHECK (partition_index >= 0),
    metadata_fingerprint TEXT NOT NULL CHECK (
        length(metadata_fingerprint) = 32
        AND metadata_fingerprint NOT GLOB '*[^0-9a-f]*'
    ),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    last_restore_status TEXT NOT NULL DEFAULT 'pending' CHECK (
        last_restore_status IN ('pending', 'restored', 'failed', 'disabled')
    ),
    last_error_code TEXT,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (data_source_id, partition_index)
);

CREATE INDEX idx_bitlocker_restore_intents_enabled
    ON bitlocker_restore_intents(enabled, data_source_id);
