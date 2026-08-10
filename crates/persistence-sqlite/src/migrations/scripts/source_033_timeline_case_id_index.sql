-- Covering index for the timeline graph population pages and case-scoped
-- timeline queries: the graph materializer pages timeline_events by
-- (case_id, id), which previously re-walked the long-key primary index for
-- every page (~19s per 20k page on multi-million-event sources).
CREATE INDEX IF NOT EXISTS idx_source_timeline_case_id_id
    ON timeline_events (case_id, id);
