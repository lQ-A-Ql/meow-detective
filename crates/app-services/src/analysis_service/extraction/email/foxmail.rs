//! Foxmail native `.box` extraction.

use super::super::ExtractionOutcome;
use super::eml::{extract_parsed_email, parse_email_message};
use crate::analysis_service::artifact_builders::{
    base_attrs, make_artifact, make_timeline_event, string_array_value, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use containers_pst::foxmail::{
    parse_account_info, parse_account_password, parse_box, parse_index, FoxmailAccountInfo,
    FoxmailIndexRecord,
};
use serde_json::Value;

const FOXMAIL_EXTRACTOR_ID: &str = "email.foxmail_box";
const FOXMAIL_INDEX_EXTRACTOR_ID: &str = "email.foxmail_index";
const FOXMAIL_ACCOUNT_EXTRACTOR_ID: &str = "email.foxmail_account";

pub(super) fn extract_foxmail_account_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let Some(info) = parse_account_info(bytes) else {
        outcome.warnings.push(format!(
            "Foxmail account {} has no recognized metadata",
            candidate.path
        ));
        return outcome;
    };
    let mut attrs = base_attrs(candidate);
    insert_account_info_attrs(&mut attrs, &info);
    attrs.insert(
        "mailClient".to_string(),
        Value::String("Foxmail".to_string()),
    );
    attrs.insert(
        "accountPath".to_string(),
        Value::String(candidate.path.clone()),
    );
    attrs.insert(
        "passwordAlgorithm".to_string(),
        Value::String(account_algorithm(bytes).to_string()),
    );
    match parse_account_password(bytes) {
        Ok(password) => {
            attrs.insert("password".to_string(), Value::String(password.password));
            attrs.insert(
                "passwordStatus".to_string(),
                Value::String("recovered".to_string()),
            );
        }
        Err(error) => {
            attrs.insert(
                "passwordStatus".to_string(),
                Value::String("unavailable".to_string()),
            );
            outcome.warnings.push(format!(
                "Foxmail account {} password could not be decoded: {}",
                candidate.path, error
            ));
        }
    }
    let account_name = info
        .account
        .as_deref()
        .or(info.email.as_deref())
        .unwrap_or("unknown");
    outcome.artifacts.push(make_artifact(
        "EmailAccount",
        format!("Foxmail account: {account_name}"),
        account_name.to_string(),
        candidate,
        FOXMAIL_ACCOUNT_EXTRACTOR_ID,
        attrs,
    ));
    outcome
}

fn insert_account_info_attrs(
    attrs: &mut std::collections::BTreeMap<String, Value>,
    info: &FoxmailAccountInfo,
) {
    insert_optional_string(attrs, "account", info.account.as_deref());
    insert_optional_string(attrs, "email", info.email.as_deref());
    insert_optional_string(attrs, "incomingServer", info.incoming_server.as_deref());
    insert_optional_string(attrs, "outgoingServer", info.outgoing_server.as_deref());
    if let Some(port) = info.incoming_port {
        attrs.insert("incomingPort".to_string(), Value::Number(port.into()));
    }
    if let Some(port) = info.outgoing_port {
        attrs.insert("outgoingPort".to_string(), Value::Number(port.into()));
    }
    if let Some(ssl) = info.incoming_ssl {
        attrs.insert("incomingSsl".to_string(), Value::Bool(ssl));
    }
    if let Some(ssl) = info.outgoing_ssl {
        attrs.insert("outgoingSsl".to_string(), Value::Bool(ssl));
    }
}

fn insert_optional_string(
    attrs: &mut std::collections::BTreeMap<String, Value>,
    key: &str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        attrs.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn account_algorithm(bytes: &[u8]) -> &'static str {
    match bytes.first().copied() {
        Some(0x52) => "foxmailV7",
        Some(0xd0) => "foxmailV6",
        _ => "unknown",
    }
}

pub(super) fn extract_foxmail_index_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let parsed = parse_index(bytes);
    let mut outcome = ExtractionOutcome::default();
    if parsed.records.is_empty() {
        if parsed.invalid_record_count > 0 {
            outcome.warnings.push(format!(
                "Foxmail index {} contains {} unrecognized record(s)",
                candidate.path, parsed.invalid_record_count
            ));
        }
        return outcome;
    }
    if parsed.invalid_record_count > 0 {
        outcome.warnings.push(format!(
            "Foxmail index {} skipped {} unrecognized record(s)",
            candidate.path, parsed.invalid_record_count
        ));
    }
    for record in parsed.records {
        append_foxmail_index_record(&mut outcome, candidate, record);
    }
    outcome.warnings.push(format!(
        "Foxmail index {} yielded metadata only; message bodies remain in private binary mail files",
        candidate.path
    ));
    outcome
}

