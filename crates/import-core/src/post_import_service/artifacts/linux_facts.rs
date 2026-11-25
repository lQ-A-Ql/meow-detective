use crate::post_import_service::CapabilityWork;
use analysis_core::analysis_service::AnalysisServiceError;
use domain::{CaseId, DataSourceId};

pub(super) fn persist(
    work: &CapabilityWork,
    case_connection: &rusqlite::Connection,
) -> Result<(), AnalysisServiceError> {
    let source = DataSourceId(work.row.data_source_id.clone());
    let case_id = CaseId(work.row.case_id.clone());
    let ready = source_kernel::source_db::open_ready_source_by_id(
        case_connection,
        &work.case_root,
        &case_id,
        &source,
    )?;
    let facts = analysis_core::linux_evidence_facts::collect_linux_evidence_facts(
        case_connection,
        &work.case_root,
        &case_id,
        &source,
    );
    analysis_core::linux_evidence_facts::persist_linux_evidence_facts(
        &ready.connection,
        &source,
        &facts,
    )?;
    Ok(())
}
