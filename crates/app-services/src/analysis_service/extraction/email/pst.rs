//! PST/OST container extraction.

use super::super::ExtractionOutcome;
use super::shared::build_body_preview;
use crate::analysis_service::artifact_builders::{
    base_attrs, make_artifact, make_timeline_event, string_array_value, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use serde_json::Value;
use transport::dto::EmailAttachmentDto;

const PST_EXTRACTOR_ID: &str = "email.pst";
const OST_EXTRACTOR_ID: &str = "email.ost";

pub(super) fn extract_pst_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();

    if bytes.len() < 4 || &bytes[0..4] != b"!BDN" {
        outcome.warnings.push(format!(
            "PST/OST file {} does not have expected magic bytes",
            candidate.path
        ));
        return outcome;
    }

    // PST/OST readers need the complete file. Skip files that exceed the
    // analysis byte budget to avoid loading multi-GB archives into memory.
    if candidate.size > MAX_ANALYSIS_SOURCE_BYTES as u64 {
        outcome.warnings.push(format!(
            "PST/OST file {} ({} bytes) exceeds analysis source byte limit ({}); skipped",
            candidate.path, candidate.size, MAX_ANALYSIS_SOURCE_BYTES
        ));
        return outcome;
    }

    // Best-effort encryption detection from the PST header.
    if is_pst_encrypted(bytes) {
        outcome.warnings.push(format!(
            "PST/OST file {} appears to be encrypted; skipping",
            candidate.path
        ));
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

    let path = temp_file.path().to_path_buf();
    let path_lower = candidate.path.to_lowercase();
    let extractor_id = if path_lower.ends_with(".ost") {
        OST_EXTRACTOR_ID
    } else {
        PST_EXTRACTOR_ID
    };

    let messages: Vec<containers_pst::PstMessage> = if path_lower.ends_with(".ost") {
        match containers_pst::ost::OstReader::open(&path) {
            Ok(reader) => match reader.read_messages() {
                Ok(msgs) => msgs,
                Err(err) => {
                    outcome.warnings.push(format!(
                        "OST read_messages error for {}: {}",
                        candidate.path, err
                    ));
                    Vec::new()
                }
            },
            Err(err) => {
                outcome
                    .warnings
                    .push(format!("OST open error for {}: {}", candidate.path, err));
                Vec::new()
            }
        }
    } else {
        match containers_pst::pst::PstReader::open(&path) {
            Ok(reader) => match reader.read_messages() {
                Ok(msgs) => msgs,
                Err(err) => {
                    outcome.warnings.push(format!(
                        "PST read_messages error for {}: {}",
                        candidate.path, err
                    ));
                    Vec::new()
                }
            },
            Err(err) => {
                outcome
                    .warnings
                    .push(format!("PST open error for {}: {}", candidate.path, err));
                Vec::new()
            }
        }
    };

    let container_path = std::path::Path::new(&candidate.path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&candidate.path)
        .to_string();

    for msg in messages {
        let from = if msg.sender_email.is_empty() {
            msg.sender_name.clone()
        } else if msg.sender_name.is_empty() {
            msg.sender_email.clone()
        } else {
            format!("{} <{}>", msg.sender_name, msg.sender_email)
        };
        let to = msg.to.clone();
        let cc = msg.cc.clone();
        let bcc = msg.bcc.clone();
        let subject = msg.subject.clone();
        let sent_at = msg.sent_time;
        let received_at = msg.received_time;
        let body_preview = build_body_preview(&msg.body_plain);
        let attachments: Vec<String> = msg
            .attachments
            .iter()
            .map(|a| a.name.clone())
            .filter(|n| !n.is_empty())
            .collect();
        let attachment_details: Vec<EmailAttachmentDto> = msg
            .attachments
            .iter()
            .map(|a| EmailAttachmentDto {
                file_name: a.name.clone(),
                size: Some(a.size),
                mime_type: Some(a.mime_type.clone()),
                content_id: a.content_id.clone(),
            })
            .collect();
        let folder_path = if msg.folder_path.is_empty() {
            container_path.clone()
        } else {
            format!("{}/{}", container_path, msg.folder_path)
        };

        let mut attrs = base_attrs(candidate);
        attrs.insert("from".to_string(), Value::String(from.clone()));
        attrs.insert("to".to_string(), string_array_value(&to));
        attrs.insert("cc".to_string(), string_array_value(&cc));
        attrs.insert("bcc".to_string(), string_array_value(&bcc));
        attrs.insert("subject".to_string(), Value::String(subject.clone()));
        attrs.insert(
            "messageId".to_string(),
            Value::String(msg.message_id.clone()),
        );
        attrs.insert("attachments".to_string(), string_array_value(&attachments));
        attrs.insert(
            "attachmentDetails".to_string(),
            Value::Array(
                attachment_details
                    .iter()
                    .map(|a| {
                        serde_json::json!({
                            "fileName": a.file_name,
                            "size": a.size,
                            "mimeType": a.mime_type,
                            "contentId": a.content_id,
                        })
                    })
                    .collect(),
            ),
        );
        attrs.insert(
            "headers".to_string(),
            Value::Array(
                msg.headers
                    .iter()
                    .map(|(name, value)| serde_json::json!({"name": name, "value": value}))
                    .collect(),
            ),
        );
        attrs.insert(
            "bodyPreview".to_string(),
            Value::String(body_preview.clone()),
        );
        if !msg.body_plain.is_empty() {
            attrs.insert(
                "bodyPlain".to_string(),
                Value::String(msg.body_plain.clone()),
            );
        }
        if !msg.body_html.is_empty() {
            attrs.insert("bodyHtml".to_string(), Value::String(msg.body_html.clone()));
        }
        if let Some(sent_at) = sent_at {
            attrs.insert("sentAt".to_string(), Value::String(sent_at.to_rfc3339()));
        }
        if let Some(received_at) = received_at {
            attrs.insert(
                "receivedAt".to_string(),
                Value::String(received_at.to_rfc3339()),
            );
        }
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
        attrs.insert(
            "containerPath".to_string(),
            Value::String(folder_path.clone()),
        );
        if !msg.message_class.is_empty() {
            attrs.insert(
                "messageClass".to_string(),
                Value::String(msg.message_class.clone()),
            );
        }
        attrs.insert(
            "attachmentCount".to_string(),
            Value::Number(serde_json::Number::from(attachments.len())),
        );
        let is_deleted = is_deleted_folder_path(&folder_path);
        attrs.insert("isDeleted".to_string(), Value::Bool(is_deleted));

        let event_time = sent_at.or(received_at);
        if let Some(event_time) = event_time {
            outcome.timeline_events.push(make_timeline_event(
                &candidate.file_id,
                "EMAIL_SENT",
                event_time,
                format!("Email: {}", title_or_url(&subject, &candidate.path)),
                from.clone(),
                attrs.clone(),
                extractor_id,
            ));
        }
        outcome.artifacts.push(make_artifact(
            "EmailMessage",
            format!("Email: {}", title_or_url(&subject, &candidate.path)),
            from,
            candidate,
            extractor_id,
            attrs,
        ));
    }

    outcome
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
