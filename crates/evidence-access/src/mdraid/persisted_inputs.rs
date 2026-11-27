use std::path::Path;

use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};

pub(crate) fn validate(
    source_id: &domain::DataSourceId,
    path: &Path,
) -> Result<(), super::MdRaidPlanError> {
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
    let conn = persistence_sqlite::connection::open_existing_case_read_only(&root.join("app.db"))?;
    let repo = DataSourceRepo::new(&conn);
    let case_id = repo.case_id(source_id)?;
    if repo.source_kind(source_id)? != domain::DataSourceKind::MdRaid
        || Path::new(&repo.source_path(source_id)?).canonicalize()? != descriptor
    {
        return Err(super::MdRaidPlanError::InvalidPublication);
    }
    SourceRevisionRepo::new(&conn).require_current_inputs(&case_id.0, &source_id.0)?;
    let version =
        persistence_sqlite::repositories::storage_object_repo::StorageObjectRepo::new(&conn)
            .version_for_source(&case_id.0, &source_id.0)?
            .ok_or(super::MdRaidPlanError::InvalidPublication)?;
    let plan = super::read_mdraid_descriptor(&descriptor)?;
    if version.mapping_sha256 != super::mdraid_array_fingerprint(&plan)?
        || version.adapter_version != "fs-lvm/mdraid-v2"
    {
        return Err(super::MdRaidPlanError::InvalidPublication);
    }
    let inputs = SourceRevisionRepo::new(&conn).inputs(&case_id.0, &source_id.0)?;
    let fingerprint = super::mdraid_array_fingerprint(&plan)?;
    let expected = super::registered_source_id(&plan.array_uuid, &fingerprint, &inputs);
    if source_id.0 != expected {
        return Err(super::MdRaidPlanError::InvalidPublication);
    }
    Ok(())
}
