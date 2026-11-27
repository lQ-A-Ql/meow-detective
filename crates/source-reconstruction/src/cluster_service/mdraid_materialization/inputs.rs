use domain::{CaseId, DataSource};
use evidence_access::mdraid::{MdRaidArrayPlan, MdRaidPlanError};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, linux_import_set_repo::LinuxImportSetRepo,
    source_revision_repo::SourceRevisionRepo,
};

pub(super) fn validate_registered(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    plan: &MdRaidArrayPlan,
    inputs: &[domain::structure::SourceRevisionRef],
) -> Result<(), MdRaidPlanError> {
    if inputs.len() != plan.source_content.len() {
        return Err(MdRaidPlanError::InvalidContentIdentity);
    }
    let revisions = SourceRevisionRepo::new(conn);
    let sources = DataSourceRepo::new(conn);
    let mut seen = std::collections::HashSet::new();
    for (content, input) in plan.source_content.iter().zip(inputs) {
        if input.case_id != case_id.0 || !seen.insert(input.source_id.as_str()) {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
        if sources.source_kind(&domain::DataSourceId(input.source_id.clone()))?
            != content.source_kind
        {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
        let source_path = sources.source_path(&domain::DataSourceId(input.source_id.clone()))?;
        if std::path::Path::new(&source_path).canonicalize()?
            != std::path::Path::new(&content.source_path)
        {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
        revisions.require_current(input)?;
        let (current, known) = revisions.current(&case_id.0, &input.source_id)?;
        if current != *input
            || known.logical_size != content.logical_size
            || known.logical_sha256.as_deref() != Some(content.logical_sha256.as_str())
        {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
    }
    Ok(())
}

pub(super) fn from_collection(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    import: &super::super::LinuxEvidenceSetImportPlan,
    plan: &MdRaidArrayPlan,
) -> Result<Vec<domain::structure::SourceRevisionRef>, MdRaidPlanError> {
    let members = LinuxImportSetRepo::new(conn);
    let revisions = SourceRevisionRepo::new(conn);
    let registered = members.find_members(&import.import_set_id)?;
    let coordinates = import
        .members
        .iter()
        .map(|member| Ok((member, member.source_path.canonicalize()?)))
        .collect::<Result<Vec<_>, MdRaidPlanError>>()?;
    let mut inputs = Vec::with_capacity(plan.source_content.len());
    for content in &plan.source_content {
        let mut matched = coordinates.iter().filter(|(member, path)| {
            member.source_kind == content.source_kind
                && path == std::path::Path::new(&content.source_path)
        });
        let (member, _) = matched
            .next()
            .ok_or(MdRaidPlanError::InvalidContentIdentity)?;
        if matched.next().is_some() {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
        let persisted = registered
            .iter()
            .find(|record| record.member_index == member.member_index)
            .ok_or(MdRaidPlanError::InvalidContentIdentity)?;
        if std::path::Path::new(&persisted.source_path) != member.source_path
            || persisted.source_kind != member.source_kind.to_string()
        {
            return Err(MdRaidPlanError::InvalidContentIdentity);
        }
        let source_id = members
            .find_member_source_id(&case_id.0, &import.import_set_id, member.member_index)?
            .ok_or(MdRaidPlanError::InvalidContentIdentity)?;
        inputs.push(revisions.current(&case_id.0, &source_id)?.0);
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
