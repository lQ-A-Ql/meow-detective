use evidence_core::EvidenceReader;

use crate::error::{LvmError, Result};
use crate::metadata::{LvMeta, LvRole, SegmentDependencies};
use crate::thin::ThinMetadata;
use crate::{lv_info_from_meta, LvInfo, LvmPool, ThinLvReader};

impl LvmPool {
    /// List volumes that can be opened as read-only block devices.
    ///
    /// This includes directly mapped linear/striped LVs and visible dm-thin
    /// volumes whose pool dependencies can be resolved at open time.
    pub fn list_readable_volumes(&self) -> Vec<(usize, LvInfo)> {
        self.logical_volumes
            .iter()
            .enumerate()
            .filter(|(_, lv)| lv.is_directly_mappable() || self.is_supported_thin_volume(lv))
            .map(|(index, lv)| (index, lv_info_from_meta(lv)))
            .collect()
    }

    /// Open a logical volume as a boxed evidence reader.
    ///
    /// Unlike [`Self::open_volume`], this can return alternate reader
    /// implementations such as dm-thin virtual volumes.
    pub fn open_volume_reader(&self, index: usize) -> Result<Box<dyn EvidenceReader>> {
        if index >= self.logical_volumes.len() {
            return Err(LvmError::LvIndexOutOfRange {
                index,
                count: self.logical_volumes.len(),
            });
        }
        let lv = &self.logical_volumes[index];
        if matches!(lv.role, LvRole::ThinVolume) {
            return Ok(Box::new(self.open_thin_volume(index)?));
        }
        Ok(Box::new(self.open_mapped_volume(index)?))
    }

    fn open_thin_volume(&self, index: usize) -> Result<ThinLvReader> {
        let lv = self
            .logical_volumes
            .get(index)
            .ok_or(LvmError::LvIndexOutOfRange {
                index,
                count: self.logical_volumes.len(),
            })?;
        let thin_deps = thin_volume_dependencies(lv)?;
        let device_id = thin_deps.device_id.ok_or_else(|| {
            metadata_error(format!(
                "thin logical volume '{}' is missing device_id",
                lv.name
            ))
        })?;
        let thin_pool_name = thin_deps.thin_pool.as_deref().ok_or_else(|| {
            metadata_error(format!(
                "thin logical volume '{}' is missing thin_pool dependency",
                lv.name
            ))
        })?;
        let thin_pool = self.find_lv_by_name(thin_pool_name).ok_or_else(|| {
            metadata_error(format!(
                "thin logical volume '{}' references unknown thin pool '{}'",
                lv.name, thin_pool_name
            ))
        })?;
        let pool_deps = thin_pool_dependencies(thin_pool)?;
        let metadata_name = pool_deps.metadata.as_deref().ok_or_else(|| {
            metadata_error(format!(
                "thin pool '{}' is missing metadata dependency",
                thin_pool.name
            ))
        })?;
        let data_name = pool_deps.pool.as_deref().ok_or_else(|| {
            metadata_error(format!(
                "thin pool '{}' is missing data dependency",
                thin_pool.name
            ))
        })?;

        let metadata_index = self.find_lv_index_by_name(metadata_name).ok_or_else(|| {
            metadata_error(format!(
                "thin pool '{}' metadata LV '{}' was not found",
                thin_pool.name, metadata_name
            ))
        })?;
        let data_index = self.find_lv_index_by_name(data_name).ok_or_else(|| {
            metadata_error(format!(
                "thin pool '{}' data LV '{}' was not found",
                thin_pool.name, data_name
            ))
        })?;

        let metadata_reader: Box<dyn EvidenceReader> =
            Box::new(self.open_mapped_volume(metadata_index)?);
        let data_reader: Box<dyn EvidenceReader> = Box::new(self.open_mapped_volume(data_index)?);
        let thin_metadata = ThinMetadata::open(metadata_reader)?;
        if let Some(chunk_size) = pool_deps.chunk_size {
            let data_block_size = thin_metadata.superblock().data_block_size_sectors as u64;
            if data_block_size != chunk_size {
                return Err(metadata_error(format!(
                    "thin pool '{}' chunk_size {} does not match metadata data_block_size {}",
                    thin_pool.name, chunk_size, data_block_size
                )));
            }
        }

        ThinLvReader::new(
            thin_metadata,
            data_reader,
            lv.name.clone(),
            lv.size_bytes,
            device_id,
        )
    }

    fn is_supported_thin_volume(&self, lv: &LvMeta) -> bool {
        if !lv.is_visible() || !matches!(lv.role, LvRole::ThinVolume) {
            return false;
        }
        let Ok(thin_deps) = thin_volume_dependencies(lv) else {
            return false;
        };
        let Some(pool_name) = thin_deps.thin_pool.as_deref() else {
            return false;
        };
        let Some(pool_lv) = self.find_lv_by_name(pool_name) else {
            return false;
        };
        let Ok(pool_deps) = thin_pool_dependencies(pool_lv) else {
            return false;
        };
        pool_deps
            .metadata
            .as_deref()
            .is_some_and(|name| self.find_lv_index_by_name(name).is_some())
            && pool_deps
                .pool
                .as_deref()
                .is_some_and(|name| self.find_lv_index_by_name(name).is_some())
            && thin_deps.device_id.is_some()
    }

    fn find_lv_by_name(&self, name: &str) -> Option<&LvMeta> {
        self.logical_volumes.iter().find(|lv| lv.name == name)
    }

    fn find_lv_index_by_name(&self, name: &str) -> Option<usize> {
        self.logical_volumes.iter().position(|lv| lv.name == name)
    }
}

fn thin_volume_dependencies(lv: &LvMeta) -> Result<&SegmentDependencies> {
    if !matches!(lv.role, LvRole::ThinVolume) {
        return Err(metadata_error(format!(
            "logical volume '{}' is not a thin volume",
            lv.name
        )));
    }
    lv.segments
        .iter()
        .find(|segment| {
            segment.dependencies.thin_pool.is_some() && segment.dependencies.device_id.is_some()
        })
        .map(|segment| &segment.dependencies)
        .ok_or_else(|| metadata_error(format!("logical volume '{}' is not a thin volume", lv.name)))
}

fn thin_pool_dependencies(lv: &LvMeta) -> Result<&SegmentDependencies> {
    if !matches!(lv.role, LvRole::ThinPool) {
        return Err(metadata_error(format!(
            "logical volume '{}' is not a thin pool",
            lv.name
        )));
    }
    lv.segments
        .iter()
        .find(|segment| {
            segment.dependencies.metadata.is_some() && segment.dependencies.pool.is_some()
        })
        .map(|segment| &segment.dependencies)
        .ok_or_else(|| metadata_error(format!("logical volume '{}' is not a thin pool", lv.name)))
}

fn metadata_error(message: String) -> LvmError {
    LvmError::MetadataParseError { line: 0, message }
}
