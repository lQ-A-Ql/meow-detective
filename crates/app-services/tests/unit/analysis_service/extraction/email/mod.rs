use super::eml::parse_email_message;
use super::mbox::extract_mbox_candidate;
use super::pst::extract_pst_candidate;
use crate::analysis_service::candidates::EvidenceCandidate;

const SIMPLE_PLAIN: &str = "\
From: Alice <alice@example.com>\n\
To: Bob <bob@example.com>\n\
Cc: Carol <carol@example.com>\n\
Subject: Hello\n\
Date: Mon, 16 Jun 2025 10:00:00 +0000\n\
Message-Id: <abc123@example.com>\n\
X-Mailer: TestMailer/1.0\n\
\n\
Hello Bob,\n\
This is a test message.\n";

const MULTIPART_ATTACHMENT: &str = "\
From: sender@example.com\n\
To: recipient@example.com\n\
Subject: Document\n\
Date: Sun, 15 Jun 2025 14:00:00 +0200\n\
Content-Type: multipart/mixed; boundary=\"bound123\"\n\
\n\
--bound123\n\
Content-Type: text/plain\n\
\n\
Please find the document attached.\n\
--bound123\n\
Content-Type: application/octet-stream; name=\"data.bin\"\n\
Content-Disposition: attachment; filename=\"data.bin\"\n\
Content-Transfer-Encoding: base64\n\
\n\
SGVsbG8gV29ybGQh\n\
--bound123--\n";

const HTML_ALTERNATIVE: &str = "\
From: a@example.com\n\
To: b@example.com\n\
Subject: HTML mail\n\
Date: Tue, 17 Jun 2025 08:00:00 +0000\n\
Content-Type: multipart/alternative; boundary=\"alt\"\n\
\n\
--alt\n\
Content-Type: text/plain\n\
\n\
Plain body.\n\
--alt\n\
Content-Type: text/html\n\
\n\
<html><body>HTML body</body></html>\n\
--alt--\n";

const EMLX_SIZE_PREFIX: &str = "1234\nFrom: a@example.com\nTo: b@example.com\nSubject: Emlx\nDate: Wed, 18 Jun 2025 09:00:00 +0000\n\nBody.\n";

const ENCODED_HEADERS: &str = "\
From: =?UTF-8?B?5p2O5aic?= <a@example.com>\n\
To: b@example.com\n\
Subject: =?UTF-8?Q?=E4=B8=AD=E6=96=87=E4=B8=BB=E9=A2=98?=\n\
Date: Thu, 19 Jun 2025 10:00:00 +0000\n\n\
Body.\n";

const REPLY_THREAD: &str = "\
From: a@example.com\n\
To: b@example.com\n\
Subject: Re: Thread\n\
Date: Fri, 20 Jun 2025 11:00:00 +0000\n\
Message-Id: <msg2@example.com>\n\
In-Reply-To: <msg1@example.com>\n\
References: <msg1@example.com> <msg1.1@example.com>\n\
\n\
Reply body.\n";

#[test]
fn parses_simple_plain_email() {
    let parsed = parse_email_message(SIMPLE_PLAIN.as_bytes()).expect("should parse simple email");
    assert_eq!(parsed.from, "Alice <alice@example.com>");
    assert_eq!(parsed.to, vec!["Bob <bob@example.com>"]);
    assert_eq!(parsed.cc, vec!["Carol <carol@example.com>"]);
    assert_eq!(parsed.subject, "Hello");
    assert_eq!(parsed.message_id, "<abc123@example.com>");
    assert_eq!(parsed.x_mailer.as_deref(), Some("TestMailer/1.0"));
    assert!(parsed.body_plain.as_deref().unwrap().contains("Hello Bob"));
    assert!(parsed.body_preview.contains("Hello Bob"));
    assert!(parsed.sent_at.is_some());
    assert!(!parsed.headers.is_empty());
}

#[test]
fn parses_multipart_attachment() {
    let parsed =
        parse_email_message(MULTIPART_ATTACHMENT.as_bytes()).expect("should parse multipart email");
    assert_eq!(parsed.attachments, vec!["data.bin"]);
    assert_eq!(parsed.attachment_details.len(), 1);
    let att = &parsed.attachment_details[0];
    assert_eq!(att.file_name, "data.bin");
    assert_eq!(att.size, Some(12));
    assert_eq!(att.mime_type.as_deref(), Some("application/octet-stream"));
    assert!(parsed
        .body_plain
        .as_deref()
        .unwrap()
        .contains("document attached"));
}

