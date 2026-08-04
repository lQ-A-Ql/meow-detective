use domain::DataSourceId;
use uuid::Uuid;

use crate::MountError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MountId(String);

impl MountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, MountError> {
        let value = value.into();
        Uuid::parse_str(&value).map_err(|_| MountError::InvalidPlan("mount id is not a UUID"))?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for MountId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountPlan {
    pub mount_id: MountId,
    pub data_source_id: DataSourceId,
    pub partition_index: usize,
    pub filesystem_kind: String,
    pub source_fingerprint: String,
    pub volume_size: u64,
}

impl MountPlan {
    pub fn new(
        data_source_id: DataSourceId,
        partition_index: usize,
        filesystem_kind: impl Into<String>,
        source_fingerprint: impl Into<String>,
    ) -> Result<Self, MountError> {
        let filesystem_kind = filesystem_kind.into();
        let source_fingerprint = source_fingerprint.into();
        if data_source_id.0.trim().is_empty() {
            return Err(MountError::InvalidPlan("data source id is empty"));
        }
        if filesystem_kind.trim().is_empty() {
            return Err(MountError::InvalidPlan("filesystem kind is empty"));
        }
        if source_fingerprint.trim().is_empty() {
            return Err(MountError::InvalidPlan("source fingerprint is empty"));
        }
        Ok(Self {
            mount_id: MountId::new(),
            data_source_id,
            partition_index,
            filesystem_kind,
            source_fingerprint,
            volume_size: 0,
        })
    }

    pub fn with_volume_size(mut self, volume_size: u64) -> Self {
        self.volume_size = volume_size;
        self
    }
}
