CREATE TABLE storage_objects_new (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    object_kind TEXT NOT NULL CHECK (object_kind IN (
        'partition', 'lvm_physical_volume', 'volume_group', 'logical_volume', 'md_raid',
        'virtual_disk', 'ceph_cluster', 'ceph_osd', 'ceph_rbd', 'ceph_fs', 'file_system'
    )),
    name TEXT NOT NULL,
    identity_state TEXT NOT NULL DEFAULT 'unproven' CHECK (identity_state IN ('unproven', 'candidate', 'proven', 'conflicted')),
    status TEXT NOT NULL DEFAULT 'discovered' CHECK (status IN ('discovered', 'partial', 'ready', 'failed')),
    provenance_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO storage_objects_new SELECT * FROM storage_objects;
DROP TABLE storage_objects;
ALTER TABLE storage_objects_new RENAME TO storage_objects;
CREATE INDEX idx_storage_objects_case_kind ON storage_objects(case_id, object_kind, status);

CREATE TABLE storage_object_versions (
    id TEXT PRIMARY KEY NOT NULL,
    storage_object_id TEXT NOT NULL REFERENCES storage_objects(id) ON DELETE CASCADE,
    derived_source_id TEXT NOT NULL UNIQUE REFERENCES data_sources(id) ON DELETE CASCADE,
    adapter_version TEXT NOT NULL,
    mapping_sha256 TEXT NOT NULL CHECK (length(mapping_sha256) = 64),
    native_versions_json TEXT NOT NULL CHECK (json_valid(native_versions_json)),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TRIGGER storage_object_versions_immutable BEFORE UPDATE ON storage_object_versions
BEGIN SELECT RAISE(ABORT, 'storage object versions are immutable'); END;
CREATE TRIGGER storage_object_version_retirement AFTER DELETE ON storage_object_versions
BEGIN
    DELETE FROM storage_objects WHERE id=OLD.storage_object_id
      AND NOT EXISTS(SELECT 1 FROM storage_object_versions WHERE storage_object_id=OLD.storage_object_id);
END;
