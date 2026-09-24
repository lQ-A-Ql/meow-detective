use super::super::types::BackgroundLinuxEvidenceSetImportJob;

pub(super) fn record_cluster_phase(
    connection: &rusqlite::Connection,
    job: &BackgroundLinuxEvidenceSetImportJob,
    phase: &str,
    success: bool,
    error: Option<&str>,
    ready_count: u32,
    failed_count: u32,
) {
    let attempt: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM investigation_steps
             WHERE case_id = ?1 AND step_kind = 'linux_evidence_set_import'
               AND json_extract(params_json, '$.phase') = ?2",
            rusqlite::params![job.case_id.0, phase],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        .saturating_add(1) as u64;
    let params = serde_json::json!({
        "importSetId": job.plan.import_set_id,
        "phase": phase,
        "attempt": attempt,
        "readyCount": ready_count,
        "failedCount": failed_count,
        "error": error,
    })
    .to_string();
    if let Err(record_error) = app_services::step_recorder::record_step(
        connection,
        &job.case_root,
        app_services::step_recorder::CaseStepInput {
            case_id: &job.case_id.0,
            step_kind: "linux_evidence_set_import",
            params_json: &params,
            duration_ms: 0,
            success,
            error_code: error,
        },
    ) {
        tracing::warn!(
            import_set_id = %job.plan.import_set_id,
            phase,
            error = %record_error,
            "Failed to record Linux evidence-set phase ledger entry"
        );
    }
}
