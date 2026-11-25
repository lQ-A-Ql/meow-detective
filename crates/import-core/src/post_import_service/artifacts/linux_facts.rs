use crate::post_import_service::CapabilityWork;
use analysis_core::analysis_service::AnalysisServiceError;
use domain::{CaseId, DataSourceId};

pub(super) fn persist(
    work: &CapabilityWork,
    case_connection: &rusqlite::Connection,
) -> Result<(), AnalysisServiceError> {
    let source = DataSourceId(work.row.data_source_id.clone());
    let facts = analysis_core::linux_evidence_facts::collect_linux_evidence_facts(
        case_connection,
        &work.case_root,
        &CaseId(work.row.case_id.clone()),
        &source,
    );
    let connection = source_kernel::source_db::open_registered_source_db(
        case_connection,
        &work.case_root,
        &source,
    )?;
    analysis_core::linux_evidence_facts::persist_linux_evidence_facts(
        &connection,
        &source,
        &facts,
    )?;
    Ok(())
}
