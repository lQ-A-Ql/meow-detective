CREATE TABLE IF NOT EXISTS ceph_fs_namespace_manifests (
    filesystem_identity TEXT NOT NULL,
    data_source_id TEXT NOT NULL
        REFERENCES data_sources(id) ON DELETE CASCADE,
    filesystem_id INTEGER NOT NULL CHECK (filesystem_id >= 0),
    fsmap_epoch INTEGER NOT NULL CHECK (fsmap_epoch BETWEEN 1 AND 4294967295),
    root_inode INTEGER NOT NULL CHECK (root_inode > 0),
    input_sha256 TEXT NOT NULL CHECK (
        length(input_sha256) = 64
        AND input_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    projection_sha256 TEXT NOT NULL CHECK (
        length(projection_sha256) = 64
        AND projection_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    decoder_profile TEXT NOT NULL CHECK (decoder_profile = 'cephfs-namespace-v1'),
    completeness TEXT NOT NULL CHECK (completeness IN ('closed', 'incomplete')),
    published INTEGER NOT NULL CHECK (published IN (0, 1)),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0),
    inode_count INTEGER NOT NULL CHECK (inode_count >= 0),
    diagnostic_count INTEGER NOT NULL CHECK (diagnostic_count >= 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (filesystem_identity, data_source_id),
    CHECK ((completeness = 'closed' AND published = 1)
        OR (completeness = 'incomplete' AND published = 0))
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_namespace_manifests_source
ON ceph_fs_namespace_manifests(data_source_id, filesystem_identity, fsmap_epoch);

CREATE TABLE IF NOT EXISTS ceph_fs_inodes (
    filesystem_identity TEXT NOT NULL,
    data_source_id TEXT NOT NULL,
    inode INTEGER NOT NULL CHECK (inode > 0),
    mode INTEGER NOT NULL CHECK (mode >= 0),
    uid INTEGER NOT NULL CHECK (uid >= 0),
    gid INTEGER NOT NULL CHECK (gid >= 0),
    nlink INTEGER NOT NULL,
    size INTEGER NOT NULL CHECK (size >= 0),
    inode_kind TEXT NOT NULL CHECK (
        inode_kind IN ('file', 'directory', 'symlink', 'other')
    ),
    encoded_version INTEGER NOT NULL CHECK (encoded_version BETWEEN 1 AND 255),
    remaining_inode_bytes INTEGER NOT NULL CHECK (remaining_inode_bytes >= 0),
    PRIMARY KEY (filesystem_identity, data_source_id, inode),
    FOREIGN KEY (filesystem_identity, data_source_id)
        REFERENCES ceph_fs_namespace_manifests(filesystem_identity, data_source_id)
        ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS ceph_fs_file_layouts (
    filesystem_identity TEXT NOT NULL,
    data_source_id TEXT NOT NULL,
    inode INTEGER NOT NULL CHECK (inode > 0),
    stripe_unit INTEGER NOT NULL CHECK (stripe_unit >= 0),
    stripe_count INTEGER NOT NULL CHECK (stripe_count >= 0),
    object_size INTEGER NOT NULL CHECK (object_size >= 0),
    pool_id INTEGER NOT NULL CHECK (pool_id >= -1),
    pool_namespace TEXT NOT NULL,
    inline_data BLOB,
    PRIMARY KEY (filesystem_identity, data_source_id, inode),
    FOREIGN KEY (filesystem_identity, data_source_id, inode)
        REFERENCES ceph_fs_inodes(filesystem_identity, data_source_id, inode)
        ON DELETE CASCADE,
    CHECK (inline_data IS NULL OR length(inline_data) <= 65536),
    CHECK ((stripe_unit = 0 AND stripe_count = 0 AND object_size = 0
            AND pool_id = -1 AND pool_namespace = '')
        OR (stripe_unit > 0 AND stripe_count > 0 AND object_size >= stripe_unit))
);

CREATE TABLE IF NOT EXISTS ceph_fs_sparse_extents (
    filesystem_identity TEXT NOT NULL,
    data_source_id TEXT NOT NULL,
    inode INTEGER NOT NULL CHECK (inode > 0),
    offset INTEGER NOT NULL CHECK (offset >= 0),
    length INTEGER NOT NULL CHECK (length > 0),
    proof_sha256 TEXT NOT NULL CHECK (
        length(proof_sha256) = 64
        AND proof_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    PRIMARY KEY (filesystem_identity, data_source_id, inode, offset),
    FOREIGN KEY (filesystem_identity, data_source_id, inode)
        REFERENCES ceph_fs_file_layouts(filesystem_identity, data_source_id, inode)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_sparse_extents_inode
ON ceph_fs_sparse_extents(data_source_id, filesystem_identity, inode, offset);

CREATE TABLE IF NOT EXISTS ceph_fs_dentries (
    filesystem_identity TEXT NOT NULL,
    data_source_id TEXT NOT NULL,
    entry_id TEXT NOT NULL,
    parent_entry_id TEXT,
    parent_inode INTEGER NOT NULL CHECK (parent_inode >= 0),
    child_inode INTEGER NOT NULL CHECK (child_inode > 0),
    fragment INTEGER NOT NULL CHECK (fragment >= 0),
    name TEXT NOT NULL CHECK (length(name) > 0 AND instr(name, char(0)) = 0),
    path TEXT NOT NULL CHECK (length(path) > 0 AND instr(path, char(0)) = 0),
    entry_kind TEXT NOT NULL CHECK (
        entry_kind IN ('file', 'directory', 'symlink', 'remote', 'other')
    ),
    mode INTEGER,
    uid INTEGER,
    gid INTEGER,
    nlink INTEGER,
    size INTEGER CHECK (size IS NULL OR size >= 0),
    alternate_name TEXT NOT NULL,
    PRIMARY KEY (filesystem_identity, data_source_id, entry_id),
    FOREIGN KEY (filesystem_identity, data_source_id)
        REFERENCES ceph_fs_namespace_manifests(filesystem_identity, data_source_id)
        ON DELETE CASCADE,
    CHECK ((parent_entry_id IS NULL AND parent_inode = 0)
        OR (parent_entry_id IS NOT NULL AND parent_inode > 0)),
    UNIQUE (filesystem_identity, data_source_id, parent_inode, name)
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_dentries_parent
ON ceph_fs_dentries(data_source_id, filesystem_identity, parent_entry_id, path);

CREATE TABLE IF NOT EXISTS ceph_fs_namespace_diagnostics (
    filesystem_identity TEXT NOT NULL,
    data_source_id TEXT NOT NULL,
    diagnostic_ordinal INTEGER NOT NULL CHECK (diagnostic_ordinal >= 0),
    diagnostic_kind TEXT NOT NULL CHECK (
        diagnostic_kind IN ('snapshot_skipped', 'duplicate', 'orphan', 'cycle')
    ),
    parent_inode INTEGER NOT NULL CHECK (parent_inode >= 0),
    child_inode INTEGER NOT NULL CHECK (child_inode >= 0),
    name TEXT NOT NULL,
    snap_id INTEGER CHECK (snap_id IS NULL OR snap_id >= 0),
    PRIMARY KEY (filesystem_identity, data_source_id, diagnostic_ordinal),
    FOREIGN KEY (filesystem_identity, data_source_id)
        REFERENCES ceph_fs_namespace_manifests(filesystem_identity, data_source_id)
        ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ceph_fs_namespace_diagnostics_kind
ON ceph_fs_namespace_diagnostics(data_source_id, filesystem_identity, diagnostic_kind);
