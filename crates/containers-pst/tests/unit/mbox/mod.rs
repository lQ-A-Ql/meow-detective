use super::framing::{detect_variant, is_message_separator, parse_mbox, MboxVariant};
use super::header::{find_header, parse_address, parse_email_date, split_headers_and_body};
use super::mime::{
    extract_boundary, extract_filename, quoted_printable_decode, unescape_from_lines,
};

const SAMPLE_SINGLE: &str = "\
From alice@example.com Fri Jun 13 10:00:00 2025
From: Alice <alice@example.com>
To: Bob <bob@example.com>
Subject: Hello
Date: Fri, 13 Jun 2025 10:00:00 +0000
Content-Type: text/plain

Hello Bob,
This is a test message.
Best,
Alice
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
>To: Alice <alice@example.com>
>Subject: Original
>
>Original message content here.

From bob@example.com Mon Jun 16 10:00:00 2025
From: Bob <bob@example.com>
To: Alice <alice@example.com>
Subject: Re: Forwarded note
Date: Mon, 16 Jun 2025 10:00:00 +0200
Content-Type: text/plain

Got it, thanks!
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

const SAMPLE_MBOXCL: &str = "\
From sender@example.com Mon Jun 16 12:00:00 2025
Content-Length: 120
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: CL test
Date: Mon, 16 Jun 2025 12:00:00 +0200

This message uses Content-Length.

From sender2@example.com Mon Jun 16 13:00:00 2025
Content-Length: 110
From: Sender2 <sender2@example.com>
To: Recipient <recipient@example.com>
Subject: Second CL test
Date: Mon, 16 Jun 2025 13:00:00 +0200

Another Content-Length message.
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

const SAMPLE_EMPTY: &str = "";

#[test]
fn test_detect_variant_mboxrd() {
    let v = detect_variant(SAMPLE_MBOXRD);
    assert_eq!(v, MboxVariant::MboxRd);
}

#[test]
fn test_detect_variant_mboxo() {
    let v = detect_variant(SAMPLE_SINGLE);
    assert_eq!(v, MboxVariant::MboxO);
}

#[test]
fn test_detect_variant_mboxcl() {
    let v = detect_variant(SAMPLE_MBOXCL);
    assert_eq!(v, MboxVariant::MboxCl);
}

#[test]
fn test_detect_variant_mboxcl2() {
    let v = detect_variant(SAMPLE_MBOXCL2);
    assert_eq!(v, MboxVariant::MboxCl2);
}

#[test]
fn test_parse_single_message() {
    let messages = parse_mbox(SAMPLE_SINGLE.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.subject, "Hello");
    assert_eq!(msg.sender_email, "alice@example.com");
    assert_eq!(msg.recipients.len(), 1);
    assert_eq!(msg.recipients[0], "Bob <bob@example.com>");
    assert!(msg.body_plain.contains("Hello Bob"));
    assert!(msg.body_plain.contains("This is a test message."));
    assert!(msg.sent_time.is_some());
    assert_eq!(msg.attachments.len(), 0);
}

#[test]
fn test_parse_mboxrd_unescaping() {
    let messages = parse_mbox(SAMPLE_MBOXRD.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 2);

    let first = &messages[0];
    assert_eq!(first.subject, "Forwarded note");
    assert!(!first.body_plain.contains(">From "));
    assert!(!first.body_plain.contains(">From:"));
    assert!(first.body_plain.contains("From charlie@example.com"));
    assert!(first.body_plain.contains("Original message content here."));

    let second = &messages[1];
    assert_eq!(second.subject, "Re: Forwarded note");
    assert!(second.body_plain.contains("Got it, thanks!"));
}

#[test]
fn test_parse_multipart_with_attachment() {
    let messages = parse_mbox(SAMPLE_MULTIPART.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.subject, "Document with attachment");
    assert!(msg
        .body_plain
        .contains("Please find the document attached."));
    assert_eq!(msg.attachments.len(), 1);

    let att = &msg.attachments[0];
    assert_eq!(att.name, "data.bin");
    assert_eq!(att.mime_type, "application/octet-stream");
    assert_eq!(att.data, b"Hello World!");
    assert_eq!(att.size, 12);
}

