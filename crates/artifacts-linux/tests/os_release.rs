use artifacts_linux::{parse_os_release, parse_os_release_bytes, MAX_OS_RELEASE_BYTES};

#[test]
fn duplicate_fields_quotes_escapes_and_literal_variables_share_posix_semantics() {
    let input = r#"ID=old
ID=ubuntu
NAME='Ubuntu Linux'
PRETTY_NAME="Ubuntu \"Server\""
VERSION_ID='24.04'
LITERAL='C:\path\$HOME'
ESCAPE="a\qb"
VARIABLE="$HOME"
UNKNOWN=value
"#;
    let result = parse_os_release(input).unwrap();
    assert_eq!(result.id.as_deref(), Some("ubuntu"));
    assert_eq!(result.pretty_name.as_deref(), Some("Ubuntu \"Server\""));
    assert_eq!(result.fields["LITERAL"], r"C:\path\$HOME");
    assert_eq!(result.fields["ESCAPE"], r"a\qb");
    assert_eq!(result.fields["VARIABLE"], "$HOME");
    assert_eq!(result.fields["UNKNOWN"], "value");
}

#[test]
fn invalid_encoding_quoting_and_missing_identity_return_errors() {
    for bytes in [
        b"ID=\xff".as_slice(),
        b"ID=\"ubuntu",
        b"garbage",
        b"# comment",
        b"ID=ubuntu\0",
        b"ID=ubuntu extra",
    ] {
        assert!(parse_os_release_bytes(bytes).is_err(), "{bytes:?}");
    }
    let bytes = vec![b'a'; MAX_OS_RELEASE_BYTES + 1];
    assert!(parse_os_release_bytes(&bytes).is_err());
}

#[test]
fn byte_and_text_entry_points_return_identical_fields() {
    let content = b"ID=debian\r\nNAME='Debian GNU/Linux'\r\nVERSION_ID=12\r\n";
    assert_eq!(
        parse_os_release_bytes(content).unwrap(),
        parse_os_release(std::str::from_utf8(content).unwrap()).unwrap()
    );
}
