pub const WRITE_BATCH_HEADER_SIZE: usize = 12;
pub const ROCKSDB_MAX_SEQUENCE_NUMBER: u64 = (1u64 << 56) - 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteBatch<'a> {
    pub sequence: u64,
    pub declared_count: u32,
    pub auxiliary_record_count: u32,
    pub auxiliary_records: Vec<WriteBatchAuxiliaryRecord<'a>>,
    pub mutations: Vec<WriteBatchMutation<'a>>,
}

impl WriteBatch<'_> {
    pub fn last_sequence(&self) -> Option<u64> {
        self.mutations.last().map(|mutation| mutation.sequence)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteBatchAuxiliaryRecord<'a> {
    pub offset: usize,
    pub kind: WriteBatchAuxiliaryKind<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteBatchAuxiliaryKind<'a> {
    LogData { data: &'a [u8] },
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteBatchMutation<'a> {
    pub sequence: u64,
    pub column_family_id: u32,
    pub key: &'a [u8],
    pub kind: WriteBatchMutationKind<'a>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteBatchMutationKind<'a> {
    Put { value: &'a [u8] },
    Delete,
    SingleDelete,
    Merge { operand: &'a [u8] },
    DeleteRange { end_key: &'a [u8] },
}
