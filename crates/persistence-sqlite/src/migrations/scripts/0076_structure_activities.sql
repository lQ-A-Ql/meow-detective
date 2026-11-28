CREATE TABLE structure_activities (
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE
        CHECK (length(trim(case_id)) > 0),
    run_id TEXT NOT NULL CHECK (length(trim(run_id)) > 0),
    work_key TEXT NOT NULL CHECK (length(trim(work_key)) > 0),
    attempt_id TEXT NOT NULL CHECK (length(trim(attempt_id)) > 0),
    owner_id TEXT NOT NULL CHECK (length(trim(owner_id)) > 0),
    definition_sha256 TEXT NOT NULL CHECK (
        length(definition_sha256) = 64 AND definition_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    input_capability_ids_json TEXT NOT NULL CHECK (
        json_valid(input_capability_ids_json)
        AND json_type(input_capability_ids_json) = 'array'
        AND json_array_length(input_capability_ids_json) > 0
    ),
    target_id TEXT NOT NULL CHECK (length(trim(target_id)) > 0),
    state TEXT NOT NULL CHECK (state IN (
        'pending', 'ready', 'admitted', 'running', 'succeeded', 'failed', 'blocked', 'cancelled'
    )),
    output_receipt TEXT CHECK (output_receipt IS NULL OR length(trim(output_receipt)) > 0),
    failure_json TEXT CHECK (
        failure_json IS NULL OR (json_valid(failure_json) AND json_type(failure_json) = 'object')
    ),
    PRIMARY KEY (case_id, run_id, work_key),
    CHECK (
        (state IN ('pending', 'ready', 'admitted', 'running')
            AND output_receipt IS NULL AND failure_json IS NULL)
        OR (state = 'succeeded' AND output_receipt IS NOT NULL AND failure_json IS NULL)
        OR (state IN ('failed', 'blocked', 'cancelled')
            AND output_receipt IS NULL AND failure_json IS NOT NULL)
    ),
    CHECK (
        failure_json IS NULL OR COALESCE(
            (state = 'failed' AND json_extract(failure_json, '$.kind') IN (
                'unsupported_feature', 'corrupt_metadata', 'conflicted',
                'resource_limit', 'io_unavailable', 'deadline_exceeded'
            ))
            OR (state = 'blocked' AND json_extract(failure_json, '$.kind') IN (
                'missing_capabilities', 'missing_evidence', 'required_dependency_failed'
            ))
            OR (state = 'cancelled' AND json_extract(failure_json, '$.kind') = 'cancelled'),
            0
        )
    )
);

CREATE INDEX idx_structure_activities_run_state
    ON structure_activities(case_id, run_id, state);

CREATE TRIGGER structure_activity_input_references
BEFORE INSERT ON structure_activities
WHEN EXISTS (
    SELECT 1 FROM json_each(NEW.input_capability_ids_json)
    WHERE type <> 'text' OR length(trim(value)) = 0
) OR (
    SELECT COUNT(*) <> COUNT(DISTINCT value) FROM json_each(NEW.input_capability_ids_json)
)
BEGIN
    SELECT RAISE(ABORT, 'activity input references must be distinct nonempty strings');
END;

CREATE TRIGGER structure_activity_definition_immutable
BEFORE UPDATE ON structure_activities
WHEN NEW.case_id IS NOT OLD.case_id
    OR NEW.run_id IS NOT OLD.run_id
    OR NEW.work_key IS NOT OLD.work_key
    OR NEW.attempt_id IS NOT OLD.attempt_id
    OR NEW.owner_id IS NOT OLD.owner_id
    OR NEW.definition_sha256 IS NOT OLD.definition_sha256
    OR NEW.input_capability_ids_json IS NOT OLD.input_capability_ids_json
    OR NEW.target_id IS NOT OLD.target_id
BEGIN
    SELECT RAISE(ABORT, 'activity definition and attempt ownership are immutable');
END;

CREATE TRIGGER structure_activity_state_transition
BEFORE UPDATE ON structure_activities
WHEN NOT (
    (OLD.state = 'pending' AND NEW.state IN ('ready', 'failed', 'blocked', 'cancelled'))
    OR (OLD.state = 'ready' AND NEW.state IN ('admitted', 'failed', 'blocked', 'cancelled'))
    OR (OLD.state = 'admitted' AND NEW.state IN ('running', 'failed', 'blocked', 'cancelled'))
    OR (OLD.state = 'running' AND NEW.state IN ('succeeded', 'failed', 'blocked', 'cancelled'))
)
BEGIN
    SELECT RAISE(ABORT, 'activity state transition is not allowed');
END;
