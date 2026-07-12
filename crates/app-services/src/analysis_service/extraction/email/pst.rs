//! PST/OST container extraction.

use super::super::ExtractionOutcome;
use super::shared::build_body_preview;
use crate::analysis_service::artifact_builders::{
    base_attrs, make_artifact, make_timeline_event, string_array_value, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use containers_pst::PstMessage;
use serde_json::Value;
use std::collections::BTreeMap;
use transport::dto::EmailAttachmentDto;

const PST_EXTRACTOR_ID: &str = "email.pst";
const OST_EXTRACTOR_ID: &str = "email.ost";

pub(super) fn extract_pst_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    if let Some(warning) = pst_validation_warning(candidate, bytes) {
        outcome.warnings.push(warning);
        return outcome;
    }
    let mut temp_file = match tempfile::NamedTempFile::with_suffix(".pst") {
        Ok(f) => f,
        Err(err) => {
            outcome.warnings.push(format!(
                "failed to create temp file for {}: {}",
                candidate.path, err
            ));
            return outcome;
        }
    };
    if let Err(err) = std::io::Write::write_all(&mut temp_file, bytes) {
        outcome.warnings.push(format!(
            "failed to write temp file for {}: {}",
            candidate.path, err
        ));
        return outcome;
    }
    let path_lower = candidate.path.to_lowercase();
    let extractor_id = if path_lower.ends_with(".ost") {
        OST_EXTRACTOR_ID
    } else {
        PST_EXTRACTOR_ID
    };
    let messages = read_pst_messages(
        temp_file.path(),
        &candidate.path,
        path_lower.ends_with(".ost"),
        &mut outcome,
    );
    let container_path = container_name(&candidate.path);
    for msg in messages {
        append_pst_message(&mut outcome, candidate, &container_path, extractor_id, msg);
    }
    outcome
}

fn pst_validation_warning(candidate: &EvidenceCandidate, bytes: &[u8]) -> Option<String> {
    if bytes.len() < 4 || &bytes[0..4] != b"!BDN" {
        return Some(format!(
            "PST/OST file {} does not have expected magic bytes",
            candidate.path
        ));
    }
    // PST/OST readers need the complete file. Skip files that exceed the
    // analysis byte budget to avoid loading multi-GB archives into memory.
    if candidate.size > MAX_ANALYSIS_SOURCE_BYTES as u64 {
        return Some(format!(
            "PST/OST file {} ({} bytes) exceeds analysis source byte limit ({}); skipped",
            candidate.path, candidate.size, MAX_ANALYSIS_SOURCE_BYTES
        ));
    }
    // Best-effort encryption detection from the PST header.
    if is_pst_encrypted(bytes) {
        return Some(format!(
            "PST/OST file {} appears to be encrypted; skipping",
            candidate.path
        ));
    }
    None
}

fn read_pst_messages(
    path: &std::path::Path,
    candidate_path: &str,
    is_ost: bool,
    outcome: &mut ExtractionOutcome,
) -> Vec<PstMessage> {
    if is_ost {
        match containers_pst::ost::OstReader::open(path) {
            Ok(reader) => match reader.read_messages() {
                Ok(messages) => messages,
                Err(err) => {
                    outcome.warnings.push(format!(
                        "OST read_messages error for {}: {}",
                        candidate_path, err
                    ));
                    Vec::new()
                }
            },
            Err(err) => {
                outcome
                    .warnings
                    .push(format!("OST open error for {}: {}", candidate_path, err));
                Vec::new()
            }
        }
    } else {
        read_pst_archive(path, candidate_path, outcome)
    }
}

fn read_pst_archive(
    path: &std::path::Path,
    candidate_path: &str,
    outcome: &mut ExtractionOutcome,
) -> Vec<PstMessage> {
    match containers_pst::pst::PstReader::open(path) {
        Ok(reader) => match reader.read_messages() {
            Ok(messages) => messages,
            Err(err) => {
                outcome.warnings.push(format!(
                    "PST read_messages error for {}: {}",
                    candidate_path, err
                ));
                Vec::new()
            }
        },
        Err(err) => {
            outcome
                .warnings
                .push(format!("PST open error for {}: {}", candidate_path, err));
            Vec::new()
        }
    }
}

fn append_pst_message(
    outcome: &mut ExtractionOutcome,
    candidate: &EvidenceCandidate,
    container_path: &str,
    extractor_id: &str,
    msg: PstMessage,
) {
    let from = format_sender(&msg.sender_name, &msg.sender_email);
    let folder_path = folder_path(container_path, &msg.folder_path);
    let attrs = build_pst_attrs(candidate, &folder_path, &msg, &from);
    let event_time = msg.sent_time.or(msg.received_time);
    let title = format!("Email: {}", title_or_url(&msg.subject, &candidate.path));
    if let Some(event_time) = event_time {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "EMAIL_SENT",
            event_time,
            title.clone(),
            from.clone(),
            attrs.clone(),
            extractor_id,
        ));
    }
    outcome.artifacts.push(make_artifact(
        "EmailMessage",
        title,
        from,
        candidate,
        extractor_id,
        attrs,
    ));
}

