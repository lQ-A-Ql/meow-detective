CREATE TABLE storage_object_versions_new (
    id TEXT PRIMARY KEY NOT NULL,
    storage_object_id TEXT NOT NULL REFERENCES storage_objects(id) ON DELETE CASCADE,
    native_object_id_json TEXT NOT NULL CHECK (json_valid(native_object_id_json)),
    owner_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    derived_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    adapter_version TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    mapping_sha256 TEXT NOT NULL CHECK (length(mapping_sha256) = 64),
    acquisition_ids_json TEXT NOT NULL CHECK (json_valid(acquisition_ids_json)),
    parent_source_revisions_json TEXT NOT NULL CHECK (json_valid(parent_source_revisions_json)),
    native_versions_json TEXT NOT NULL CHECK (json_valid(native_versions_json)),
    consistency TEXT NOT NULL CHECK (consistency IN (
        'coherent', 'historical', 'partial', 'conflicted', 'indeterminate'
    )),
    version_kind TEXT NOT NULL CHECK (version_kind IN (
        'derived_output', 'native_observation'
    )),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO storage_object_versions_new (
    id,
    storage_object_id,
    native_object_id_json,
    owner_source_id,
    derived_source_id,
    adapter_version,
    policy_version,
    mapping_sha256,
    acquisition_ids_json,
    parent_source_revisions_json,
    native_versions_json,
    consistency,
    version_kind,
    created_at
)
SELECT
    id,
    storage_object_id,
    json_object(
        'adapter', 'legacy',
        'identitySpace', 'legacy',
        'nativeId', storage_object_id,
        'scope', derived_source_id
    ),
    derived_source_id,
    derived_source_id,
    adapter_version,
    'legacy-v1',
    mapping_sha256,
    json_array(derived_source_id),
    '[]',
    native_versions_json,
    'indeterminate',
    'derived_output',
    created_at
FROM storage_object_versions;

DROP TRIGGER IF EXISTS storage_object_versions_immutable;
DROP TRIGGER IF EXISTS storage_object_version_retirement;
DROP TABLE storage_object_versions;
ALTER TABLE storage_object_versions_new RENAME TO storage_object_versions;

CREATE INDEX idx_storage_object_versions_object
    ON storage_object_versions(storage_object_id, created_at);
CREATE INDEX idx_storage_object_versions_owner
    ON storage_object_versions(owner_source_id, version_kind);

CREATE TRIGGER storage_object_versions_immutable
BEFORE UPDATE ON storage_object_versions
BEGIN
    SELECT RAISE(ABORT, 'storage object versions are immutable');
END;

CREATE TRIGGER storage_object_version_retirement
AFTER DELETE ON storage_object_versions
BEGIN
    DELETE FROM storage_objects
    WHERE id = OLD.storage_object_id
      AND NOT EXISTS (
          SELECT 1
          FROM storage_object_versions
          WHERE storage_object_id = OLD.storage_object_id
      );
END;

CREATE TABLE source_revisions_new (
    source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK (revision > 0),
    content_json TEXT NOT NULL CHECK (json_valid(content_json)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (source_id, revision)
);

INSERT INTO source_revisions_new (
    source_id,
    revision,
    content_json,
    created_at
)
SELECT source_id, revision, content_json, created_at
FROM source_revisions;

DROP TRIGGER IF EXISTS source_revisions_immutable;
DROP TABLE source_revisions;
ALTER TABLE source_revisions_new RENAME TO source_revisions;

CREATE TRIGGER source_revisions_immutable
BEFORE UPDATE ON source_revisions
BEGIN
    SELECT RAISE(ABORT, 'source revisions are immutable');
END;

CREATE TABLE structure_observations (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    source_id TEXT NOT NULL,
    source_revision INTEGER NOT NULL,
    native_identity TEXT NOT NULL,
    native_version_json TEXT NOT NULL CHECK (json_valid(native_version_json)),
    location_kind TEXT NOT NULL CHECK (location_kind IN ('source_window', 'unavailable')),
    byte_offset INTEGER,
    byte_length INTEGER,
    gap_reason TEXT,
    parser_version TEXT NOT NULL,
    native_checksum_verified INTEGER NOT NULL CHECK (native_checksum_verified IN (0, 1)),
    detail_key TEXT NOT NULL,
    detail_sha256 TEXT NOT NULL CHECK (
        length(detail_sha256) = 64
        AND detail_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    attempt_id TEXT NOT NULL,
    generation TEXT NOT NULL,
    receipt TEXT NOT NULL CHECK (
        length(receipt) = 64
        AND receipt NOT GLOB '*[^0-9a-f]*'
    ),
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'published')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (source_id, source_revision)
        REFERENCES source_revisions(source_id, revision) ON DELETE CASCADE,
    CHECK (
        (
            location_kind = 'source_window'
            AND byte_offset IS NOT NULL
            AND byte_length IS NOT NULL
            AND byte_length > 0
            AND gap_reason IS NULL
        )
        OR (
            location_kind = 'unavailable'
            AND byte_offset IS NULL
            AND byte_length IS NULL
            AND gap_reason IS NOT NULL
        )
    )
);

CREATE INDEX idx_structure_observations_revision
    ON structure_observations(source_id, source_revision, state);
CREATE INDEX idx_structure_observations_attempt
    ON structure_observations(attempt_id, generation, state);

CREATE TRIGGER structure_observations_immutable
BEFORE UPDATE ON structure_observations
WHEN NEW.id <> OLD.id
  OR NEW.case_id <> OLD.case_id
  OR NEW.source_id <> OLD.source_id
  OR NEW.source_revision <> OLD.source_revision
  OR NEW.native_identity <> OLD.native_identity
  OR NEW.native_version_json <> OLD.native_version_json
  OR NEW.location_kind <> OLD.location_kind
  OR COALESCE(NEW.byte_offset, -1) <> COALESCE(OLD.byte_offset, -1)
  OR COALESCE(NEW.byte_length, -1) <> COALESCE(OLD.byte_length, -1)
  OR COALESCE(NEW.gap_reason, '') <> COALESCE(OLD.gap_reason, '')
  OR NEW.parser_version <> OLD.parser_version
  OR NEW.native_checksum_verified <> OLD.native_checksum_verified
  OR NEW.detail_key <> OLD.detail_key
  OR NEW.detail_sha256 <> OLD.detail_sha256
  OR NEW.attempt_id <> OLD.attempt_id
  OR NEW.generation <> OLD.generation
  OR NEW.receipt <> OLD.receipt
BEGIN
    SELECT RAISE(ABORT, 'structure observations are immutable');
END;

CREATE TRIGGER structure_observations_publication_transition
BEFORE UPDATE OF state ON structure_observations
WHEN OLD.state <> 'pending' OR NEW.state <> 'published'
BEGIN
    SELECT RAISE(ABORT, 'structure observation publication transition is invalid');
END;
