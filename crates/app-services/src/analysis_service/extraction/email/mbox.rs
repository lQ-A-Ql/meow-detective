//! mbox container extraction.

use super::super::ExtractionOutcome;
use super::shared::build_body_preview;
use crate::analysis_service::artifact_builders::{
    base_attrs, make_artifact, make_timeline_event, string_array_value, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use containers_pst::MboxMessage;
use serde_json::Value;
use std::collections::BTreeMap;
use transport::dto::EmailAttachmentDto;

const MBOX_EXTRACTOR_ID: &str = "email.mbox";

pub(super) fn extract_mbox_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let messages = match containers_pst::mbox::parse_mbox(bytes) {
        Ok(msgs) => msgs,
        Err(err) => {
            outcome
                .warnings
                .push(format!("mbox parse error for {}: {}", candidate.path, err));
            return outcome;
        }
    };
    let container_path = container_name(&candidate.path);
    for msg in messages {
        append_mbox_message(&mut outcome, candidate, &container_path, msg);
    }
    outcome
}

fn append_mbox_message(
    outcome: &mut ExtractionOutcome,
    candidate: &EvidenceCandidate,
    container_path: &str,
    msg: MboxMessage,
) {
    let from = format_sender(&msg.sender_name, &msg.sender_email);
    let attrs = build_mbox_attrs(candidate, container_path, &msg, &from);
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
            MBOX_EXTRACTOR_ID,
        ));
    }
    outcome.artifacts.push(make_artifact(
        "EmailMessage",
        title,
        from,
        candidate,
        MBOX_EXTRACTOR_ID,
        attrs,
    ));
}

fn build_mbox_attrs(
    candidate: &EvidenceCandidate,
    container_path: &str,
    msg: &MboxMessage,
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
        Value::String(container_path.to_string()),
    );
    attrs.insert(
        "attachmentCount".to_string(),
        Value::Number(serde_json::Number::from(attachments.len())),
    );
    attrs.insert("isDeleted".to_string(), Value::Null);
    attrs
}

fn insert_message_content(attrs: &mut BTreeMap<String, Value>, msg: &MboxMessage) {
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

fn insert_message_metadata(attrs: &mut BTreeMap<String, Value>, msg: &MboxMessage) {
    insert_non_empty(attrs, "replyTo", &msg.reply_to);
    insert_non_empty(attrs, "returnPath", &msg.return_path);
    insert_non_empty(attrs, "inReplyTo", &msg.in_reply_to);
    if !msg.references.is_empty() {
        attrs.insert(
            "references".to_string(),
            string_array_value(&msg.references),
        );
    }
    insert_non_empty(attrs, "messageClass", &msg.message_class);
    insert_non_empty(attrs, "xMailer", &msg.x_mailer);
    insert_non_empty(attrs, "xOriginatingIp", &msg.x_originating_ip);
}

fn insert_non_empty(attrs: &mut BTreeMap<String, Value>, key: &str, value: &str) {
    if !value.is_empty() {
        attrs.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn attachment_names(msg: &MboxMessage) -> Vec<String> {
    msg.attachments
        .iter()
        .map(|attachment| attachment.name.clone())
        .filter(|name| !name.is_empty())
        .collect()
}

fn attachment_details_value(msg: &MboxMessage) -> Value {
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

fn headers_value(msg: &MboxMessage) -> Value {
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
