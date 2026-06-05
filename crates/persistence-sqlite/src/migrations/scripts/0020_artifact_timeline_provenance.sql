ALTER TABLE artifacts ADD COLUMN extractor_id TEXT;
ALTER TABLE artifacts ADD COLUMN extractor_version TEXT;
ALTER TABLE artifacts ADD COLUMN confidence REAL;
ALTER TABLE artifacts ADD COLUMN source_attribution TEXT;

ALTER TABLE timeline_events ADD COLUMN parser_id TEXT;
ALTER TABLE timeline_events ADD COLUMN parser_version TEXT;
ALTER TABLE timeline_events ADD COLUMN confidence REAL;
ALTER TABLE timeline_events ADD COLUMN source_attribution TEXT;
