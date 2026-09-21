CREATE TABLE environment_objects (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    object_kind TEXT NOT NULL CHECK (object_kind IN ('physical_host', 'pve', 'virtual_machine', 'os_instance', 'kubernetes', 'kubernetes_node', 'namespace', 'pod', 'container')),
    name TEXT NOT NULL,
    identity_state TEXT NOT NULL DEFAULT 'unproven' CHECK (identity_state IN ('unproven', 'candidate', 'proven', 'conflicted')),
    status TEXT NOT NULL DEFAULT 'discovered' CHECK (status IN ('discovered', 'partial', 'ready', 'failed')),
    provenance_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_environment_objects_case_kind ON environment_objects(case_id, object_kind, status);
CREATE TABLE environment_relations (
    source_object_id TEXT NOT NULL REFERENCES environment_objects(id) ON DELETE CASCADE,
    target_object_id TEXT NOT NULL REFERENCES environment_objects(id) ON DELETE CASCADE,
    relation_kind TEXT NOT NULL CHECK (relation_kind IN ('hosts', 'boots', 'runs', 'manages', 'contains')),
    confidence TEXT NOT NULL DEFAULT 'candidate' CHECK (confidence IN ('candidate', 'corroborated', 'proven', 'conflicted')),
    provenance_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (source_object_id, target_object_id, relation_kind),
    CHECK (source_object_id <> target_object_id)
);