#[test]
fn test_parse_mboxcl() {
    let messages = parse_mbox(SAMPLE_MBOXCL.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].subject, "CL test");
    assert_eq!(messages[1].subject, "Second CL test");
}

#[test]
fn test_parse_mboxcl2() {
    let messages = parse_mbox(SAMPLE_MBOXCL2.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 1);

    let msg = &messages[0];
    assert_eq!(msg.subject, "CL2 test");
    assert_eq!(msg.sender_email, "sender@example.com");
}

#[test]
fn test_parse_empty() {
    let messages = parse_mbox(SAMPLE_EMPTY.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 0);
}

#[test]
fn test_parse_address_name_and_email() {
    let (name, email) = parse_address("\"Alice Johnson\" <alice@example.com>");
    assert_eq!(name, "Alice Johnson");
    assert_eq!(email, "alice@example.com");
}

#[test]
fn test_parse_address_email_only() {
    let (name, email) = parse_address("bob@example.com");
    assert_eq!(name, "");
    assert_eq!(email, "bob@example.com");
}

#[test]
fn test_unescape_from_lines_no_escape() {
    let input = "Hello\nWorld\n";
    let output = unescape_from_lines(input);
    assert_eq!(output, "Hello\nWorld\n");
}

#[test]
fn test_unescape_from_lines_single_level() {
    let input = "Line 1\n>From alice@example.com Mon Jun 16 10:00:00 2025\nLine 3\n";
    let output = unescape_from_lines(input);
    assert_eq!(
        output,
        "Line 1\nFrom alice@example.com Mon Jun 16 10:00:00 2025\nLine 3\n"
    );
}

#[test]
fn test_unescape_from_lines_double_escape() {
    let input = ">>From alice@example.com Mon Jun 16 10:00:00 2025\n";
    let output = unescape_from_lines(input);
    assert_eq!(output, ">From alice@example.com Mon Jun 16 10:00:00 2025\n");
}

#[test]
fn test_unescape_from_lines_non_from_greater_than() {
    let input = "> This is a quote\n>From escaped\n";
    let output = unescape_from_lines(input);
    assert_eq!(output, "> This is a quote\nFrom escaped\n");
}

#[test]
fn test_is_message_separator_valid() {
    assert!(is_message_separator(
        "From alice@example.com Fri Jun 13 10:00:00 2025"
    ));
}

#[test]
fn test_is_message_separator_invalid() {
    assert!(!is_message_separator("From alice@example.com"));
    assert!(!is_message_separator("From: Alice <alice@example.com>"));
    assert!(!is_message_separator("From the beginning"));
}

#[test]
fn test_split_headers_and_body() {
    let raw = "From: alice@example.com\nSubject: Test\n\nBody text here\n";
    let (headers, body) = split_headers_and_body(raw);
    assert!(headers.contains("From:"));
    assert!(headers.contains("Subject:"));
    assert_eq!(body.trim(), "Body text here");
}

#[test]
fn test_find_header() {
    let headers = "From: Alice <alice@example.com>\nSubject: Test\nTo: Bob <bob@example.com>";
    assert_eq!(find_header(headers, "Subject").unwrap(), "Test");
    assert_eq!(find_header(headers, "To").unwrap(), "Bob <bob@example.com>");
    assert!(find_header(headers, "X-Unknown").is_none());
}

#[test]
fn test_find_header_folded() {
    let headers = "Subject: This is a very long subject\n line that continues\nTo: Bob";
    let val = find_header(headers, "Subject").unwrap();
    assert_eq!(val, "This is a very long subject line that continues");
}

#[test]
fn test_quoted_printable_decode() {
    let input = "Hello=20World=21";
    let output = quoted_printable_decode(input);
    assert_eq!(String::from_utf8_lossy(&output), "Hello World!");
}

#[test]
fn test_extract_boundary() {
    let ct = r#"multipart/mixed; boundary="----=_NextPart_001""#;
    let b = extract_boundary(ct).unwrap();
    assert_eq!(b, "----=_NextPart_001");
}

#[test]
fn test_extract_boundary_no_quotes() {
    let ct = "multipart/alternative; boundary=boundary123";
    let b = extract_boundary(ct).unwrap();
    assert_eq!(b, "boundary123");
}