#[test]
fn parses_html_alternative() {
    let parsed = parse_email_message(HTML_ALTERNATIVE.as_bytes()).expect("should parse html email");
    assert!(parsed.body_plain.as_deref().unwrap().contains("Plain body"));
    assert!(parsed
        .body_html
        .as_deref()
        .unwrap()
        .contains("<html><body>HTML body</body></html>"));
}

#[test]
fn strips_emlx_size_prefix() {
    let parsed = parse_email_message(EMLX_SIZE_PREFIX.as_bytes()).expect("should parse emlx email");
    assert_eq!(parsed.subject, "Emlx");
    assert!(parsed
        .body_plain
        .as_deref()
        .unwrap_or("")
        .trim()
        .contains("Body."));
}

#[test]
fn decodes_encoded_headers() {
    let parsed =
        parse_email_message(ENCODED_HEADERS.as_bytes()).expect("should parse encoded headers");
    assert_eq!(parsed.from, "李娜 <a@example.com>");
    assert_eq!(parsed.subject, "中文主题");
}

#[test]
fn parses_thread_headers() {
    let parsed = parse_email_message(REPLY_THREAD.as_bytes()).expect("should parse reply thread");
    assert_eq!(parsed.in_reply_to.as_deref(), Some("<msg1@example.com>"));
    assert_eq!(
        parsed.references,
        vec!["msg1@example.com", "msg1.1@example.com"]
    );
}

/// Regression gate for the public-small synthetic email fixtures.
#[test]
fn public_small_email_fixtures_match_expected() {
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir
        .join("../../testdata/fixtures/public-small/email")
        .canonicalize()
        .expect("fixture dir exists");
    let expected_path = fixture_dir.join("expected.json");
    let expected: Value =
        serde_json::from_str(&fs::read_to_string(expected_path).unwrap()).unwrap();

    for sample in expected["samples"].as_array().unwrap() {
        let file_name = sample["file"].as_str().unwrap();
        let sample_type = sample["type"].as_str().unwrap_or("eml");
        let exp = &sample["expected"];
        let bytes = fs::read(fixture_dir.join(file_name)).unwrap();

        if sample_type == "mbox" {
            assert_mbox_fixture(&bytes, exp, file_name);
            continue;
        }
        if sample_type == "pst" || sample_type == "ost" {
            assert_pst_fixture(&bytes, exp, file_name, sample_type);
            continue;
        }

        let parsed = parse_email_message(&bytes)
            .unwrap_or_else(|err| panic!("{file_name} should parse: {err}"));

        if let Some(v) = exp["from"].as_str() {
            assert_eq!(parsed.from, v, "{file_name} from");
        }
        if let Some(v) = exp["fromContains"].as_str() {
            assert!(
                parsed.from.contains(v),
                "{file_name} from should contain {v}"
            );
        }
        assert_eq_str_vec(&parsed.to, &exp["to"], file_name, "to");
        assert_eq_str_vec(&parsed.cc, &exp["cc"], file_name, "cc");
        assert_eq_str_vec(&parsed.bcc, &exp["bcc"], file_name, "bcc");
        assert_opt_eq(&parsed.reply_to, &exp["replyTo"], file_name, "replyTo");
        assert_opt_eq(
            &parsed.return_path,
            &exp["returnPath"],
            file_name,
            "returnPath",
        );
        if let Some(v) = exp["subject"].as_str() {
            assert_eq!(parsed.subject, v, "{file_name} subject");
        }
        if let Some(v) = exp["subjectContains"].as_str() {
            assert!(
                parsed.subject.contains(v),
                "{file_name} subject should contain {v}"
            );
        }
        if let Some(v) = exp["messageId"].as_str() {
            assert_eq!(parsed.message_id, v, "{file_name} messageId");
        }
        assert_opt_eq(
            &parsed.in_reply_to,
            &exp["inReplyTo"],
            file_name,
            "inReplyTo",
        );
        assert_eq_str_vec(
            &parsed.references,
            &exp["references"],
            file_name,
            "references",
        );
        assert_eq_str_vec(
            &parsed.attachments,
            &exp["attachments"],
            file_name,
            "attachments",
        );
        assert_contains(
            parsed.body_preview.as_str(),
            &exp["bodyPreviewContains"],
            file_name,
            "bodyPreview",
        );
        assert_opt_contains(
            parsed.body_plain.as_deref(),
            &exp["bodyPlainContains"],
            file_name,
            "bodyPlain",
        );
        assert_opt_contains(
            parsed.body_html.as_deref(),
            &exp["bodyHtmlContains"],
            file_name,
            "bodyHtml",
        );
        assert_opt_eq(&parsed.x_mailer, &exp["xMailer"], file_name, "xMailer");
        assert_opt_contains(
            parsed.x_originating_ip.as_deref(),
            &exp["xOriginatingIp"],
            file_name,
            "xOriginatingIp",
        );

        if let Some(v) = exp["attachmentCount"].as_u64() {
            assert_eq!(
                parsed.attachment_details.len() as u64,
                v,
                "{file_name} attachment count"
            );
        }
        if let Some(details) = exp["attachmentDetails"].as_array() {
            assert_eq!(
                parsed.attachment_details.len(),
                details.len(),
                "{file_name} attachment details length"
            );
            for (actual, expected) in parsed.attachment_details.iter().zip(details.iter()) {
                if let Some(v) = expected["fileName"].as_str() {
                    assert_eq!(actual.file_name, v, "attachment fileName");
                }
                if let Some(v) = expected["mimeType"].as_str() {
                    assert_eq!(actual.mime_type.as_deref(), Some(v), "attachment mimeType");
                }
                if let Some(v) = expected["size"].as_u64() {
                    assert_eq!(actual.size.unwrap_or(0), v, "attachment size");
                }
                assert_opt_eq(
                    &actual.content_id,
                    &expected["contentId"],
                    file_name,
                    "contentId",
                );
            }
        }

        if let Some(v) = exp["sentAt"].as_str() {
            let expected_date = chrono::DateTime::parse_from_rfc3339(v)
                .unwrap()
                .with_timezone(&chrono::Utc);
            assert_eq!(parsed.sent_at.unwrap(), expected_date, "{file_name} sentAt");
        }
        assert_opt_eq(
            &parsed.message_class,
            &exp["messageClass"],
            file_name,
            "messageClass",
        );
    }
}

