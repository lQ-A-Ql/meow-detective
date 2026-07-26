CREATE INDEX IF NOT EXISTS idx_source_timeline_ts_id
ON timeline_events(ts DESC, id ASC);

CREATE INDEX IF NOT EXISTS idx_source_timeline_type_ts_id
ON timeline_events(event_type, ts DESC, id ASC);
