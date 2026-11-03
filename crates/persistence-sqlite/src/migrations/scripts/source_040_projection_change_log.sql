CREATE TABLE IF NOT EXISTS source_projection_change_clock (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    revision INTEGER NOT NULL CHECK (revision >= 0)
);

INSERT OR IGNORE INTO source_projection_change_clock (singleton, revision)
VALUES (1, 0);

CREATE TABLE IF NOT EXISTS source_projection_changes (
    entity TEXT NOT NULL CHECK (entity IN ('artifact', 'timeline')),
    entity_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    operation TEXT NOT NULL CHECK (operation IN ('upsert', 'delete')),
    revision INTEGER NOT NULL CHECK (revision > 0),
    PRIMARY KEY (entity, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_source_projection_changes_revision
ON source_projection_changes(revision, entity, entity_id);

CREATE TRIGGER IF NOT EXISTS trg_projection_artifacts_insert
AFTER INSERT ON artifacts
BEGIN
    UPDATE source_projection_change_clock SET revision = revision + 1 WHERE singleton = 1;
    INSERT INTO source_projection_changes (entity, entity_id, source_id, operation, revision)
    VALUES ('artifact', NEW.id, NEW.data_source_id, 'upsert',
            (SELECT revision FROM source_projection_change_clock WHERE singleton = 1))
    ON CONFLICT(entity, entity_id) DO UPDATE SET
        source_id = excluded.source_id,
        operation = excluded.operation,
        revision = excluded.revision;
END;

CREATE TRIGGER IF NOT EXISTS trg_projection_artifacts_update
AFTER UPDATE ON artifacts
BEGIN
    UPDATE source_projection_change_clock SET revision = revision + 1 WHERE singleton = 1;
    INSERT INTO source_projection_changes (entity, entity_id, source_id, operation, revision)
    VALUES ('artifact', OLD.id, OLD.data_source_id, 'delete',
            (SELECT revision FROM source_projection_change_clock WHERE singleton = 1))
    ON CONFLICT(entity, entity_id) DO UPDATE SET
        source_id = excluded.source_id,
        operation = excluded.operation,
        revision = excluded.revision;
    UPDATE source_projection_change_clock SET revision = revision + 1 WHERE singleton = 1;
    INSERT INTO source_projection_changes (entity, entity_id, source_id, operation, revision)
    VALUES ('artifact', NEW.id, NEW.data_source_id, 'upsert',
            (SELECT revision FROM source_projection_change_clock WHERE singleton = 1))
    ON CONFLICT(entity, entity_id) DO UPDATE SET
        source_id = excluded.source_id,
        operation = excluded.operation,
        revision = excluded.revision;
END;

CREATE TRIGGER IF NOT EXISTS trg_projection_artifacts_delete
AFTER DELETE ON artifacts
BEGIN
    UPDATE source_projection_change_clock SET revision = revision + 1 WHERE singleton = 1;
    INSERT INTO source_projection_changes (entity, entity_id, source_id, operation, revision)
    VALUES ('artifact', OLD.id, OLD.data_source_id, 'delete',
            (SELECT revision FROM source_projection_change_clock WHERE singleton = 1))
    ON CONFLICT(entity, entity_id) DO UPDATE SET
        source_id = excluded.source_id,
        operation = excluded.operation,
        revision = excluded.revision;
END;

CREATE TRIGGER IF NOT EXISTS trg_projection_timeline_insert
AFTER INSERT ON timeline_events
BEGIN
    UPDATE source_projection_change_clock SET revision = revision + 1 WHERE singleton = 1;
    INSERT INTO source_projection_changes (entity, entity_id, source_id, operation, revision)
    VALUES ('timeline', NEW.id,
            COALESCE((SELECT id FROM data_sources ORDER BY id LIMIT 1), ''),
            'upsert',
            (SELECT revision FROM source_projection_change_clock WHERE singleton = 1))
    ON CONFLICT(entity, entity_id) DO UPDATE SET
        source_id = excluded.source_id,
        operation = excluded.operation,
        revision = excluded.revision;
END;

CREATE TRIGGER IF NOT EXISTS trg_projection_timeline_update
AFTER UPDATE ON timeline_events
BEGIN
    UPDATE source_projection_change_clock SET revision = revision + 1 WHERE singleton = 1;
    INSERT INTO source_projection_changes (entity, entity_id, source_id, operation, revision)
    VALUES ('timeline', OLD.id,
            COALESCE((SELECT id FROM data_sources ORDER BY id LIMIT 1), ''),
            'delete',
            (SELECT revision FROM source_projection_change_clock WHERE singleton = 1))
    ON CONFLICT(entity, entity_id) DO UPDATE SET
        source_id = excluded.source_id,
        operation = excluded.operation,
        revision = excluded.revision;
    UPDATE source_projection_change_clock SET revision = revision + 1 WHERE singleton = 1;
    INSERT INTO source_projection_changes (entity, entity_id, source_id, operation, revision)
    VALUES ('timeline', NEW.id,
            COALESCE((SELECT id FROM data_sources ORDER BY id LIMIT 1), ''),
            'upsert',
            (SELECT revision FROM source_projection_change_clock WHERE singleton = 1))
    ON CONFLICT(entity, entity_id) DO UPDATE SET
        source_id = excluded.source_id,
        operation = excluded.operation,
        revision = excluded.revision;
END;

CREATE TRIGGER IF NOT EXISTS trg_projection_timeline_delete
AFTER DELETE ON timeline_events
BEGIN
    UPDATE source_projection_change_clock SET revision = revision + 1 WHERE singleton = 1;
    INSERT INTO source_projection_changes (entity, entity_id, source_id, operation, revision)
    VALUES ('timeline', OLD.id,
            COALESCE((SELECT id FROM data_sources ORDER BY id LIMIT 1), ''),
            'delete',
            (SELECT revision FROM source_projection_change_clock WHERE singleton = 1))
    ON CONFLICT(entity, entity_id) DO UPDATE SET
        source_id = excluded.source_id,
        operation = excluded.operation,
        revision = excluded.revision;
END;