/// Regression gate for the public-medium synthetic email fixtures.
#[test]
fn public_medium_email_fixtures_match_expected() {
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture_dir = manifest_dir
        .join("../../testdata/fixtures/public-medium/email")
        .canonicalize()
        .expect("fixture dir exists");
    let expected_path = fixture_dir.join("expected.json");
    let expected: Value =
        serde_json::from_str(&fs::read_to_string(expected_path).unwrap()).unwrap();

    for sample in expected["samples"].as_array().unwrap() {
        let file_name = sample["file"].as_str().unwrap();
        let sample_type = sample["type"].as_str().unwrap_or("eml");
        let exp = &sample["expected"];
        let bytes = fs::read(fixture_dir.join(file_name)).unwrap();

        if sample_type == "mbox" {
            assert_mbox_fixture(&bytes, exp, file_name);
            continue;
        }
        if sample_type == "pst" || sample_type == "ost" {
            assert_pst_fixture(&bytes, exp, file_name, sample_type);
            continue;
        }

        let parsed = parse_email_message(&bytes)
            .unwrap_or_else(|err| panic!("{file_name} should parse: {err}"));

        if let Some(v) = exp["fromContains"].as_str() {
            assert!(
                parsed.from.contains(v),
                "{file_name} from should contain {v}"
            );
        }
        if let Some(v) = exp["subjectContains"].as_str() {
            assert!(
                parsed.subject.contains(v),
                "{file_name} subject should contain {v}, got {}",
                parsed.subject
            );
        }
    }
}

