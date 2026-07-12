use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryTransactionOperation {
    CreateKey,
    DeleteKey,
    SetValue,
    DeleteValue,
    RenameKey,
}

#[derive(Debug, Clone)]
pub struct RegistryTransaction {
    pub operation: RegistryTransactionOperation,
    pub key_path: String,
    pub value_name: Option<String>,
    pub data_before: Option<Vec<u8>>,
    pub data_after: Option<Vec<u8>>,
    pub sequence_number: u64,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct TxLogParseResult {
    pub transactions: Vec<RegistryTransaction>,
    pub primary: bool,
    pub warnings: Vec<String>,
}
