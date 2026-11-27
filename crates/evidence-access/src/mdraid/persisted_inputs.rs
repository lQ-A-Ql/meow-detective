use std::path::Path;

use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};

pub(super) fn validate(path: &Path) -> Result<(), super::MdRaidPlanError> {
    let descriptor = path.canonicalize()?;
    let directory = descriptor
        .parent()
        .ok_or(super::MdRaidPlanError::InvalidPublication)?;
    let derived = directory
        .parent()
        .ok_or(super::MdRaidPlanError::InvalidPublication)?;
    let root = derived
        .parent()
        .ok_or(super::MdRaidPlanError::InvalidPublication)?;
    if directory.file_name().and_then(|name| name.to_str()) != Some("mdraid")
        || derived.file_name().and_then(|name| name.to_str()) != Some("derived")
    {
        return Err(super::MdRaidPlanError::InvalidPublication);
    }
    let stem = descriptor
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or(super::MdRaidPlanError::InvalidPublication)?;
    let source_id = domain::DataSourceId(format!("mdraid-{stem}"));
    let conn = persistence_sqlite::connection::open_existing_case_read_only(&root.join("app.db"))?;
    let repo = DataSourceRepo::new(&conn);
    let case_id = repo.case_id(&source_id)?;
    let source = repo
        .find_by_case(&case_id)?
        .into_iter()
        .find(|source| source.id == source_id)
        .ok_or(super::MdRaidPlanError::InvalidPublication)?;
    if source.kind != domain::DataSourceKind::MdRaid
        || source.source_path.canonicalize()? != descriptor
    {
        return Err(super::MdRaidPlanError::InvalidPublication);
    }
    SourceRevisionRepo::new(&conn).require_current_inputs(&case_id.0, &source_id.0)?;
    Ok(())
}
