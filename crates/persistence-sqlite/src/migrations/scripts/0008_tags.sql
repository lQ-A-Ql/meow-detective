CREATE TABLE tags (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id),
    name TEXT NOT NULL,
    color TEXT
);

CREATE TABLE tag_bindings (
    tag_id TEXT NOT NULL REFERENCES tags(id),
    object_id TEXT NOT NULL,
    PRIMARY KEY (tag_id, object_id)
);

CREATE INDEX idx_tags_case ON tags(case_id);
CREATE INDEX idx_tag_bindings_object ON tag_bindings(object_id);