fn append_foxmail_index_record(
    outcome: &mut ExtractionOutcome,
    candidate: &EvidenceCandidate,
    record: FoxmailIndexRecord,
) {
    let from = format_address(&record.from_name, &record.from_email);
    let to = format_address(&record.to_name, &record.to_email);
    let mut attrs = base_attrs(candidate);
    attrs.insert("from".to_string(), Value::String(from.clone()));
    attrs.insert("to".to_string(), string_array_value(&[to]));
    attrs.insert("cc".to_string(), Value::Array(Vec::new()));
    attrs.insert("bcc".to_string(), Value::Array(Vec::new()));
    attrs.insert("subject".to_string(), Value::String(record.subject.clone()));
    attrs.insert("messageId".to_string(), Value::String(String::new()));
    attrs.insert("attachments".to_string(), Value::Array(Vec::new()));
    attrs.insert("attachmentDetails".to_string(), Value::Array(Vec::new()));
    attrs.insert("headers".to_string(), Value::Array(Vec::new()));
    attrs.insert("bodyPreview".to_string(), Value::String(String::new()));
    attrs.insert(
        "mailClient".to_string(),
        Value::String("Foxmail".to_string()),
    );
    attrs.insert(
        "containerPath".to_string(),
        Value::String(candidate.path.clone()),
    );
    attrs.insert("messageIndex".to_string(), Value::Number(record.id.into()));
    attrs.insert(
        "contentStatus".to_string(),
        Value::String("metadataOnly".to_string()),
    );
    attrs.insert(
        "messageIdSource".to_string(),
        Value::String("foxmailIndexRecord".to_string()),
    );
    attrs.insert("attachmentCount".to_string(), Value::Number(0.into()));
    attrs.insert("isDeleted".to_string(), Value::Null);
    if let Some(timestamp) = record.primary_time {
        attrs.insert("sentAt".to_string(), Value::String(timestamp.to_rfc3339()));
    }
    if let Some(timestamp) = record.secondary_time {
        attrs.insert(
            "receivedAt".to_string(),
            Value::String(timestamp.to_rfc3339()),
        );
    }
    let title = format!("Email: {}", title_or_url(&record.subject, &candidate.path));
    if let Some(timestamp) = record.primary_time.or(record.secondary_time) {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "EMAIL_SENT",
            timestamp,
            title.clone(),
            from.clone(),
            attrs.clone(),
            FOXMAIL_INDEX_EXTRACTOR_ID,
        ));
    }
    outcome.artifacts.push(make_artifact(
        "EmailMessage",
        title,
        from,
        candidate,
        FOXMAIL_INDEX_EXTRACTOR_ID,
        attrs,
    ));
}

fn format_address(name: &str, address: &str) -> String {
    if name.is_empty() || name == address {
        address.to_string()
    } else if address.is_empty() {
        name.to_string()
    } else {
        format!("{name} <{address}>")
    }
}

pub(super) fn extract_foxmail_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let parsed_box = parse_box(bytes);
    let mut outcome = ExtractionOutcome::default();
    if parsed_box.separator_count == 0 {
        if containers_pst::foxmail::is_list_container(bytes) {
            return outcome;
        }
        outcome.warnings.push(format!(
            "Foxmail .box file {} has no recognized record separator",
            candidate.path
        ));
        return outcome;
    }
    if parsed_box.ignored_record_count > 0 {
        outcome.warnings.push(format!(
            "Foxmail .box file {} skipped {} unrecognized record(s)",
            candidate.path, parsed_box.ignored_record_count
        ));
    }
    for (index, record) in parsed_box.records.into_iter().enumerate() {
        match parse_email_message(&record) {
            Ok(parsed) => {
                let mut parsed_outcome =
                    extract_parsed_email(candidate, parsed, FOXMAIL_EXTRACTOR_ID);
                add_foxmail_metadata(&mut parsed_outcome, candidate, index as u64 + 1);
                outcome.artifacts.extend(parsed_outcome.artifacts);
                outcome
                    .timeline_events
                    .extend(parsed_outcome.timeline_events);
                outcome.warnings.extend(parsed_outcome.warnings);
            }
            Err(error) => outcome.warnings.push(format!(
                "Foxmail .box record {} in {} could not be parsed as RFC822: {}",
                index + 1,
                candidate.path,
                error
            )),
        }
    }
    outcome
}

fn add_foxmail_metadata(
    outcome: &mut ExtractionOutcome,
    candidate: &EvidenceCandidate,
    message_index: u64,
) {
    for artifact in &mut outcome.artifacts {
        artifact.attrs.insert(
            "mailClient".to_string(),
            Value::String("Foxmail".to_string()),
        );
        artifact.attrs.insert(
            "containerPath".to_string(),
            Value::String(candidate.path.clone()),
        );
        artifact.attrs.insert(
            "messageIndex".to_string(),
            Value::Number(message_index.into()),
        );
    }
    for event in &mut outcome.timeline_events {
        event.attrs.insert(
            "mailClient".to_string(),
            Value::String("Foxmail".to_string()),
        );
        event.attrs.insert(
            "containerPath".to_string(),
            Value::String(candidate.path.clone()),
        );
        event.attrs.insert(
            "messageIndex".to_string(),
            Value::Number(message_index.into()),
        );
    }
}
