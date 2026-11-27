CREATE TABLE source_revisions (
    source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK (revision > 0),
    content_json TEXT NOT NULL CHECK (json_valid(content_json)),
    observations_json TEXT NOT NULL CHECK (json_valid(observations_json)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (source_id, revision)
);

CREATE TRIGGER source_revisions_immutable
BEFORE UPDATE ON source_revisions
BEGIN
    SELECT RAISE(ABORT, 'source revisions are immutable');
END;

CREATE TABLE structure_view_inputs (
    derived_source_id TEXT NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE,
    parent_source_id TEXT NOT NULL,
    parent_revision INTEGER NOT NULL,
    PRIMARY KEY (derived_source_id, parent_source_id),
    FOREIGN KEY (parent_source_id, parent_revision)
        REFERENCES source_revisions(source_id, revision) DEFERRABLE INITIALLY DEFERRED,
    CHECK (derived_source_id <> parent_source_id)
);

CREATE TRIGGER structure_view_inputs_immutable
BEFORE UPDATE ON structure_view_inputs
BEGIN
    SELECT RAISE(ABORT, 'structure view inputs are immutable');
END;
