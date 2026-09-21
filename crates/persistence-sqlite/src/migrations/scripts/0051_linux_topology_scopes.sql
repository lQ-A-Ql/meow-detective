CREATE TABLE IF NOT EXISTS linux_topology_scopes (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN (
        'physical_host', 'pve', 'ceph', 'virtual_machine', 'os_instance', 'kubernetes'
    )),
    name TEXT NOT NULL,
    identity_state TEXT NOT NULL DEFAULT 'unproven' CHECK (identity_state IN (
        'unproven', 'candidate', 'proven', 'conflicted'
    )),
    identity_fingerprint TEXT,
    status TEXT NOT NULL DEFAULT 'discovered' CHECK (status IN (
        'discovered', 'partial', 'ready', 'failed'
    )),
    evidence_completeness TEXT NOT NULL DEFAULT 'indeterminate' CHECK (evidence_completeness IN (
        'indeterminate', 'partial', 'complete'
    )),
    diagnostics_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_linux_topology_scopes_case_kind
ON linux_topology_scopes(case_id, scope_kind, status);

CREATE TABLE IF NOT EXISTS linux_topology_memberships (
    scope_id TEXT NOT NULL REFERENCES linux_topology_scopes(id) ON DELETE CASCADE,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    member_index INTEGER,
    confidence TEXT NOT NULL DEFAULT 'candidate' CHECK (confidence IN (
        'candidate', 'corroborated', 'proven', 'conflicted'
    )),
    provenance_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (scope_id, data_source_id, role),
    CHECK (member_index IS NULL OR member_index >= 0)
);

CREATE INDEX IF NOT EXISTS idx_linux_topology_memberships_source
ON linux_topology_memberships(data_source_id, scope_id);

CREATE TABLE IF NOT EXISTS linux_topology_edges (
    source_scope_id TEXT NOT NULL REFERENCES linux_topology_scopes(id) ON DELETE CASCADE,
    target_scope_id TEXT NOT NULL REFERENCES linux_topology_scopes(id) ON DELETE CASCADE,
    edge_kind TEXT NOT NULL CHECK (edge_kind IN (
        'hosts', 'provides_storage', 'boots', 'runs', 'manages',
        'consumes_storage', 'derived_from'
    )),
    confidence TEXT NOT NULL DEFAULT 'candidate' CHECK (confidence IN (
        'candidate', 'corroborated', 'proven', 'conflicted'
    )),
    provenance_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (source_scope_id, target_scope_id, edge_kind),
    CHECK (source_scope_id <> target_scope_id)
);

CREATE INDEX IF NOT EXISTS idx_linux_topology_edges_target
ON linux_topology_edges(target_scope_id, edge_kind);

CREATE TABLE IF NOT EXISTS linux_topology_artifacts (
    scope_id TEXT NOT NULL REFERENCES linux_topology_scopes(id) ON DELETE CASCADE,
    data_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    file_id TEXT,
    layer TEXT NOT NULL CHECK (layer IN (
        'evidence', 'storage', 'os', 'orchestration', 'workload'
    )),
    artifact_kind TEXT NOT NULL,
    parser TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN (
        'candidate_found', 'parsed', 'partial', 'failed', 'not_found'
    )),
    diagnostics_json TEXT NOT NULL DEFAULT '[]',
    content_digest TEXT,
    PRIMARY KEY (scope_id, data_source_id, artifact_kind, file_id),
    FOREIGN KEY (data_source_id) REFERENCES data_sources(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_linux_topology_artifacts_source_layer
ON linux_topology_artifacts(data_source_id, layer, status);
