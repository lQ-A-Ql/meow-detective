CREATE TABLE provenance_assertions (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    subject_domain TEXT NOT NULL CHECK (subject_domain IN ('environment', 'storage', 'analysis')),
    subject_id TEXT NOT NULL,
    relation_kind TEXT NOT NULL,
    source_data_source_id TEXT REFERENCES data_sources(id) ON DELETE SET NULL,
    source_file_id TEXT,
    confidence TEXT NOT NULL CHECK (confidence IN ('candidate', 'corroborated', 'proven', 'conflicted')),
    basis TEXT NOT NULL,
    parser TEXT NOT NULL,
    parser_version TEXT NOT NULL,
    content_digest TEXT,
    details_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_provenance_assertions_subject ON provenance_assertions(case_id, subject_domain, subject_id);
CREATE INDEX idx_provenance_assertions_source ON provenance_assertions(source_data_source_id, source_file_id);

CREATE TABLE analysis_object_links (
    analysis_object_id TEXT NOT NULL,
    analysis_kind TEXT NOT NULL CHECK (analysis_kind IN ('file_entry', 'artifact', 'timeline_event', 'finding', 'report')),
    environment_object_id TEXT REFERENCES environment_objects(id) ON DELETE CASCADE,
    storage_object_id TEXT REFERENCES storage_objects(id) ON DELETE CASCADE,
    evidence_source_id TEXT REFERENCES data_sources(id) ON DELETE CASCADE,
    relation_kind TEXT NOT NULL CHECK (relation_kind IN ('derived_from', 'observed_on', 'stored_in', 'supported_by')),
    provenance_assertion_id TEXT REFERENCES provenance_assertions(id) ON DELETE RESTRICT,
    PRIMARY KEY (analysis_object_id, analysis_kind, relation_kind, environment_object_id, storage_object_id, evidence_source_id)
);
