use crate::DataSourceId;

/// Auditable relationship between a case source and a nested evidence file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedEvidenceLineage {
    pub parent_data_source_id: DataSourceId,
    pub nested_file_path: String,
    pub derived_data_source_id: Option<DataSourceId>,
    pub offset: u64,
    pub length: u64,
    pub probe_kind: String,
}