fn assert_mbox_fixture(bytes: &[u8], exp: &serde_json::Value, file_name: &str) {
    let candidate = EvidenceCandidate {
        file_id: domain::FileEntryId("file-mbox".to_string()),
        data_source_id: "ds-1".to_string(),
        path: format!("/fixtures/{file_name}"),
        size: bytes.len() as u64,
        evidence_kind: "email_mbox".to_string(),
        parser: "email.mbox".to_string(),
        category: "Email".to_string(),
    };
    let outcome = extract_mbox_candidate(&candidate, bytes);
    assert!(
        outcome.warnings.is_empty(),
        "{file_name} warnings: {:?}",
        outcome.warnings
    );

    if let Some(v) = exp["messagesCount"].as_u64() {
        assert_eq!(
            outcome.artifacts.len() as u64,
            v,
            "{file_name} artifact count"
        );
    }

    if let Some(expected_messages) = exp["messages"].as_array() {
        for (idx, (artifact, expected)) in outcome
            .artifacts
            .iter()
            .zip(expected_messages.iter())
            .enumerate()
        {
            let prefix = format!("{file_name} message {idx}");
            let attrs = &artifact.attrs;
            if let Some(v) = expected["from"].as_str() {
                assert_eq!(string_attr(attrs, "from"), v, "{prefix} from");
            }
            if let Some(v) = expected["fromContains"].as_str() {
                assert!(
                    string_attr(attrs, "from").contains(v),
                    "{prefix} from should contain {v}"
                );
            }
            assert_eq_str_vec(
                &string_vec_attr(attrs, "to"),
                &expected["to"],
                &prefix,
                "to",
            );
            assert_eq_str_vec(
                &string_vec_attr(attrs, "cc"),
                &expected["cc"],
                &prefix,
                "cc",
            );
            assert_eq_str_vec(
                &string_vec_attr(attrs, "bcc"),
                &expected["bcc"],
                &prefix,
                "bcc",
            );
            assert_opt_eq(
                &optional_string_attr(attrs, "replyTo"),
                &expected["replyTo"],
                &prefix,
                "replyTo",
            );
            assert_opt_eq(
                &optional_string_attr(attrs, "returnPath"),
                &expected["returnPath"],
                &prefix,
                "returnPath",
            );
            if let Some(v) = expected["subject"].as_str() {
                assert_eq!(string_attr(attrs, "subject"), v, "{prefix} subject");
            }
            assert_opt_eq(
                &optional_string_attr(attrs, "messageId"),
                &expected["messageId"],
                &prefix,
                "messageId",
            );
            assert_opt_eq(
                &optional_string_attr(attrs, "inReplyTo"),
                &expected["inReplyTo"],
                &prefix,
                "inReplyTo",
            );
            assert_eq_str_vec(
                &string_vec_attr(attrs, "references"),
                &expected["references"],
                &prefix,
                "references",
            );
            assert_contains(
                &string_attr(attrs, "bodyPreview"),
                &expected["bodyPreviewContains"],
                &prefix,
                "bodyPreview",
            );
            assert_opt_contains(
                optional_string_attr(attrs, "bodyPlain").as_deref(),
                &expected["bodyPlainContains"],
                &prefix,
                "bodyPlain",
            );
            assert_opt_contains(
                optional_string_attr(attrs, "bodyHtml").as_deref(),
                &expected["bodyHtmlContains"],
                &prefix,
                "bodyHtml",
            );
            assert_not_contains(
                optional_string_attr(attrs, "bodyPlain")
                    .as_deref()
                    .unwrap_or(""),
                &expected["bodyPlainNotContains"],
                &prefix,
                "bodyPlainNotContains",
            );
            assert_eq_str_vec(
                &string_vec_attr(attrs, "attachments"),
                &expected["attachments"],
                &prefix,
                "attachments",
            );
            assert_opt_contains(
                optional_string_attr(attrs, "xMailer").as_deref(),
                &expected["xMailer"],
                &prefix,
                "xMailer",
            );
            assert_opt_contains(
                optional_string_attr(attrs, "xOriginatingIp").as_deref(),
                &expected["xOriginatingIp"],
                &prefix,
                "xOriginatingIp",
            );
            if let Some(v) = expected["attachmentCount"].as_u64() {
                assert_eq!(
                    attachment_details_attr(attrs, "attachmentDetails").len() as u64,
                    v,
                    "{prefix} attachment count"
                );
            }
            if let Some(details) = expected["attachmentDetails"].as_array() {
                let actual = attachment_details_attr(attrs, "attachmentDetails");
                assert_eq!(
                    actual.len(),
                    details.len(),
                    "{prefix} attachment details length"
                );
                for (a, e) in actual.iter().zip(details.iter()) {
                    if let Some(v) = e["fileName"].as_str() {
                        assert_eq!(a.file_name, v, "attachment fileName");
                    }
                    if let Some(v) = e["mimeType"].as_str() {
                        assert_eq!(a.mime_type.as_deref(), Some(v), "attachment mimeType");
                    }
                    if let Some(v) = e["size"].as_u64() {
                        assert_eq!(a.size.unwrap_or(0), v, "attachment size");
                    }
                }
            }
            if let Some(v) = expected["sentAt"].as_str() {
                let expected_date = chrono::DateTime::parse_from_rfc3339(v)
                    .unwrap()
                    .with_timezone(&chrono::Utc);
                let actual = optional_string_attr(attrs, "sentAt")
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc));
                assert_eq!(actual.unwrap(), expected_date, "{prefix} sentAt");
            }
            assert_opt_eq(
                &optional_string_attr(attrs, "messageClass"),
                &expected["messageClass"],
                &prefix,
                "messageClass",
            );
            if !expected["isDeleted"].is_null() {
                assert_eq!(
                    bool_attr(attrs, "isDeleted"),
                    expected["isDeleted"].as_bool(),
                    "{prefix} isDeleted"
                );
            }
        }
    }

    if let Some(first) = exp["firstMessage"].as_object() {
        if let Some(artifact) = outcome.artifacts.first() {
            assert_message_summary(&artifact.attrs, first, &format!("{file_name} firstMessage"));
        }
    }
    if let Some(last) = exp["lastMessage"].as_object() {
        if let Some(artifact) = outcome.artifacts.last() {
            assert_message_summary(&artifact.attrs, last, &format!("{file_name} lastMessage"));
        }
    }

    if let Some(v) = exp["containerPath"].as_str() {
        if let Some(first) = outcome.artifacts.first() {
            assert_eq!(
                optional_string_attr(&first.attrs, "containerPath"),
                Some(v.to_string()),
                "{file_name} containerPath"
            );
        }
    }
}

