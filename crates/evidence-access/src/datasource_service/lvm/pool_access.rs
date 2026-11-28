use crate::{datasource_service::LvmPhysicalVolumeSource, lvm::mapping_cache::SharedLvmPool};
use domain::DataSourceKind;
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

pub(super) fn discover_lvm_pool(
    sources: &[LvmPhysicalVolumeSource],
    default_kind: &DataSourceKind,
    content: Option<&super::collection_content::CollectionContent>,
) -> Result<(Arc<SharedLvmPool>, u64), fs_lvm::LvmError> {
    let seed = sources.first().map_or(0, |source| source.offset);
    if let Some(content) = content {
        let pool = crate::lvm::mapping_cache::open(
            sources,
            |source| {
                content
                    .get(
                        Path::new(&source.source_path),
                        source.source_kind.as_ref().unwrap_or(default_kind),
                    )
                    .map(|content| content.as_ref().clone())
                    .map_err(std::io::Error::other)
            },
            &AtomicBool::new(false),
            None,
        )?;
        return Ok((pool, seed));
    }
    let mut readers = Vec::with_capacity(sources.len());
    for source in sources {
        readers.push(
            crate::source_runtime::open_factory(
                Path::new(&source.source_path),
                source.source_kind.as_ref().unwrap_or(default_kind),
            )?
            .open_reader()?,
        );
    }
    Ok((
        SharedLvmPool::from_pool(fs_lvm::LvmPool::discover(
            readers,
            sources.iter().map(|source| source.offset).collect(),
        )?)?,
        seed,
    ))
}
