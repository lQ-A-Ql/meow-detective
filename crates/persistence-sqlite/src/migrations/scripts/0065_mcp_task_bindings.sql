CREATE TABLE mcp_task_bindings (
    case_id TEXT NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    case_session_id TEXT NOT NULL,
    principal_id TEXT NOT NULL,
    operation TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    arguments_sha256 TEXT NOT NULL,
    task_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY(case_id, principal_id, operation, idempotency_key)
);

CREATE INDEX idx_mcp_task_bindings_task ON mcp_task_bindings(case_id, task_id);