#[test]
fn test_extract_filename_from_disposition() {
    let cd = r#"attachment; filename="report.pdf""#;
    let ct = "application/pdf";
    let name = extract_filename(cd, ct);
    assert_eq!(name, "report.pdf");
}

#[test]
fn test_extract_filename_from_content_type() {
    let cd = "inline";
    let ct = r#"application/octet-stream; name="data.bin""#;
    let name = extract_filename(cd, ct);
    assert_eq!(name, "data.bin");
}

#[test]
fn test_parse_mbox_returns_mbox_message_struct() {
    let messages = parse_mbox(SAMPLE_SINGLE.as_bytes()).expect("parse should succeed");
    let msg = &messages[0];

    assert!(!msg.subject.is_empty());
    assert!(!msg.body_plain.is_empty());
    assert!(msg.body_html.is_empty());
    assert!(!msg.sender_name.is_empty() || !msg.sender_email.is_empty());
    assert!(!msg.recipients.is_empty());
    assert!(msg.sent_time.is_some());
    assert!(msg.received_time.is_none());
    assert_eq!(msg.attachments.len(), 0);
    assert_eq!(msg.folder_path, "");
}

#[test]
fn test_parse_mbox_handles_html_content() {
    let sample = "\
From sender@example.com Mon Jun 16 15:00:00 2025
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: HTML test
Date: Mon, 16 Jun 2025 15:00:00 +0200
Content-Type: text/html

<html><body><h1>Hello</h1><p>HTML message</p></body></html>
";
    let messages = parse_mbox(sample.as_bytes()).expect("parse should succeed");
    let msg = &messages[0];
    assert!(!msg.body_html.is_empty());
    assert!(msg.body_html.contains("<h1>Hello</h1>"));
}

#[test]
fn test_parse_mbox_handles_multipart_alternative() {
    let sample = "\
From sender@example.com Mon Jun 16 16:00:00 2025
From: Sender <sender@example.com>
To: Recipient <recipient@example.com>
Subject: Multipart alternative
Date: Mon, 16 Jun 2025 16:00:00 +0200
Content-Type: multipart/alternative; boundary=altboundary

--altboundary
Content-Type: text/plain

Plain text version.

--altboundary
Content-Type: text/html

<html><body><p>HTML version.</p></body></html>

--altboundary--
";
    let messages = parse_mbox(sample.as_bytes()).expect("parse should succeed");
    let msg = &messages[0];
    assert!(msg.body_plain.contains("Plain text version."));
    assert!(msg.body_html.contains("<p>HTML version.</p>"));
}

#[test]
fn test_parse_mbox_handles_no_blank_line_separator() {
    let sample = "From sender@example.com Mon Jun 16 17:00:00 2025\nSubject: Minimal";
    let messages = parse_mbox(sample.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].subject, "Minimal");
}

#[test]
fn test_parse_mbox_handles_crlf() {
    let sample = "\
From sender@example.com Mon Jun 16 18:00:00 2025\r\n\
From: Sender <sender@example.com>\r\n\
To: Recipient <recipient@example.com>\r\n\
Subject: CRLF test\r\n\
Date: Mon, 16 Jun 2025 18:00:00 +0200\r\n\
\r\n\
Body with CRLF line endings.\r\n\
";
    let messages = parse_mbox(sample.as_bytes()).expect("parse should succeed");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].subject, "CRLF test");
    assert!(messages[0]
        .body_plain
        .contains("Body with CRLF line endings."));
}

#[test]
fn test_variant_detection_handles_no_escape_no_cl() {
    let v = detect_variant(SAMPLE_SINGLE);
    assert_eq!(v, MboxVariant::MboxO);
}

#[test]
fn test_parse_address_with_angle_brackets_but_no_name() {
    let (name, email) = parse_address("<alice@example.com>");
    assert_eq!(email, "alice@example.com");
    assert_eq!(name, "");
}

#[test]
fn test_parse_date_iso_format() {
    let date = parse_email_date("2025-06-16T10:30:00");
    assert!(date.is_some());
}

#[test]
fn test_parse_date_invalid_returns_none() {
    let date = parse_email_date("not a date");
    assert!(date.is_none());
}
