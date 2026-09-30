use super::*;

#[test]
fn dictionary_status_uses_camel_case_and_has_no_credential() {
    let dto = BitLockerDictionaryAttackDto {
        task_id: "bitlocker-dictionary:case:source:0".to_string(),
        phase: "running".to_string(),
        backend: BitLockerDictionaryBackendDto::Gpu,
        tested_candidates: 3,
        bytes_processed: 24,
        total_bytes: 100,
        error: None,
    };
    let value = serde_json::to_value(&dto).expect("status serializes");
    assert_eq!(value["taskId"], dto.task_id);
    assert_eq!(value["testedCandidates"], 3);
    assert_eq!(value["backend"], "gpu");
    assert_eq!(
        serde_json::from_value::<BitLockerDictionaryAttackDto>(value.clone()).expect("roundtrip"),
        dto
    );
    assert!(serde_json::from_str::<BitLockerDictionaryBackendDto>("\"cudaOnly\"").is_err());
    assert!(value.get("password").is_none());
    assert!(value.get("credential").is_none());
}
