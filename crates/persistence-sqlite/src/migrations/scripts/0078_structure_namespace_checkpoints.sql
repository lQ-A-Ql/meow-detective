CREATE TABLE structure_namespace_checkpoints (
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE CHECK (length(trim(case_id)) > 0),
    run_id TEXT NOT NULL CHECK (length(trim(run_id)) > 0),
    target_id TEXT NOT NULL CHECK (length(trim(target_id)) > 0),
    snapshot_id TEXT NOT NULL CHECK (length(trim(snapshot_id)) > 0),
    query_sha256 TEXT NOT NULL CHECK (
        length(query_sha256) = 64 AND query_sha256 NOT GLOB '*[^0-9a-f]*'
    ),
    ordering_id TEXT NOT NULL CHECK (length(trim(ordering_id)) > 0),
    attempt_id TEXT NOT NULL CHECK (length(trim(attempt_id)) > 0),
    owner_id TEXT NOT NULL CHECK (length(trim(owner_id)) > 0),
    next_page BLOB NOT NULL CHECK (typeof(next_page) = 'blob' AND length(next_page) = 8),
    next_ordinal BLOB NOT NULL CHECK (typeof(next_ordinal) = 'blob' AND length(next_ordinal) = 8),
    verified_objects BLOB NOT NULL CHECK (
        typeof(verified_objects) = 'blob' AND length(verified_objects) = 8
        AND verified_objects = next_ordinal
    ),
    next_cursor TEXT CHECK (next_cursor IS NULL OR length(trim(next_cursor)) > 0),
    eof INTEGER NOT NULL CHECK (eof IN (0,1)),
    last_page_sha256 TEXT CHECK (
        last_page_sha256 IS NULL OR (
            length(last_page_sha256) = 64 AND last_page_sha256 NOT GLOB '*[^0-9a-f]*'
        )
    ),
    output_receipt TEXT CHECK (output_receipt IS NULL OR length(trim(output_receipt)) > 0),
    last_page_identity TEXT CHECK (
        last_page_identity IS NULL OR (
            length(last_page_identity) = 89
            AND substr(last_page_identity,1,25) = 'namespace-page-commit-v1-'
            AND substr(last_page_identity,26) NOT GLOB '*[^0-9a-f]*'
        )
    ),
    PRIMARY KEY (case_id,run_id,target_id),
    CHECK (
        (next_page = X'0000000000000000' AND next_ordinal = X'0000000000000000'
            AND next_cursor IS NULL AND eof = 0 AND last_page_sha256 IS NULL
            AND output_receipt IS NULL AND last_page_identity IS NULL)
        OR (next_page > X'0000000000000000' AND last_page_sha256 IS NOT NULL
            AND output_receipt IS NOT NULL AND last_page_identity IS NOT NULL
            AND ((eof = 1 AND next_cursor IS NULL)
                OR (eof = 0 AND next_cursor IS NOT NULL AND next_ordinal > X'0000000000000000')))
    )
);

CREATE INDEX idx_structure_namespace_checkpoints_run
    ON structure_namespace_checkpoints(case_id,run_id,eof);

CREATE TRIGGER structure_namespace_checkpoint_identity_immutable
BEFORE UPDATE ON structure_namespace_checkpoints
WHEN NEW.case_id IS NOT OLD.case_id OR NEW.run_id IS NOT OLD.run_id
    OR NEW.target_id IS NOT OLD.target_id OR NEW.snapshot_id IS NOT OLD.snapshot_id
    OR NEW.query_sha256 IS NOT OLD.query_sha256 OR NEW.ordering_id IS NOT OLD.ordering_id
    OR NEW.attempt_id IS NOT OLD.attempt_id OR NEW.owner_id IS NOT OLD.owner_id
BEGIN
    SELECT RAISE(ABORT, 'namespace checkpoint identity and ownership are immutable');
END;

CREATE TRIGGER structure_namespace_checkpoint_progress
BEFORE UPDATE ON structure_namespace_checkpoints
WHEN OLD.eof = 1 OR NEW.next_page <= OLD.next_page
    OR NEW.next_ordinal < OLD.next_ordinal OR NEW.verified_objects < OLD.verified_objects
    OR (NEW.eof = 0 AND NEW.next_ordinal = OLD.next_ordinal)
BEGIN
    SELECT RAISE(ABORT, 'namespace checkpoint update must commit forward progress');
END;
