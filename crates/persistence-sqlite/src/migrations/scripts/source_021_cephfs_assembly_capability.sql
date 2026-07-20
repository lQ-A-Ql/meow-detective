ALTER TABLE ceph_fs_sparse_extents
ADD COLUMN evidence_sha256 TEXT NOT NULL DEFAULT
    '0000000000000000000000000000000000000000000000000000000000000000'
CHECK (
    length(evidence_sha256) = 64
    AND evidence_sha256 NOT GLOB '*[^0-9a-f]*'
);

CREATE TABLE IF NOT EXISTS ceph_fs_namespace_assemblies (
    filesystem_identity TEXT NOT NULL,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    assembly_sha256 TEXT NOT NULL CHECK (
        length(assembly_sha256) = 64
        AND assembly_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    assembly_version INTEGER NOT NULL CHECK (assembly_version > 0),
    complete INTEGER NOT NULL CHECK (complete IN (0, 1)),
    frozen INTEGER NOT NULL CHECK (frozen IN (0, 1)),
    freeze_reasons_json TEXT NOT NULL CHECK (length(freeze_reasons_json) > 1),
    mutation_state TEXT NOT NULL CHECK (mutation_state IN ('complete', 'unknown')),
    mutation_digest TEXT CHECK (
        mutation_digest IS NULL
        OR (
            length(mutation_digest) = 64
            AND mutation_digest NOT GLOB '*[^0-9a-f]*'
        )
    ),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (filesystem_identity, data_source_id),
    FOREIGN KEY (filesystem_identity, data_source_id)
        REFERENCES ceph_fs_namespace_manifests(filesystem_identity, data_source_id)
        ON DELETE CASCADE,
    CHECK (complete <> frozen),
    CHECK ((complete = 1 AND freeze_reasons_json = '[]')
        OR (complete = 0 AND freeze_reasons_json <> '[]')),
    CHECK ((mutation_state = 'complete' AND mutation_digest IS NULL)
        OR (mutation_state = 'unknown' AND mutation_digest IS NOT NULL))
);

CREATE TABLE IF NOT EXISTS ceph_fs_source_capabilities (
    filesystem_identity TEXT NOT NULL,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    capability TEXT NOT NULL CHECK (
        capability IN ('metadata-only', 'metadata-browseable', 'bounded-preview')
    ),
    lineage_fingerprint TEXT NOT NULL CHECK (
        length(lineage_fingerprint) = 64
        AND lineage_fingerprint NOT GLOB '*[^0-9a-f]*'
    ),
    assembly_sha256 TEXT NOT NULL CHECK (
        length(assembly_sha256) = 64
        AND assembly_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    namespace_projection_sha256 TEXT NOT NULL CHECK (
        length(namespace_projection_sha256) = 64
        AND namespace_projection_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    decoder_profile TEXT NOT NULL CHECK (decoder_profile = 'cephfs-namespace-v1'),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (filesystem_identity, data_source_id),
    FOREIGN KEY (filesystem_identity, data_source_id)
        REFERENCES ceph_fs_namespace_manifests(filesystem_identity, data_source_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_source_capabilities_source
ON ceph_fs_source_capabilities(data_source_id, capability);