fn assert_message_summary(
    attrs: &std::collections::BTreeMap<String, serde_json::Value>,
    expected: &serde_json::Map<String, serde_json::Value>,
    prefix: &str,
) {
    use serde_json::Value;

    if let Some(v) = expected.get("fromContains").and_then(Value::as_str) {
        assert!(
            string_attr(attrs, "from").contains(v),
            "{prefix} from should contain {v}"
        );
    }
    if let Some(v) = expected.get("subjectContains").and_then(Value::as_str) {
        assert!(
            string_attr(attrs, "subject").contains(v),
            "{prefix} subject should contain {v}"
        );
    }
    if let Some(v) = expected.get("bodyContains").and_then(Value::as_str) {
        assert!(
            string_attr(attrs, "bodyPreview").contains(v),
            "{prefix} bodyPreview should contain {v}"
        );
    }
    if let Some(v) = expected.get("bodyPlainContains").and_then(Value::as_str) {
        assert!(
            optional_string_attr(attrs, "bodyPlain")
                .as_deref()
                .unwrap_or("")
                .contains(v),
            "{prefix} bodyPlain should contain {v}"
        );
    }
    assert_eq_str_vec(
        &string_vec_attr(attrs, "to"),
        &expected.get("to").cloned().unwrap_or(Value::Null),
        prefix,
        "to",
    );
    if let Some(v) = expected.get("attachmentCount").and_then(Value::as_u64) {
        assert_eq!(
            attachment_details_attr(attrs, "attachmentDetails").len() as u64,
            v,
            "{prefix} attachmentCount"
        );
    }
    assert_opt_eq(
        &optional_string_attr(attrs, "messageClass"),
        &expected.get("messageClass").cloned().unwrap_or(Value::Null),
        prefix,
        "messageClass",
    );
    if expected.get("isDeleted").is_some_and(|v| !v.is_null()) {
        assert_eq!(
            bool_attr(attrs, "isDeleted"),
            expected.get("isDeleted").and_then(Value::as_bool),
            "{prefix} isDeleted"
        );
    }
}

