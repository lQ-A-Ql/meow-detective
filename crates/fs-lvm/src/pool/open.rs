use super::{lv_info_from_meta, LvInfo, LvmPool};
use crate::error::{LvmError, Result};
use crate::metadata::VolumeGroup;
use crate::{segment, LvReader};

impl LvmPool {
    pub fn metadata_plan(&self) -> super::LvmPoolPlan {
        super::LvmPoolPlan {
            volume_group: self.volume_group.clone(),
            pv_start_offsets: self.pv_start_offsets.clone(),
            pv_data_offsets: self.pv_data_offsets.clone(),
            logical_volumes: self.logical_volumes.clone(),
            warnings: self.warnings.clone(),
        }
    }

    pub fn volume_index(&self, vg_uuid: &str, lv_uuid: &str) -> Result<usize> {
        let normalize = |value: &str| value.replace('-', "").to_ascii_lowercase();
        if vg_uuid.is_empty()
            || lv_uuid.is_empty()
            || normalize(&self.volume_group.id) != normalize(vg_uuid)
        {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: "logical volume group UUID differs from the selected version".into(),
            });
        }
        self.logical_volumes
            .iter()
            .position(|volume| normalize(&volume.uuid) == normalize(lv_uuid))
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "logical volume UUID is absent from the selected version".into(),
            })
    }

    pub fn open_volume_by_uuid(
        &self,
        vg_uuid: &str,
        lv_uuid: &str,
    ) -> Result<Box<dyn evidence_core::EvidenceReader>> {
        self.open_volume_reader(self.volume_index(vg_uuid, lv_uuid)?)
    }

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

    /// Diagnostics recorded during discovery, e.g. additional volume groups
    /// present on the supplied readers that were not selected.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}
