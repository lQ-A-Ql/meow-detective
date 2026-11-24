use crate::{FileEntryRecord, Result, SnapshotEntry, TimelineRecord, TimelineRecordStore};

pub trait NativeWriteInput: Send {
    fn budget_bytes(&self) -> Result<usize>;
}

impl NativeWriteInput for SnapshotEntry {
    fn budget_bytes(&self) -> Result<usize> {
        Ok(std::mem::size_of::<Self>() + self.key.capacity() + self.value.capacity())
    }
}

impl NativeWriteInput for TimelineRecord {
    fn budget_bytes(&self) -> Result<usize> {
        let cost = TimelineRecordStore::record_storage_cost(self)?;
        Ok(usize::try_from(cost.write_bytes)
            .unwrap_or(usize::MAX)
            .saturating_add(std::mem::size_of::<Self>()))
    }
}

impl NativeWriteInput for u64 {
    fn budget_bytes(&self) -> Result<usize> {
        Ok(std::mem::size_of::<Self>())
    }
}

impl NativeWriteInput for FileEntryRecord {
    fn budget_bytes(&self) -> Result<usize> {
        Ok(std::mem::size_of::<Self>()
            + crate::catalog::encoded_record_bytes(self)?
            + self.id.capacity()
            + self.data_source_id.capacity()
            + self.parent_id.as_ref().map_or(0, String::capacity)
            + self.path.capacity()
            + self.name.capacity()
            + self.ext.as_ref().map_or(0, String::capacity)
            + self.hash_sha256.as_ref().map_or(0, String::capacity))
    }
}
