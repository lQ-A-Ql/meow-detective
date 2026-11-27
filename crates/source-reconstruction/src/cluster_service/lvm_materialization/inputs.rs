use domain::DataSource;
use evidence_access::lvm::{LvmPlanError, LvmVolumePlan};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, linux_import_set_repo::LinuxImportSetRepo,
    source_revision_repo::SourceRevisionRepo,
};

pub(super) fn validate_registered(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    plan: &LvmVolumePlan,
    inputs: &[domain::structure::SourceRevisionRef],
) -> Result<(), LvmPlanError> {
    if inputs.len() != plan.sources.len() {
        return Err(LvmPlanError::InvalidContentIdentity);
    }
    let revisions = SourceRevisionRepo::new(conn);
    let sources = DataSourceRepo::new(conn);
    let mut seen = std::collections::HashSet::new();
    for (source, input) in plan.sources.iter().zip(inputs) {
        if input.case_id != case_id.0 || !seen.insert(input.source_id.as_str()) {
            return Err(LvmPlanError::InvalidContentIdentity);
        }
        let source_id = domain::DataSourceId(input.source_id.clone());
        if sources.source_kind(&source_id)? != source.source_kind {
            return Err(LvmPlanError::InvalidContentIdentity);
        }
        let source_path = sources.source_path(&source_id)?;
        if std::path::Path::new(&source_path).canonicalize()?
            != std::path::Path::new(&source.source_path).canonicalize()?
        {
            return Err(LvmPlanError::InvalidContentIdentity);
        }
        revisions.require_current(input)?;
        let (current, known) = revisions.current(&case_id.0, &input.source_id)?;
        if current != *input
            || known.logical_size != source.content.logical_size
            || known.logical_sha256.as_deref() != Some(source.content.logical_sha256.as_str())
        {
            return Err(LvmPlanError::InvalidContentIdentity);
        }
    }
    Ok(())
}

pub(super) fn from_collection(
    conn: &rusqlite::Connection,
    case_id: &domain::CaseId,
    import: &super::super::LinuxEvidenceSetImportPlan,
    plan: &LvmVolumePlan,
) -> Result<Vec<domain::structure::SourceRevisionRef>, LvmPlanError> {
    let members = LinuxImportSetRepo::new(conn);
    let revisions = SourceRevisionRepo::new(conn);
    let registered = members.find_members(&import.import_set_id)?;
    let coordinates = import
        .members
        .iter()
        .map(|member| Ok((member, member.source_path.canonicalize()?)))
        .collect::<Result<Vec<_>, LvmPlanError>>()?;
    let mut inputs = Vec::with_capacity(plan.sources.len());
    for source in &plan.sources {
        let canonical = std::path::Path::new(&source.source_path).canonicalize()?;
        let mut matched = coordinates.iter().filter(|(member, path)| {
            member.source_kind == source.source_kind && path == &canonical
        });
        let (member, _) = matched.next().ok_or(LvmPlanError::InvalidContentIdentity)?;
        if matched.next().is_some() {
            return Err(LvmPlanError::InvalidContentIdentity);
        }
        let persisted = registered
            .iter()
            .find(|record| record.member_index == member.member_index)
            .ok_or(LvmPlanError::InvalidContentIdentity)?;
        if std::path::Path::new(&persisted.source_path) != member.source_path
            || persisted.source_kind != member.source_kind.to_string()
        {
            return Err(LvmPlanError::InvalidContentIdentity);
        }
        let source_id = members
            .find_member_source_id(&case_id.0, &import.import_set_id, member.member_index)?
            .ok_or(LvmPlanError::InvalidContentIdentity)?;
        inputs.push(revisions.current(&case_id.0, &source_id)?.0);
    }
    Ok(inputs)
}

pub(super) fn validate(
    conn: &rusqlite::Connection,
    source: &DataSource,
) -> Result<(), LvmPlanError> {
    let case_id = DataSourceRepo::new(conn).case_id(&source.id)?;
    SourceRevisionRepo::new(conn).require_current_inputs(&case_id.0, &source.id.0)?;
    Ok(())
}
