use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use transport::dto::*;

fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../frontend/mocks/registry-browser/contract.json");
    serde_json::from_slice(&std::fs::read(path).expect("synthetic cross-language fixture")).unwrap()
}

fn roundtrip<T: DeserializeOwned + Serialize>(value: &Value) {
    let decoded: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), *value);
}

#[test]
fn browser_requests_keep_the_camel_case_wire_contract() {
    let wire = fixture();
    roundtrip::<RegistryHiveListRequestDto>(&wire["hiveListRequest"]);
    roundtrip::<RegistryHiveOpenRequestDto>(&wire["hiveOpenRequest"]);
    roundtrip::<RegistryHiveCloseRequestDto>(&wire["hiveCloseRequest"]);
    roundtrip::<RegistryKeyResolveRequestDto>(&wire["keyResolveRequest"]);
    roundtrip::<RegistrySubkeysRequestDto>(&wire["subkeysRequest"]);
    roundtrip::<RegistryValuesRequestDto>(&wire["valuesRequest"]);
    roundtrip::<RegistryValueRangeRequestDto>(&wire["rangeRequest"]);
}

#[test]
fn browser_responses_preserve_precision_default_names_and_coverage() {
    let wire = fixture();
    roundtrip::<RegistryHivePageDto>(&wire["hivePage"]);
    roundtrip::<RegistryHiveSessionDto>(&wire["session"]);
    roundtrip::<RegistrySubkeyPageDto>(&wire["subkeyPage"]);
    roundtrip::<RegistryValuePageDto>(&wire["valuePage"]);
    roundtrip::<RegistryValueRangeDto>(&wire["range"]);
    assert_eq!(wire["range"]["decodedText"], "18446744073709551615");
    assert_eq!(
        wire["session"]["rootKey"]["lastWriteFiletime"],
        "133700000000000001"
    );
    assert_eq!(wire["valuePage"]["values"][0]["name"], "");
    assert_eq!(wire["valuePage"]["coverage"]["status"], "partial");
    assert!(wire["hivePage"].get("nextCursor").is_none());
}

#[test]
fn read_state_enums_are_stable_and_view_modes_are_explicit() {
    for state in ["candidate", "readable", "locked", "unsupported", "corrupt"] {
        roundtrip::<RegistryHiveReadStateDto>(&Value::String(state.to_string()));
    }
    for state in ["readable", "deferred", "unsupported", "corrupt"] {
        roundtrip::<RegistryItemReadStateDto>(&Value::String(state.to_string()));
    }
    for state in ["decoded", "rawOnly", "partial"] {
        roundtrip::<RegistryValueDecodeStatusDto>(&Value::String(state.to_string()));
    }
    assert!(serde_json::from_value::<RegistryHiveViewDto>(Value::String("merged".into())).is_err());
    roundtrip::<RegistryHiveViewDto>(&Value::String("recovered".into()));
}

#[test]
fn recovery_provenance_matches_shared_synthetic_wire_fixture() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../frontend/mocks/registry-browser/recovery-contract.json");
    let value: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    roundtrip::<RegistryRecoveryDto>(&value);
    roundtrip::<RegistryLogProvenanceDto>(&value["logs"][0]);
    for state in ["notNeeded", "recovered", "partial", "failed"] {
        roundtrip::<RegistryRecoveryStatusDto>(&Value::String(state.into()));
    }
    for state in [
        "applied",
        "notNeeded",
        "stale",
        "invalid",
        "unavailable",
        "duplicate",
        "notSelected",
        "missing",
        "ambiguous",
        "locked",
        "tooLarge",
    ] {
        roundtrip::<RegistryLogStatusDto>(&Value::String(state.into()));
    }
    for format in ["hvle", "dirt"] {
        roundtrip::<RegistryLogFormatDto>(&Value::String(format.into()));
    }
    for slot in ["log1", "log2"] {
        roundtrip::<RegistryLogSlotDto>(&Value::String(slot.into()));
    }
}
