use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BitLockerDictionaryBackendDto {
    Cpu,
    Gpu,
}

/// Progress and terminal state for a BitLocker password dictionary attempt.
///
/// Candidate passwords are intentionally absent.  The matching credential is
/// consumed by the backend's verified-unlock path and is never returned over
/// IPC, written to a job row, or emitted in an event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitLockerDictionaryAttackDto {
    pub task_id: String,
    /// `queued`, `running`, `cancelling`, `found`, `exhausted`, `cancelled`, or `failed`.
    pub phase: String,
    pub backend: BitLockerDictionaryBackendDto,
    pub tested_candidates: u64,
    pub bytes_processed: u64,
    pub total_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/bitlocker_dictionary.rs"]
mod tests;
