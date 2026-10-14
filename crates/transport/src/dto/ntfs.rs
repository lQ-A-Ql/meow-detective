use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ForensicDataRunDto {
    pub header: u8,
    pub length_field_size: u8,
    pub offset_field_size: u8,
    pub cluster_count: u64,
    pub relative_lcn: Option<i64>,
    pub absolute_lcn: Option<i64>,
    pub logical_offset: u64,
    pub raw: Vec<u8>,
    pub physical_offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NtfsTechnicalAttributeDto {
    pub record_offset: u32,
    pub attribute_type: u32,
    pub name: Option<String>,
    pub instance: u16,
    pub non_resident: bool,
    pub allocated_size: Option<u64>,
    pub real_size: Option<u64>,
    pub initialized_size: Option<u64>,
    pub raw: Vec<u8>,
    pub data_runs: Vec<ForensicDataRunDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NtfsTechnicalFileDto {
    pub inode: u64,
    pub sequence_number: u16,
    pub flags: u16,
    pub parent_reference: Option<u64>,
    pub record_offset: u64,
    pub record_size: u32,
    pub record_raw: Vec<u8>,
    pub record_decoded: Vec<u8>,
    pub source_ranges: Vec<NtfsRecordSourceRangeDto>,
    pub attributes: Vec<NtfsTechnicalAttributeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NtfsRecordSourceRangeDto {
    pub record_offset: u32,
    pub source_offset: u64,
    pub length: u32,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/ntfs.rs"]
mod tests;
