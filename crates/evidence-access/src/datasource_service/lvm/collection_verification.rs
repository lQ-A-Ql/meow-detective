use super::collection::{CollectionPlanError, ImageCollectionPlan};
use crate::datasource_service::LvmLogicalVolumeIdentity;
use std::{collections::BTreeSet, path::Path, sync::atomic::AtomicBool};

impl ImageCollectionPlan {
    pub fn validate_member_inputs(
        &self,
        index: usize,
        path: &Path,
        kind: &domain::DataSourceKind,
        cancel: &AtomicBool,
    ) -> Result<(), CollectionPlanError> {
        super::collection::check_cancel(cancel)?;
        let probe = self.member_probe(index, path, kind)?;
        let mut sources = BTreeSet::from([path.canonicalize()?]);
        for identity in probe
            .candidates
            .iter()
            .filter_map(|item| item.lvm_identity.as_ref())
            .chain(
                probe
                    .partitions
                    .iter()
                    .filter_map(|item| item.lvm_identity.as_ref()),
            )
            .chain(
                probe
                    .unsupported_volumes
                    .iter()
                    .filter_map(|item| item.lvm_identity.as_ref()),
            )
        {
            append_dependencies(identity, &mut sources)?;
        }
        for path in sources {
            self.validate_input_identity(&path)?;
        }
        self.validate_snapshot()
    }
}

fn append_dependencies(
    identity: &LvmLogicalVolumeIdentity,
    paths: &mut BTreeSet<std::path::PathBuf>,
) -> Result<(), CollectionPlanError> {
    if identity.pv_sources.is_empty() || identity.pv_sources.len() != identity.pv_offsets.len() {
        return Err(CollectionPlanError::MemberMismatch);
    }
    for source in &identity.pv_sources {
        if source.source_kind.is_none() || source.pv_uuid.is_empty() {
            return Err(CollectionPlanError::MemberMismatch);
        }
        paths.insert(Path::new(&source.source_path).canonicalize()?);
    }
    Ok(())
}
