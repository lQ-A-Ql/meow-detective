use std::collections::HashMap;
use std::sync::Mutex;

use transport::dto::BitLockerDictionaryAttackDto;

#[derive(Default)]
pub struct BitLockerDictionaryAttackRegistry {
    states: Mutex<HashMap<String, BitLockerDictionaryAttackDto>>,
}

impl BitLockerDictionaryAttackRegistry {
    #[must_use]
    pub fn key(case_id: &str, data_source_id: &str, partition_index: u32) -> String {
        format!("{case_id}:{data_source_id}:{partition_index}")
    }

    pub fn get(&self, key: &str) -> Option<BitLockerDictionaryAttackDto> {
        self.states
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(key)
            .cloned()
    }

    pub fn set(&self, key: impl Into<String>, state: BitLockerDictionaryAttackDto) {
        self.states
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(key.into(), state);
    }
}
