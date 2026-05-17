CREATE TABLE timeline_events (
    id TEXT PRIMARY KEY NOT NULL,
    case_id TEXT NOT NULL REFERENCES cases(id),
    source_object_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    ts TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    attrs TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX idx_timeline_case_ts ON timeline_events(case_id, ts);
CREATE INDEX idx_timeline_type ON timeline_events(event_type);
CREATE INDEX idx_timeline_source ON timeline_events(source_object_id);
