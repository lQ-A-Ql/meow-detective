use containers_pst::{
    mbox::{parse_mbox, visit_mbox},
    message::{emlx_payload, parse_message, MAX_MESSAGE_BYTES},
    PstError,
};

const RAW: &[u8] = b"From: =?UTF-8?Q?Alice?= <alice@example.com>\r\nTo: Team: Bob <bob@example.com>;\r\nSubject: =?UTF-8?Q?R=C3=A9sum=C3=A9?=\r\nMessage-ID: <one@example.com>\r\nDate: Fri, 13 Jun 2025 10:00:00 +0200\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=x\r\n\r\n--x\r\nContent-Type: text/plain; charset=iso-8859-1\r\nContent-Transfer-Encoding: quoted-printable\r\n\r\nCaf=E9\r\n--x\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename*=utf-8''%E6%96%87%E4%BB%B6.bin\r\nContent-Transfer-Encoding: base64\r\n\r\nAAECAw==\r\n--x--\r\n";

#[test]
fn eml_emlx_and_mbox_share_fields_attachments_and_source_ranges() {
    let direct = parse_message(RAW, false).unwrap();
    let mut emlx = format!("{}\n", RAW.len()).into_bytes();
    emlx.extend_from_slice(RAW);
    emlx.extend_from_slice(b"<plist/>");
    let (payload, offset) = emlx_payload(&emlx).unwrap();
    assert_eq!(payload, RAW);
    assert_eq!(offset, RAW.len().to_string().len() + 1);
    assert_eq!(parse_message(payload, false).unwrap(), direct);
    let envelope = b"From envelope@example.com Fri Jun 13 10:00:00 2025\r\n";
    let mut mbox = envelope.to_vec();
    mbox.extend_from_slice(RAW);
    let mut observed = 0;
    visit_mbox(&mbox, false, |record| {
        assert_eq!(record.message, direct);
        assert_eq!(record.source.source_range.start, envelope.len() as u64);
        assert_eq!(record.source.envelope_sender, "envelope@example.com");
        observed += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(observed, 1);
    assert_eq!(direct.subject, "Résumé");
    assert_eq!(direct.body_plain.as_deref(), Some("Café"));
    assert_eq!(direct.attachments[0].name.as_deref(), Some("文件.bin"));
    assert_eq!(direct.attachments[0].decoded_size, 4);
    assert!(direct.attachments[0].data.is_none());
    let body = &RAW[direct.attachments[0].body_range.clone()];
    assert_eq!(body, b"AAECAw==");
    let decoded = parse_message(RAW, true).unwrap();
    assert_eq!(
        decoded.attachments[0].data.as_deref(),
        Some([0, 1, 2, 3].as_slice())
    );
}

#[test]
fn non_utf8_mbox_body_is_preserved_and_decoded_by_declared_charset() {
    let raw = b"From a@example.com Fri Jun 13 10:00:00 2025\nFrom: a@example.com\nContent-Type: text/plain; charset=iso-8859-1\n\nCaf\xe9\n";
    let messages = parse_mbox(raw).unwrap();
    assert_eq!(messages[0].body_plain.trim(), "Café");
}

#[test]
fn structural_and_byte_limits_return_errors_and_emlx_checks_length() {
    assert!(matches!(
        parse_message(&vec![b'a'; MAX_MESSAGE_BYTES + 1], false),
        Err(PstError::MessageLimit(_))
    ));
    assert!(emlx_payload(b"100\nSubject: short\n\nx").is_err());
    let mut nested = b"Content-Type: text/plain\r\n\r\nbody".to_vec();
    for depth in 0..40 {
        let mut wrapper =
            format!("Content-Type: multipart/mixed; boundary=b{depth}\r\n\r\n--b{depth}\r\n")
                .into_bytes();
        wrapper.extend(nested);
        wrapper.extend_from_slice(format!("\r\n--b{depth}--\r\n").as_bytes());
        nested = wrapper;
    }
    assert!(matches!(
        parse_message(&nested, false),
        Err(PstError::MessageLimit(_))
    ));
}
