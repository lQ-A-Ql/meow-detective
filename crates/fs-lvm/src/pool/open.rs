use super::{lv_info_from_meta, LvInfo, LvmPool};
use crate::error::{LvmError, Result};
use crate::metadata::VolumeGroup;
use crate::{segment, LvReader};

impl LvmPool {
    pub fn list_volumes(&self) -> Vec<LvInfo> {
        self.logical_volumes.iter().map(lv_info_from_meta).collect()
    }

    pub fn list_direct_volumes(&self) -> Vec<(usize, LvInfo)> {
        self.logical_volumes
            .iter()
            .enumerate()
            .filter(|(_, logical_volume)| logical_volume.is_directly_mappable())
            .map(|(index, logical_volume)| (index, lv_info_from_meta(logical_volume)))
            .collect()
    }

    pub fn open_volume(&self, index: usize) -> Result<LvReader> {
        self.open_mapped_volume(index)
    }

    pub(crate) fn open_mapped_volume(&self, index: usize) -> Result<LvReader> {
        let logical_volume =
            self.logical_volumes
                .get(index)
                .ok_or(LvmError::LvIndexOutOfRange {
                    index,
                    count: self.logical_volumes.len(),
                })?;
        let extent_map =
            segment::build_extent_map(&self.volume_group, logical_volume, &self.pv_data_offsets)?;
        Ok(LvReader::new_shared(
            self.device_readers.clone(),
            logical_volume.name.clone(),
            logical_volume.size_bytes,
            extent_map,
        ))
    }

    pub fn volume_group(&self) -> &VolumeGroup {
        &self.volume_group
    }

    pub fn physical_volume_data_offsets(&self) -> &[(String, u64)] {
        &self.pv_data_offsets
    }

    pub fn physical_volume_offsets(&self) -> &[(String, u64)] {
        &self.pv_start_offsets
    }
}
