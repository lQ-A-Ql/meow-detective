CREATE INDEX IF NOT EXISTS idx_timeline_ts_id ON timeline_events(ts DESC, id ASC);
CREATE INDEX IF NOT EXISTS idx_timeline_case_ts_id ON timeline_events(case_id, ts DESC, id ASC);
CREATE INDEX IF NOT EXISTS idx_timeline_type_ts_id ON timeline_events(event_type, ts DESC, id ASC);
CREATE INDEX IF NOT EXISTS idx_timeline_case_type_ts_id ON timeline_events(case_id, event_type, ts DESC, id ASC);
