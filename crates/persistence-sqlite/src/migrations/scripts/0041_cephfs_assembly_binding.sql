ALTER TABLE ceph_fs_derived_lineage
ADD COLUMN namespace_assembly_sha256 TEXT NOT NULL DEFAULT
    '0000000000000000000000000000000000000000000000000000000000000000'
CHECK (
    length(namespace_assembly_sha256) = 64
    AND namespace_assembly_sha256 NOT GLOB '*[^0-9a-f]*'
);

ALTER TABLE ceph_fs_derived_lineage
ADD COLUMN source_capability TEXT NOT NULL DEFAULT 'metadata-only'
CHECK (source_capability IN ('metadata-only', 'metadata-browseable', 'bounded-preview'));
