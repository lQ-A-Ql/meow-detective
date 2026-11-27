use domain::{CaseId, DataSource};
use evidence_access::mdraid::{MdRaidArrayPlan, MdRaidPlanError};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};

pub(super) fn resolve(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
) -> Result<Vec<domain::structure::SourceRevisionRef>, MdRaidPlanError> {
    let sources = DataSourceRepo::new(conn).find_by_case(case_id)?;
    let revisions = SourceRevisionRepo::new(conn);
    let mut inputs = Vec::new();
    for content in &plan.source_content {
        let canonical = std::path::Path::new(&content.source_path).canonicalize()?;
        let mut matched = Vec::new();
        for source in &sources {
            if source.kind == content.source_kind && source.source_path.canonicalize()? == canonical
            {
                matched.push(source);
            }
        }
        if matched.len() != 1 {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
        let (reference, known) = revisions.current(&case_id.0, &matched[0].id.0)?;
        if known.logical_size != content.logical_size
            || known.logical_sha256.as_deref() != Some(content.logical_sha256.as_str())
        {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
        inputs.push(reference);
    }
    Ok(inputs)
}

pub(super) fn validate(
    conn: &rusqlite::Connection,
    source: &DataSource,
) -> Result<(), MdRaidPlanError> {
    let case_id = DataSourceRepo::new(conn).case_id(&source.id)?;
    SourceRevisionRepo::new(conn).require_current_inputs(&case_id.0, &source.id.0)?;
    Ok(())
}
