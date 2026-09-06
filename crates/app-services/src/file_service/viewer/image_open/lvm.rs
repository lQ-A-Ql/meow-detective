use std::{collections::HashMap, path::Path};

use evidence_core::EvidenceReader;

use crate::{
    datasource_service::normalize_lvm_uuid_for_match,
    file_service::viewer::{
        PreviewLvmIdentity, PreviewLvmPhysicalVolumeSource, PreviewPartitionCandidate,
    },
};

pub(crate) struct LvmPoolRequestCache {
    pools: HashMap<LvmPoolCacheKey, fs_lvm::LvmPool>,
}

impl LvmPoolRequestCache {
    pub(crate) fn new() -> Self {
        Self {
            pools: HashMap::new(),
        }
    }

    pub(crate) fn open_volume<F>(
        &mut self,
        source_path: &Path,
        identity: &PreviewLvmIdentity,
        open_reader: &mut F,
    ) -> std::io::Result<Box<dyn EvidenceReader>>
    where
        F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>> + ?Sized,
    {
        let key = LvmPoolCacheKey::from_identity(identity);
        if !self.pools.contains_key(&key) {
            self.pools.insert(
                key.clone(),
                discover_lvm_pool(source_path, identity, open_reader)?,
            );
        }
        open_lvm_volume_from_pool(
            self.pools
                .get(&key)
                .expect("pool was inserted before lookup"),
            identity,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LvmPoolCacheKey {
    vg_uuid: String,
    vg_name: String,
    pv_offsets: Vec<u64>,
    pv_sources: Vec<(String, u64, String)>,
}

impl LvmPoolCacheKey {
    fn from_identity(identity: &PreviewLvmIdentity) -> Self {
        Self {
            vg_uuid: identity.vg_uuid.clone(),
            vg_name: identity.vg_name.clone(),
            pv_offsets: identity.pv_offsets.clone(),
            pv_sources: identity
                .pv_sources
                .iter()
                .map(|source| {
                    (
                        source.source_path.clone(),
                        source.offset,
                        format!(
                            "{}|{}",
                            source.source_kind,
                            normalize_lvm_uuid_for_match(&source.pv_uuid)
                        ),
                    )
                })
                .collect(),
        }
    }
}

pub(crate) fn open_candidate_block_reader<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    open_reader: &mut F,
) -> std::io::Result<(Box<dyn EvidenceReader>, u64)>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>> + ?Sized,
{
    match &candidate.lvm_identity {
        Some(identity) => open_lvm_logical_volume_reader(source_path, identity, open_reader)
            .map(|reader| (reader, 0)),
        None => open_reader(source_path).map(|reader| (reader, candidate.offset)),
    }
}

pub(crate) fn open_candidate_block_reader_with_lvm_cache<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    open_reader: &mut F,
    lvm_cache: &mut LvmPoolRequestCache,
) -> std::io::Result<(Box<dyn EvidenceReader>, u64)>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>> + ?Sized,
{
    match &candidate.lvm_identity {
        Some(identity) => lvm_cache
            .open_volume(source_path, identity, open_reader)
            .map(|reader| (reader, 0)),
        None => open_reader(source_path).map(|reader| (reader, candidate.offset)),
    }
}

pub(crate) fn open_lvm_logical_volume_reader<F>(
    source_path: &Path,
    identity: &PreviewLvmIdentity,
    open_reader: &mut F,
) -> std::io::Result<Box<dyn EvidenceReader>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>> + ?Sized,
{
    validate_identity(identity)?;
    let pool = discover_lvm_pool(source_path, identity, open_reader)?;
    open_lvm_volume_from_pool(&pool, identity)
}

fn validate_identity(identity: &PreviewLvmIdentity) -> std::io::Result<()> {
    if identity.pv_offsets.is_empty() {
        return Err(invalid_input(
            "LVM preview identity has no physical volume offsets",
        ));
    }
    if identity.pv_offsets.len() > 1 && identity.pv_sources.len() != identity.pv_offsets.len() {
        return Err(invalid_input(format!(
            "LVM preview identity has {} PV offsets but {} PV sources",
            identity.pv_offsets.len(),
            identity.pv_sources.len()
        )));
    }
    Ok(())
}

fn discover_lvm_pool<F>(
    source_path: &Path,
    identity: &PreviewLvmIdentity,
    open_reader: &mut F,
) -> std::io::Result<fs_lvm::LvmPool>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>> + ?Sized,
{
    validate_identity(identity)?;
    let mut readers = Vec::with_capacity(identity.pv_offsets.len());
    for (index, pv_offset) in identity.pv_offsets.iter().enumerate() {
        let source = identity.pv_sources.get(index);
        let reader_path = source
            .map(|source| Path::new(&source.source_path))
            .unwrap_or(source_path);
        let mut reader = match source {
            Some(source) => open_lvm_pv_reader(reader_path, source, open_reader)?,
            None => open_reader(reader_path)?,
        };
        if let Some(source) = source {
            validate_preview_lvm_pv_source(reader.as_mut(), *pv_offset, source)?;
        }
        readers.push(reader);
    }
    fs_lvm::LvmPool::discover(readers, identity.pv_offsets.clone()).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("LVM discovery failed for preview: {error}"),
        )
    })
}

