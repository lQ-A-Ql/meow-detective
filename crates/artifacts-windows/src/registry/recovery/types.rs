use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FreeCell {
    pub size: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredKey {
    pub key_name: String,
    pub last_written: Option<DateTime<Utc>>,
    pub num_values: u32,
    pub cell_offset: u32,
    pub parent_path_hint: String,
    pub confidence: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredValue {
    pub value_name: String,
    pub value_type: u32,
    pub value_data_preview: String,
    pub key_path_hint: String,
    pub confidence: &'static str,
}

#[derive(Debug, Clone)]
pub struct RecoverResult {
    pub recovered_keys: Vec<RecoveredKey>,
    pub recovered_values: Vec<RecoveredValue>,
    pub free_cells_scanned: u32,
}