fn assert_pst_fixture(bytes: &[u8], exp: &serde_json::Value, file_name: &str, sample_type: &str) {
    let candidate = EvidenceCandidate {
        file_id: domain::FileEntryId(format!("file-{sample_type}")),
        data_source_id: "ds-1".to_string(),
        path: format!("/fixtures/{file_name}"),
        size: bytes.len() as u64,
        evidence_kind: format!("email_{sample_type}"),
        parser: format!("email.{sample_type}"),
        category: "Email".to_string(),
    };
    let outcome = extract_pst_candidate(&candidate, bytes);
    assert!(
        outcome.warnings.is_empty(),
        "{file_name} warnings: {:?}",
        outcome.warnings
    );

    if let Some(v) = exp["messagesCount"].as_u64() {
        assert_eq!(
            outcome.artifacts.len() as u64,
            v,
            "{file_name} artifact count"
        );
    }

    if let Some(expected_messages) = exp["messages"].as_array() {
        for (idx, (artifact, expected)) in outcome
            .artifacts
            .iter()
            .zip(expected_messages.iter())
            .enumerate()
        {
            let prefix = format!("{file_name} message {idx}");
            let attrs = &artifact.attrs;
            if let Some(v) = expected["from"].as_str() {
                assert_eq!(string_attr(attrs, "from"), v, "{prefix} from");
            }
            if let Some(v) = expected["fromContains"].as_str() {
                assert!(
                    string_attr(attrs, "from").contains(v),
                    "{prefix} from should contain {v}"
                );
            }
            assert_eq_str_vec(
                &string_vec_attr(attrs, "to"),
                &expected["to"],
                &prefix,
                "to",
            );
            assert_eq_str_vec(
                &string_vec_attr(attrs, "cc"),
                &expected["cc"],
                &prefix,
                "cc",
            );
            assert_eq_str_vec(
                &string_vec_attr(attrs, "bcc"),
                &expected["bcc"],
                &prefix,
                "bcc",
            );
            assert_opt_eq(
                &optional_string_attr(attrs, "replyTo"),
                &expected["replyTo"],
                &prefix,
                "replyTo",
            );
            assert_opt_eq(
                &optional_string_attr(attrs, "returnPath"),
                &expected["returnPath"],
                &prefix,
                "returnPath",
            );
            if let Some(v) = expected["subject"].as_str() {
                assert_eq!(string_attr(attrs, "subject"), v, "{prefix} subject");
            }
            assert_opt_eq(
                &optional_string_attr(attrs, "messageId"),
                &expected["messageId"],
                &prefix,
                "messageId",
            );
            assert_opt_eq(
                &optional_string_attr(attrs, "inReplyTo"),
                &expected["inReplyTo"],
                &prefix,
                "inReplyTo",
            );
            assert_eq_str_vec(
                &string_vec_attr(attrs, "references"),
                &expected["references"],
                &prefix,
                "references",
            );
            assert_contains(
                &string_attr(attrs, "bodyPreview"),
                &expected["bodyPreviewContains"],
                &prefix,
                "bodyPreview",
            );
            assert_opt_contains(
                optional_string_attr(attrs, "bodyPlain").as_deref(),
                &expected["bodyPlainContains"],
                &prefix,
                "bodyPlain",
            );
            assert_opt_contains(
                optional_string_attr(attrs, "bodyHtml").as_deref(),
                &expected["bodyHtmlContains"],
                &prefix,
                "bodyHtml",
            );
            assert_opt_contains(
                optional_string_attr(attrs, "xMailer").as_deref(),
                &expected["xMailer"],
                &prefix,
                "xMailer",
            );
            assert_opt_contains(
                optional_string_attr(attrs, "xOriginatingIp").as_deref(),
                &expected["xOriginatingIp"],
                &prefix,
                "xOriginatingIp",
            );
            assert_eq_str_vec(
                &string_vec_attr(attrs, "attachments"),
                &expected["attachments"],
                &prefix,
                "attachments",
            );
            if let Some(v) = expected["attachmentCount"].as_u64() {
                assert_eq!(
                    attachment_details_attr(attrs, "attachmentDetails").len() as u64,
                    v,
                    "{prefix} attachment count"
                );
            }
            if let Some(v) = expected["sentAt"].as_str() {
                let expected_date = chrono::DateTime::parse_from_rfc3339(v)
                    .unwrap()
                    .with_timezone(&chrono::Utc);
                let actual = optional_string_attr(attrs, "sentAt")
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc));
                assert_eq!(actual.unwrap(), expected_date, "{prefix} sentAt");
            }
            assert_opt_eq(
                &optional_string_attr(attrs, "messageClass"),
                &expected["messageClass"],
                &prefix,
                "messageClass",
            );
            if !expected["isDeleted"].is_null() {
                assert_eq!(
                    bool_attr(attrs, "isDeleted"),
                    expected["isDeleted"].as_bool(),
                    "{prefix} isDeleted"
                );
            }
        }
    }

    if let Some(first) = exp["firstMessage"].as_object() {
        if let Some(artifact) = outcome.artifacts.first() {
            assert_message_summary(&artifact.attrs, first, &format!("{file_name} firstMessage"));
        }
    }
    if let Some(last) = exp["lastMessage"].as_object() {
        if let Some(artifact) = outcome.artifacts.last() {
            assert_message_summary(&artifact.attrs, last, &format!("{file_name} lastMessage"));
        }
    }

    if let Some(v) = exp["containerPath"].as_str() {
        if let Some(first) = outcome.artifacts.first() {
            assert_eq!(
                optional_string_attr(&first.attrs, "containerPath"),
                Some(v.to_string()),
                "{file_name} containerPath"
            );
        }
    }
    if let Some(v) = exp["containerPathContains"].as_str() {
        if let Some(first) = outcome.artifacts.first() {
            let actual = optional_string_attr(&first.attrs, "containerPath").unwrap_or_default();
            assert!(
                actual.contains(v),
                "{file_name} containerPath should contain {v}, got {actual}"
            );
        }
    }
}

