CREATE TABLE IF NOT EXISTS filesystem_recovery_scans (
    id TEXT PRIMARY KEY NOT NULL,
    data_source_id TEXT NOT NULL,
    partition_index INTEGER NOT NULL CHECK (partition_index >= 0),
    filesystem_type TEXT NOT NULL CHECK (filesystem_type IN ('ext4', 'xfs', 'ntfs')),
    filesystem_uuid TEXT,
    parser_version TEXT NOT NULL,
    log_kind TEXT NOT NULL CHECK (log_kind IN ('internal_journal', 'internal_log')),
    snapshot_identity_sha256 TEXT NOT NULL CHECK (
        length(snapshot_identity_sha256) = 64
        AND snapshot_identity_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    state TEXT NOT NULL CHECK (state IN ('complete', 'partial', 'failed')),
    transaction_count INTEGER NOT NULL DEFAULT 0 CHECK (transaction_count >= 0),
    candidate_count INTEGER NOT NULL DEFAULT 0 CHECK (candidate_count >= 0),
    warnings_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(warnings_json)),
    started_at TEXT NOT NULL,
    completed_at TEXT NOT NULL,
    FOREIGN KEY (data_source_id) REFERENCES data_sources(id) ON DELETE CASCADE,
    UNIQUE (data_source_id, partition_index, parser_version, snapshot_identity_sha256)
);

CREATE INDEX IF NOT EXISTS idx_filesystem_recovery_scans_source
ON filesystem_recovery_scans(data_source_id, partition_index, completed_at DESC);

CREATE TABLE IF NOT EXISTS deleted_file_recoveries (
    id TEXT PRIMARY KEY NOT NULL,
    scan_id TEXT NOT NULL,
    inode TEXT NOT NULL CHECK (inode <> '' AND inode NOT GLOB '*[^0-9]*'),
    original_path TEXT,
    entry_type TEXT CHECK (entry_type IS NULL OR entry_type IN ('file', 'directory', 'symlink')),
    mode INTEGER CHECK (mode IS NULL OR (mode >= 0 AND mode <= 65535)),
    deleted_at_unix INTEGER CHECK (deleted_at_unix IS NULL OR deleted_at_unix >= 0),
    declared_size INTEGER NOT NULL CHECK (declared_size >= 0),
    recoverable_bytes INTEGER NOT NULL DEFAULT 0 CHECK (recoverable_bytes >= 0),
    completeness TEXT NOT NULL CHECK (completeness IN ('metadata_only', 'partial', 'complete')),
    recovery_method TEXT NOT NULL,
    confidence REAL NOT NULL CHECK (confidence >= 0.0 AND confidence <= 1.0),
    allocation_state TEXT NOT NULL CHECK (
        allocation_state IN ('unverified', 'free', 'allocated', 'partially_overwritten')
    ),
    transaction_id TEXT,
    log_sequence INTEGER CHECK (log_sequence IS NULL OR log_sequence >= 0),
    log_cycle INTEGER CHECK (log_cycle IS NULL OR log_cycle >= 0),
    content_sha256 TEXT CHECK (
        content_sha256 IS NULL OR (
            length(content_sha256) = 64
            AND content_sha256 NOT GLOB '*[^0-9a-f]*'
        )
    ),
    warnings_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(warnings_json)),
    FOREIGN KEY (scan_id) REFERENCES filesystem_recovery_scans(id) ON DELETE CASCADE,
    UNIQUE (scan_id, inode, recovery_method, transaction_id, log_sequence, log_cycle)
);

CREATE INDEX IF NOT EXISTS idx_deleted_file_recoveries_scan
ON deleted_file_recoveries(scan_id, inode);

CREATE TABLE IF NOT EXISTS deleted_file_recovery_ranges (
    recovery_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    range_role TEXT NOT NULL CHECK (range_role IN ('metadata', 'content')),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('filesystem', 'journal', 'log')),
    logical_offset INTEGER NOT NULL CHECK (logical_offset >= 0),
    source_offset INTEGER NOT NULL CHECK (source_offset >= 0),
    physical_offset INTEGER CHECK (physical_offset IS NULL OR physical_offset >= 0),
    length INTEGER NOT NULL CHECK (length > 0),
    allocation_state TEXT NOT NULL CHECK (
        allocation_state IN ('unverified', 'free', 'allocated', 'partially_overwritten')
    ),
    sha256 TEXT CHECK (
        sha256 IS NULL OR (
            length(sha256) = 64
            AND sha256 NOT GLOB '*[^0-9a-f]*'
        )
    ),
    PRIMARY KEY (recovery_id, ordinal),
    FOREIGN KEY (recovery_id) REFERENCES deleted_file_recoveries(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS filesystem_recovery_issues (
    scan_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error')),
    code TEXT NOT NULL,
    message TEXT NOT NULL,
    log_offset INTEGER CHECK (log_offset IS NULL OR log_offset >= 0),
    sequence INTEGER CHECK (sequence IS NULL OR sequence >= 0),
    PRIMARY KEY (scan_id, ordinal),
    FOREIGN KEY (scan_id) REFERENCES filesystem_recovery_scans(id) ON DELETE CASCADE
);
