use std::path::Path;

use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};

use super::plan::LvmPlanError;

pub(crate) fn validate(
    source_id: &domain::DataSourceId,
    descriptor_path: &Path,
) -> Result<(), LvmPlanError> {
    let descriptor = descriptor_path.canonicalize()?;
    let directory = descriptor
        .parent()
        .ok_or(LvmPlanError::InvalidPublication)?;
    let derived = directory.parent().ok_or(LvmPlanError::InvalidPublication)?;
    let root = derived.parent().ok_or(LvmPlanError::InvalidPublication)?;
    if directory.file_name().and_then(|name| name.to_str()) != Some("lvm")
        || derived.file_name().and_then(|name| name.to_str()) != Some("derived")
    {
        return Err(LvmPlanError::InvalidPublication);
    }
    let conn = persistence_sqlite::connection::open_existing_case_read_only(&root.join("app.db"))?;
    let repo = DataSourceRepo::new(&conn);
    let case_id = repo.case_id(source_id)?;
    if repo.source_kind(source_id)? != domain::DataSourceKind::LvmLogicalVolume
        || Path::new(&repo.source_path(source_id)?).canonicalize()? != descriptor
    {
        return Err(LvmPlanError::InvalidPublication);
    }
    let revisions = SourceRevisionRepo::new(&conn);
    revisions.require_current_inputs(&case_id.0, &source_id.0)?;
    let (_, output_revision) = revisions.current(&case_id.0, &source_id.0)?;
    let version =
        persistence_sqlite::repositories::storage_object_repo::StorageObjectRepo::new(&conn)
            .version_for_source(&case_id.0, &source_id.0)?
            .ok_or(LvmPlanError::InvalidPublication)?;
    let plan = super::descriptor::read_lvm_descriptor(&descriptor)?;
    let fingerprint = super::plan::lvm_volume_fingerprint(&plan)?;
    if version.mapping_sha256 != fingerprint
        || version.adapter_version != super::plan::LVM_ADAPTER_VERSION
        || output_revision.metadata_identity.as_deref() != Some(version.id.0.as_str())
    {
        return Err(LvmPlanError::InvalidPublication);
    }
    let inputs = revisions.inputs(&case_id.0, &source_id.0)?;
    let expected =
        super::identity::registered_source_id(&plan.vg_uuid, &plan.lv_uuid, &fingerprint, &inputs);
    if source_id.0 != expected {
        return Err(LvmPlanError::InvalidPublication);
    }
    Ok(())
}
