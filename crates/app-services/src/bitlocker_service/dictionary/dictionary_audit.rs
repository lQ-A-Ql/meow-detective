use super::super::{
    audit::{self, BitLockerAudit},
    dictionary_identity::DictionaryIdentity,
    source::{open_partition_window, open_source_read_only},
    BitLockerServiceError, DictionaryAttackOutcome, DictionaryAttackRequest,
};
use transport::ServiceErrorCategory;
use volume_bitlocker::{read_volume_identities, MetadataFingerprint};

pub(crate) fn record_dictionary_outcome(
    request: &DictionaryAttackRequest<'_>,
    result: &Result<DictionaryAttackOutcome, BitLockerServiceError>,
    identity: Option<&DictionaryIdentity>,
    tested_candidates: u64,
) {
    let (outcome, code) = match result {
        Ok(DictionaryAttackOutcome::Found { .. }) => ("success", None),
        Ok(DictionaryAttackOutcome::Exhausted { .. }) => ("exhausted", None),
        Ok(DictionaryAttackOutcome::Cancelled { .. }) => ("cancelled", None),
        Err(error) => ("failed", error.code()),
    };
    let fingerprint = read_dictionary_fingerprint(request);
    let tested_candidates = dictionary_tested_candidates(result, tested_candidates);
    let mut extra = dictionary_input_details(identity, tested_candidates);
    extra["backend"] = serde_json::json!(request.backend);
    audit::record(
        request.case_conn,
        BitLockerAudit {
            case_id: &request.case_id.0,
            data_source_id: &request.data_source_id.0,
            partition_index: request.partition_index,
            metadata_fingerprint: fingerprint.as_deref(),
            operation: "passwordDictionary",
            outcome,
            error_code: code,
            extra_details: Some(&extra),
        },
    );
}

pub(crate) fn dictionary_input_details(
    identity: Option<&DictionaryIdentity>,
    tested_candidates: u64,
) -> serde_json::Value {
    let mut details = serde_json::Map::new();
    details.insert(
        "testedCandidates".to_string(),
        serde_json::json!(tested_candidates),
    );
    if let Some(identity) = identity {
        details.insert(
            "dictionarySize".to_string(),
            serde_json::json!(identity.size),
        );
        details.insert(
            "dictionarySha256".to_string(),
            serde_json::json!(identity.sha256),
        );
    }
    serde_json::Value::Object(details)
}

fn dictionary_tested_candidates(
    result: &Result<DictionaryAttackOutcome, BitLockerServiceError>,
    fallback: u64,
) -> u64 {
    match result {
        Ok(DictionaryAttackOutcome::Found { progress })
        | Ok(DictionaryAttackOutcome::Exhausted { progress })
        | Ok(DictionaryAttackOutcome::Cancelled { progress }) => progress.tested_candidates,
        Err(_) => fallback,
    }
}

fn read_dictionary_fingerprint(request: &DictionaryAttackRequest<'_>) -> Option<String> {
    let source = open_source_read_only(
        request.case_conn,
        request.case_root,
        request.case_id,
        request.data_source_id,
        request.partition_index,
    )
    .ok()?;
    let mut window = open_partition_window(&source).ok()?;
    let identities = read_volume_identities(&mut window).ok()?;
    identities.first().map(|identity| {
        MetadataFingerprint::from_metadata(&identity.metadata)
            .as_str()
            .to_string()
    })
}