fn assert_eq_str_vec(
    actual: &[String],
    expected: &serde_json::Value,
    file_name: &str,
    field: &str,
) {
    if expected.is_null() {
        return;
    }
    let expected: Vec<String> = expected
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(actual.to_vec(), expected, "{file_name} {field}");
}

fn assert_opt_eq(
    actual: &Option<String>,
    expected: &serde_json::Value,
    file_name: &str,
    field: &str,
) {
    if expected.is_null() {
        return;
    }
    if let Some(v) = expected.as_str() {
        assert_eq!(actual.as_deref(), Some(v), "{file_name} {field}");
    }
}

fn assert_opt_contains(
    actual: Option<&str>,
    expected: &serde_json::Value,
    file_name: &str,
    field: &str,
) {
    if expected.is_null() {
        return;
    }
    if let Some(v) = expected.as_str() {
        let actual = actual.unwrap_or("");
        assert!(
            actual.contains(v),
            "{file_name} {field} should contain {v}, got {actual}"
        );
    }
}

fn assert_contains(actual: &str, expected: &serde_json::Value, file_name: &str, field: &str) {
    if expected.is_null() {
        return;
    }
    if let Some(v) = expected.as_str() {
        assert!(
            actual.contains(v),
            "{file_name} {field} should contain {v}, got {actual}"
        );
    }
}

fn assert_not_contains(actual: &str, expected: &serde_json::Value, file_name: &str, field: &str) {
    if expected.is_null() {
        return;
    }
    if let Some(v) = expected.as_str() {
        assert!(
            !actual.contains(v),
            "{file_name} {field} should NOT contain {v}, got {actual}"
        );
    }
}

fn string_attr(attrs: &std::collections::BTreeMap<String, serde_json::Value>, key: &str) -> String {
    attrs
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn bool_attr(
    attrs: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<bool> {
    attrs.get(key).and_then(serde_json::Value::as_bool)
}

fn optional_string_attr(
    attrs: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    attrs
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn string_vec_attr(
    attrs: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Vec<String> {
    attrs
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn attachment_details_attr(
    attrs: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Vec<transport::dto::EmailAttachmentDto> {
    attrs
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|v| {
                    Some(transport::dto::EmailAttachmentDto {
                        file_name: v.get("fileName")?.as_str()?.to_string(),
                        size: v.get("size")?.as_u64(),
                        mime_type: v.get("mimeType")?.as_str().map(str::to_string),
                        content_id: v.get("contentId")?.as_str().map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}
