CREATE TABLE storage_objects (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    object_kind TEXT NOT NULL CHECK (object_kind IN (
        'partition', 'lvm_physical_volume', 'volume_group', 'logical_volume',
        'virtual_disk', 'ceph_cluster', 'ceph_osd', 'ceph_rbd', 'ceph_fs', 'file_system'
    )),
    name TEXT NOT NULL,
    identity_state TEXT NOT NULL DEFAULT 'unproven' CHECK (identity_state IN ('unproven', 'candidate', 'proven', 'conflicted')),
    status TEXT NOT NULL DEFAULT 'discovered' CHECK (status IN ('discovered', 'partial', 'ready', 'failed')),
    provenance_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_storage_objects_case_kind ON storage_objects(case_id, object_kind, status);

CREATE TABLE storage_object_memberships (
    storage_object_id TEXT NOT NULL REFERENCES storage_objects(id) ON DELETE CASCADE,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    confidence TEXT NOT NULL DEFAULT 'candidate' CHECK (confidence IN ('candidate', 'corroborated', 'proven', 'conflicted')),
    provenance_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (storage_object_id, data_source_id, role)
);

CREATE TABLE storage_relations (
    source_object_id TEXT NOT NULL REFERENCES storage_objects(id) ON DELETE CASCADE,
    target_object_id TEXT NOT NULL REFERENCES storage_objects(id) ON DELETE CASCADE,
    relation_kind TEXT NOT NULL CHECK (relation_kind IN ('contains', 'materializes_as', 'mounted_from', 'derived_from', 'provides_storage')),
    confidence TEXT NOT NULL DEFAULT 'candidate' CHECK (confidence IN ('candidate', 'corroborated', 'proven', 'conflicted')),
    provenance_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (source_object_id, target_object_id, relation_kind),
    CHECK (source_object_id <> target_object_id)
);

CREATE INDEX idx_storage_relations_target ON storage_relations(target_object_id, relation_kind);

ALTER TABLE ceph_rbd_derived_lineage ADD COLUMN parent_storage_object_id TEXT NOT NULL DEFAULT '' REFERENCES storage_objects(id) ON DELETE RESTRICT;
ALTER TABLE ceph_fs_derived_lineage ADD COLUMN parent_storage_object_id TEXT NOT NULL DEFAULT '' REFERENCES storage_objects(id) ON DELETE RESTRICT;
