use super::types::BackgroundLinuxEvidenceSetImportJob;

pub(super) fn assess_cephfs_presence(
    connection: &rusqlite::Connection,
    job: &BackgroundLinuxEvidenceSetImportJob,
    ceph_scope_id: &domain::CephScopeId,
) {
    match app_services::ceph_reconstruction::assess_cephfs_presence_for_scope(
        connection,
        &job.case_root,
        &job.case_id,
        ceph_scope_id,
    ) {
        Ok(assessment) => tracing::info!(
            ceph_scope_id = %ceph_scope_id.0,
            state = %assessment.state,
            sources = assessment.source_count,
            "CephFS presence assessed for typed Ceph scope"
        ),
        Err(error) => tracing::warn!(
            ceph_scope_id = %ceph_scope_id.0,
            %error,
            "CephFS presence assessment failed"
        ),
    }
}
