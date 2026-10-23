use super::super::shared::identify_mail_client;
use super::parser::ParsedEmail;
use crate::analysis_service::artifact_builders::{
    base_attrs, make_artifact, make_timeline_event, string_array_value, title_or_url,
};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::ExtractionOutcome;
use serde_json::Value;
use std::collections::BTreeMap;
use transport::dto::{EmailAttachmentDto, EmailHeaderDto};

const EMAIL_EXTRACTOR_ID: &str = "email.eml_emlx";

pub(crate) fn extract_eml_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let parsed = match super::parser::parse_email_message(bytes) {
        Ok(parsed) => parsed,
        Err(error) => {
            let mut outcome = ExtractionOutcome::default();
            outcome
                .warnings
                .push(format!("EML parse error for {}: {}", candidate.path, error));
            return outcome;
        }
    };
    extract_parsed_email(candidate, parsed, EMAIL_EXTRACTOR_ID)
}

pub(crate) fn extract_parsed_email(
    candidate: &EvidenceCandidate,
    parsed: ParsedEmail,
    extractor_id: &str,
) -> ExtractionOutcome {
    let attrs = build_eml_attrs(candidate, &parsed);
    build_eml_outcome(candidate, parsed, attrs, extractor_id)
}

fn build_eml_attrs(candidate: &EvidenceCandidate, parsed: &ParsedEmail) -> BTreeMap<String, Value> {
    let mut attrs = base_attrs(candidate);
    attrs.insert("from".to_string(), Value::String(parsed.from.clone()));
    attrs.insert("to".to_string(), string_array_value(&parsed.to));
    attrs.insert("cc".to_string(), string_array_value(&parsed.cc));
    attrs.insert("bcc".to_string(), string_array_value(&parsed.bcc));
    attrs.insert("subject".to_string(), Value::String(parsed.subject.clone()));
    attrs.insert(
        "messageId".to_string(),
        Value::String(parsed.message_id.clone()),
    );
    attrs.insert(
        "attachments".to_string(),
        string_array_value(&parsed.attachments),
    );
    attrs.insert(
        "attachmentDetails".to_string(),
        attachment_details_value(&parsed.attachment_details),
    );
    attrs.insert("headers".to_string(), headers_value(&parsed.headers));
    attrs.insert(
        "bodyPreview".to_string(),
        Value::String(parsed.body_preview.clone()),
    );
    if let Some(body_plain) = &parsed.body_plain {
        attrs.insert("bodyPlain".to_string(), Value::String(body_plain.clone()));
    }
    if let Some(body_html) = &parsed.body_html {
        attrs.insert("bodyHtml".to_string(), Value::String(body_html.clone()));
    }
    insert_eml_metadata(&mut attrs, parsed);
    if let Some(client) = identify_mail_client(
        parsed.x_mailer.as_deref().unwrap_or_default(),
        &candidate.path,
    ) {
        attrs.insert("mailClient".to_string(), Value::String(client));
    }
    attrs.insert(
        "attachmentCount".to_string(),
        Value::Number(serde_json::Number::from(parsed.attachments.len())),
    );
    attrs.insert("isDeleted".to_string(), Value::Null);
    attrs
}

fn insert_eml_metadata(attrs: &mut BTreeMap<String, Value>, parsed: &ParsedEmail) {
    if let Some(sent_at) = parsed.sent_at {
        attrs.insert("sentAt".to_string(), Value::String(sent_at.to_rfc3339()));
    }
    if let Some(received_at) = parsed.received_at {
        attrs.insert(
            "receivedAt".to_string(),
            Value::String(received_at.to_rfc3339()),
        );
    }
    if let Some(reply_to) = &parsed.reply_to {
        attrs.insert("replyTo".to_string(), Value::String(reply_to.clone()));
    }
    if let Some(return_path) = &parsed.return_path {
        attrs.insert("returnPath".to_string(), Value::String(return_path.clone()));
    }
    if let Some(in_reply_to) = &parsed.in_reply_to {
        attrs.insert("inReplyTo".to_string(), Value::String(in_reply_to.clone()));
    }
    if !parsed.references.is_empty() {
        attrs.insert(
            "references".to_string(),
            string_array_value(&parsed.references),
        );
    }
    if let Some(x_mailer) = &parsed.x_mailer {
        attrs.insert("xMailer".to_string(), Value::String(x_mailer.clone()));
    }
    if let Some(x_originating_ip) = &parsed.x_originating_ip {
        attrs.insert(
            "xOriginatingIp".to_string(),
            Value::String(x_originating_ip.clone()),
        );
    }
    if let Some(message_class) = &parsed.message_class {
        attrs.insert(
            "messageClass".to_string(),
            Value::String(message_class.clone()),
        );
    }
}

fn build_eml_outcome(
    candidate: &EvidenceCandidate,
    parsed: ParsedEmail,
    attrs: BTreeMap<String, Value>,
    extractor_id: &str,
) -> ExtractionOutcome {
    let mut outcome = ExtractionOutcome::default();
    let event_time = parsed.sent_at.or(parsed.received_at);
    if let Some(event_time) = event_time {
        outcome.timeline_events.push(make_timeline_event(
            &candidate.file_id,
            "EMAIL_SENT",
            event_time,
            format!("Email: {}", title_or_url(&parsed.subject, &candidate.path)),
            parsed.from.clone(),
            attrs.clone(),
            extractor_id,
        ));
    }
    outcome.artifacts.push(make_artifact(
        "EmailMessage",
        format!("Email: {}", title_or_url(&parsed.subject, &candidate.path)),
        parsed.from,
        candidate,
        extractor_id,
        attrs,
    ));
    outcome
}

fn attachment_details_value(attachments: &[EmailAttachmentDto]) -> Value {
    Value::Array(
        attachments
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

fn headers_value(headers: &[EmailHeaderDto]) -> Value {
    Value::Array(
        headers
            .iter()
            .map(|header| serde_json::json!({"name": header.name, "value": header.value}))
            .collect(),
    )
}
