-- SQLite remains the control plane for cross-storage operations. Fjall records
-- and artifact bytes stay outside this database; these rows only track intent,
-- publication, references, and repair state.
CREATE TABLE storage_operations (
    operation_id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    source_id TEXT REFERENCES data_sources(id) ON DELETE CASCADE,
    attempt_id TEXT NOT NULL,
    operation_kind TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('planned', 'running', 'committed', 'published', 'failed', 'cancelled')),
    input_digest TEXT,
    expected_batch_digest TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_error_code TEXT
);

CREATE INDEX idx_storage_operations_case_state
    ON storage_operations(case_id, state, updated_at);
CREATE INDEX idx_storage_operations_source_attempt
    ON storage_operations(source_id, attempt_id);

CREATE TABLE storage_manifests (
    manifest_id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    source_id TEXT REFERENCES data_sources(id) ON DELETE CASCADE,
    relative_path TEXT NOT NULL,
    format_version INTEGER NOT NULL CHECK (format_version > 0),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    manifest_digest TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('building', 'sealed', 'published', 'stale', 'repair_required')),
    sealed_at TEXT,
    published_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (case_id, relative_path)
);

CREATE INDEX idx_storage_manifests_case_state
    ON storage_manifests(case_id, state);
CREATE INDEX idx_storage_manifests_source
    ON storage_manifests(source_id, state);

CREATE TABLE artifact_objects (
    object_id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64 AND sha256 NOT GLOB '*[^0-9a-fA-F]*'),
    original_length INTEGER NOT NULL CHECK (original_length >= 0),
    stored_length INTEGER NOT NULL CHECK (stored_length >= 0),
    media_type TEXT NOT NULL,
    compression TEXT NOT NULL CHECK (compression IN ('none', 'zstd')),
    relative_path TEXT NOT NULL,
    write_state TEXT NOT NULL CHECK (write_state IN ('planned', 'writing', 'ready', 'quarantined', 'deleted', 'failed')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_error_code TEXT,
    UNIQUE (case_id, sha256)
);

CREATE INDEX idx_artifact_objects_case_state
    ON artifact_objects(case_id, write_state, updated_at);

CREATE TABLE artifact_references (
    reference_id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    object_id TEXT NOT NULL REFERENCES artifact_objects(object_id) ON DELETE RESTRICT,
    source_id TEXT REFERENCES data_sources(id) ON DELETE CASCADE,
    owner_kind TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    released_at TEXT,
    UNIQUE (case_id, object_id, owner_kind, owner_id, role)
);

CREATE INDEX idx_artifact_references_object_active
    ON artifact_references(object_id, released_at);
CREATE INDEX idx_artifact_references_owner
    ON artifact_references(case_id, owner_kind, owner_id);

CREATE TABLE storage_repair_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    source_id TEXT REFERENCES data_sources(id) ON DELETE CASCADE,
    operation_id TEXT REFERENCES storage_operations(operation_id) ON DELETE SET NULL,
    event_kind TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('started', 'completed', 'failed', 'skipped')),
    details_json TEXT,
    error_code TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE INDEX idx_storage_repair_events_case_created
    ON storage_repair_events(case_id, created_at);
CREATE INDEX idx_storage_repair_events_operation
    ON storage_repair_events(operation_id, state);
