use containers_pst::mbox::{detect_variant, parse_mbox, MboxVariant};

const SAMPLE_MBOXO: &str = "\
From alice@example.com Fri Jun 13 10:00:00 2025
From: Alice <alice@example.com>
To: Bob <bob@example.com>
Subject: Hello
Date: Fri, 13 Jun 2025 10:00:00 +0000
Content-Type: text/plain

Hello Bob,
This is a test message.
";

const SAMPLE_MBOXRD: &str = "\
From alice@example.com Mon Jun 16 09:00:00 2025
From: Alice <alice@example.com>
To: Bob <bob@example.com>
Subject: Forwarded note
Date: Mon, 16 Jun 2025 09:00:00 +0200
Content-Type: text/plain

FYI - see below.

>From charlie@example.com Mon Jun 16 08:00:00 2025
>From: Charlie <charlie@example.com>
>Subject: Original

Original message content here.
";

const SAMPLE_MBOXCL2: &str = "\
From sender@example.com Mon Jun 16 14:00:00 2025
Content-Length: 145
Status: RO
X-Status: F
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: CL2 test
Date: Mon, 16 Jun 2025 14:00:00 +0200

This message uses Content-Length with Status headers.
";

const SAMPLE_MULTIPART: &str = "\
From sender@example.com Sun Jun 15 14:00:00 2025
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: Document with attachment
Date: Sun, 15 Jun 2025 14:00:00 +0200
Content-Type: multipart/mixed; boundary=\"----boundary123\"

------boundary123
Content-Type: text/plain

Please find the document attached.

------boundary123
Content-Type: application/octet-stream; name=\"data.bin\"
Content-Disposition: attachment; filename=\"data.bin\"
Content-Transfer-Encoding: base64

SGVsbG8gV29ybGQh

------boundary123--
";

const SAMPLE_HEADERS: &str = "\
From sender@example.com Mon Jun 16 10:30:00 2025
From: \"Sender Name\" <sender@example.com>
To: Alice <alice@example.com>, Bob <bob@example.com>
Cc: Carol <carol@example.com>
Bcc: Dan <dan@example.com>
Reply-To: Reply Team <reply@example.com>
Return-Path: bounce@example.com
Message-Id: <id-1@example.com>
In-Reply-To: <parent@example.com>
References: <root@example.com> <parent@example.com>
X-Message-Class: IPM.Note
X-Mailer: Example Mailer
X-Originating-IP: [192.0.2.1]
Date: Mon, 16 Jun 2025 10:30:00 +0200
Content-Type: text/plain

Body text.
";

#[test]
fn detect_variant_covers_common_mbox_flavors() {
    assert_eq!(detect_variant(SAMPLE_MBOXO), MboxVariant::MboxO);
    assert_eq!(detect_variant(SAMPLE_MBOXRD), MboxVariant::MboxRd);
    assert_eq!(detect_variant(SAMPLE_MBOXCL2), MboxVariant::MboxCl2);
}

#[test]
fn parse_mbox_preserves_header_fields() {
    let messages = parse_mbox(SAMPLE_HEADERS.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.sender_name, "");
    assert_eq!(msg.sender_email, "sender@example.com");
    assert_eq!(
        msg.recipients,
        vec![
            "Alice <alice@example.com>".to_string(),
            "Bob <bob@example.com>".to_string()
        ]
    );
    assert_eq!(
        msg.to,
        vec![
            "Alice <alice@example.com>".to_string(),
            "Bob <bob@example.com>".to_string()
        ]
    );
    assert_eq!(msg.cc, vec!["Carol <carol@example.com>".to_string()]);
    assert_eq!(msg.bcc, vec!["Dan <dan@example.com>".to_string()]);
    assert_eq!(msg.reply_to, "Reply Team <reply@example.com>");
    assert_eq!(msg.return_path, "bounce@example.com");
    assert_eq!(msg.message_id, "<id-1@example.com>");
    assert_eq!(msg.in_reply_to, "<parent@example.com>");
    assert_eq!(
        msg.references,
        vec![
            "root@example.com".to_string(),
            "parent@example.com".to_string()
        ]
    );
    assert_eq!(msg.message_class, "IPM.Note");
    assert_eq!(msg.x_mailer, "Example Mailer");
    assert_eq!(msg.x_originating_ip, "[192.0.2.1]");
    assert!(msg.sent_time.is_some());
    assert_eq!(msg.folder_path, "");
    assert_eq!(msg.headers.len(), 14);
}

#[test]
fn parse_mbox_unescapes_mboxrd_body() {
    let messages = parse_mbox(SAMPLE_MBOXRD.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert!(!msg.body_plain.contains(">From "));
    assert!(msg.body_plain.contains("From charlie@example.com"));
    assert!(msg.body_plain.contains("Original message content here."));
}

#[test]
fn parse_mbox_parses_multipart_attachment() {
    let messages = parse_mbox(SAMPLE_MULTIPART.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert!(msg
        .body_plain
        .contains("Please find the document attached."));
    assert_eq!(msg.attachments.len(), 1);
    assert_eq!(msg.attachments[0].name, "data.bin");
    assert_eq!(msg.attachments[0].mime_type, "application/octet-stream");
    assert_eq!(msg.attachments[0].data, b"Hello World!");
}

#[test]
fn parse_mbox_handles_empty_and_invalid_inputs() {
    let empty = parse_mbox(b"").expect("parse should succeed");
    assert!(empty.is_empty());

    let err = parse_mbox(&[0xff, 0xfe, 0xfd]).expect_err("invalid utf-8 should fail");
    assert!(matches!(err, containers_pst::error::PstError::MboxError(_)));
}