fn build_pst_attrs(
    candidate: &EvidenceCandidate,
    folder_path: &str,
    msg: &PstMessage,
    from: &str,
) -> BTreeMap<String, Value> {
    let attachments = attachment_names(msg);
    let mut attrs = base_attrs(candidate);
    attrs.insert("from".to_string(), Value::String(from.to_string()));
    attrs.insert("to".to_string(), string_array_value(&msg.to));
    attrs.insert("cc".to_string(), string_array_value(&msg.cc));
    attrs.insert("bcc".to_string(), string_array_value(&msg.bcc));
    attrs.insert("subject".to_string(), Value::String(msg.subject.clone()));
    attrs.insert(
        "messageId".to_string(),
        Value::String(msg.message_id.clone()),
    );
    attrs.insert("attachments".to_string(), string_array_value(&attachments));
    attrs.insert(
        "attachmentDetails".to_string(),
        attachment_details_value(msg),
    );
    attrs.insert("headers".to_string(), headers_value(msg));
    attrs.insert(
        "bodyPreview".to_string(),
        Value::String(build_body_preview(&msg.body_plain)),
    );
    insert_message_content(&mut attrs, msg);
    insert_message_metadata(&mut attrs, msg);
    attrs.insert(
        "containerPath".to_string(),
        Value::String(folder_path.to_string()),
    );
    attrs.insert(
        "attachmentCount".to_string(),
        Value::Number(serde_json::Number::from(attachments.len())),
    );
    attrs.insert(
        "isDeleted".to_string(),
        Value::Bool(is_deleted_folder_path(folder_path)),
    );
    attrs
}

fn insert_message_content(attrs: &mut BTreeMap<String, Value>, msg: &PstMessage) {
    if !msg.body_plain.is_empty() {
        attrs.insert(
            "bodyPlain".to_string(),
            Value::String(msg.body_plain.clone()),
        );
    }
    if !msg.body_html.is_empty() {
        attrs.insert("bodyHtml".to_string(), Value::String(msg.body_html.clone()));
    }
    if let Some(sent_at) = msg.sent_time {
        attrs.insert("sentAt".to_string(), Value::String(sent_at.to_rfc3339()));
    }
    if let Some(received_at) = msg.received_time {
        attrs.insert(
            "receivedAt".to_string(),
            Value::String(received_at.to_rfc3339()),
        );
    }
}

fn insert_message_metadata(attrs: &mut BTreeMap<String, Value>, msg: &PstMessage) {
    if !msg.in_reply_to.is_empty() {
        attrs.insert(
            "inReplyTo".to_string(),
            Value::String(msg.in_reply_to.clone()),
        );
    }
    if !msg.references.is_empty() {
        attrs.insert(
            "references".to_string(),
            string_array_value(&msg.references),
        );
    }
    if !msg.message_class.is_empty() {
        attrs.insert(
            "messageClass".to_string(),
            Value::String(msg.message_class.clone()),
        );
    }
}

fn attachment_names(msg: &PstMessage) -> Vec<String> {
    msg.attachments
        .iter()
        .map(|attachment| attachment.name.clone())
        .filter(|name| !name.is_empty())
        .collect()
}

fn attachment_details_value(msg: &PstMessage) -> Value {
    let details: Vec<EmailAttachmentDto> = msg
        .attachments
        .iter()
        .map(|attachment| EmailAttachmentDto {
            file_name: attachment.name.clone(),
            size: Some(attachment.size),
            mime_type: Some(attachment.mime_type.clone()),
            content_id: attachment.content_id.clone(),
        })
        .collect();
    Value::Array(
        details
            .iter()
            .map(|attachment| {
                serde_json::json!({
                    "fileName": attachment.file_name,
                    "size": attachment.size,
                    "mimeType": attachment.mime_type,
                    "contentId": attachment.content_id,
                })
            })
            .collect(),
    )
}

fn headers_value(msg: &PstMessage) -> Value {
    Value::Array(
        msg.headers
            .iter()
            .map(|(name, value)| serde_json::json!({"name": name, "value": value}))
            .collect(),
    )
}

fn format_sender(sender_name: &str, sender_email: &str) -> String {
    if sender_email.is_empty() {
        sender_name.to_string()
    } else if sender_name.is_empty() {
        sender_email.to_string()
    } else {
        format!("{} <{}>", sender_name, sender_email)
    }
}

fn container_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

fn folder_path(container_path: &str, message_folder_path: &str) -> String {
    if message_folder_path.is_empty() {
        container_path.to_string()
    } else {
        format!("{}/{}", container_path, message_folder_path)
    }
}

fn is_pst_encrypted(bytes: &[u8]) -> bool {
    // PST header offset 0x1CC (bCryptMethod) indicates encryption:
    // 0 = no encryption, 1 = compressible encryption, 2 = high encryption.
    const CRYPT_METHOD_OFFSET: usize = 0x1CC;
    if bytes.len() <= CRYPT_METHOD_OFFSET {
        return false;
    }
    let method = bytes[CRYPT_METHOD_OFFSET];
    method != 0
}

fn is_deleted_folder_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    const DELETED_FOLDERS: &[&str] = &[
        "deleted items",
        "已删除邮件",
        "trash",
        "junk",
        "spam",
        "deleted",
    ];
    DELETED_FOLDERS
        .iter()
        .any(|name| lower.split(['/', '\\']).any(|segment| segment == *name))
}