fn open_lvm_pv_reader<F>(
    reader_path: &Path,
    source: &PreviewLvmPhysicalVolumeSource,
    open_reader: &mut F,
) -> std::io::Result<Box<dyn EvidenceReader>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>> + ?Sized,
{
    match source.source_kind.to_ascii_lowercase().as_str() {
        "e01" => image_e01::E01Reader::open(reader_path)
            .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>),
        "raw" => evidence_core::RawImageReader::open(reader_path)
            .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>),
        "local_disk" => evidence_core::LocalDiskReader::open(reader_path)
            .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>),
        _ => open_reader(reader_path),
    }
}

fn validate_preview_lvm_pv_source(
    reader: &mut dyn EvidenceReader,
    expected_offset: u64,
    expected_source: &PreviewLvmPhysicalVolumeSource,
) -> std::io::Result<()> {
    if expected_source.pv_uuid.is_empty() {
        return Ok(());
    }
    let label = fs_lvm::label::parse_pv_label(reader, expected_offset).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "LVM preview PV '{}' at offset {} label validation failed: {}",
                crate::datasource_service::lvm_source_fingerprint(&expected_source.source_path),
                expected_offset,
                error
            ),
        )
    })?;
    if normalize_lvm_uuid_for_match(&label.pv_uuid)
        != normalize_lvm_uuid_for_match(&expected_source.pv_uuid)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "LVM preview PV '{}' at offset {} UUID mismatch: expected {}, found {}",
                crate::datasource_service::lvm_source_fingerprint(&expected_source.source_path),
                expected_offset,
                expected_source.pv_uuid,
                label.pv_uuid
            ),
        ));
    }
    Ok(())
}

fn open_lvm_volume_from_pool(
    pool: &fs_lvm::LvmPool,
    identity: &PreviewLvmIdentity,
) -> std::io::Result<Box<dyn EvidenceReader>> {
    let index = find_lvm_preview_volume_index(pool, identity).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "LVM logical volume not found for preview: {}/{}",
                identity.vg_name, identity.lv_name
            ),
        )
    })?;
    pool.open_volume_reader(index).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("LVM logical volume open failed for preview: {error}"),
        )
    })
}

pub(crate) fn find_lvm_preview_volume_index(
    pool: &fs_lvm::LvmPool,
    identity: &PreviewLvmIdentity,
) -> Option<usize> {
    let volumes = pool.list_volumes();
    if !identity.lv_uuid.is_empty() {
        if let Some(index) = volumes
            .iter()
            .position(|volume| volume.uuid == identity.lv_uuid)
        {
            return Some(index);
        }
    }
    volumes
        .iter()
        .position(|volume| volume.name == identity.lv_name)
}

fn invalid_input(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into())
}
