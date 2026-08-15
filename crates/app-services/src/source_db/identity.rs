use domain::{DataSourceId, FileEntryId};
use persistence_sqlite::{DbError, DbResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalFileId {
    pub data_source_id: DataSourceId,
    pub local_id: FileEntryId,
}

impl GlobalFileId {
    pub fn new(data_source_id: DataSourceId, local_id: FileEntryId) -> Self {
        Self {
            data_source_id,
            local_id,
        }
    }

    pub fn encode(&self) -> FileEntryId {
        FileEntryId(encode_source_scoped_id(
            &self.data_source_id,
            &self.local_id.0,
        ))
    }

    pub fn parse(value: &FileEntryId) -> DbResult<Self> {
        let (data_source_id, local_id) = parse_source_scoped_id("File id", &value.0)?;
        Ok(Self::new(data_source_id, FileEntryId(local_id)))
    }
}

pub fn encode_source_scoped_id(data_source_id: &DataSourceId, local_id: &str) -> String {
    format!("ds:{}:{}", data_source_id.0, local_id)
}

pub fn parse_source_scoped_id(label: &str, value: &str) -> DbResult<(DataSourceId, String)> {
    let Some(rest) = value.strip_prefix("ds:") else {
        return Err(DbError::System(format!(
            "{label} '{}' is not a source-scoped id",
            value
        )));
    };
    let Some((data_source_id, local_id)) = rest.split_once(':') else {
        return Err(DbError::System(format!(
            "{label} '{}' is missing source or local id",
            value
        )));
    };
    if data_source_id.is_empty() || local_id.is_empty() {
        return Err(DbError::System(format!(
            "{label} '{}' contains an empty source or local id",
            value
        )));
    }
    if !is_safe_data_source_id(data_source_id) {
        return Err(DbError::System(format!(
            "{label} '{}' contains an invalid source id",
            value
        )));
    }
    Ok((
        DataSourceId(data_source_id.to_string()),
        local_id.to_string(),
    ))
}

pub(super) fn is_safe_data_source_id(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}
