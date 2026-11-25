use artifacts_linux::ssh::{
    parse_authorized_keys_metadata, parse_known_hosts_metadata, parse_ssh_key_metadata,
};

// Public Ed25519 example distributed by RustCrypto ssh-key under MIT OR Apache-2.0.
const PUBLIC_KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILM+rvN+ot98qgEN796jTiQfZfG1KaT0PtFDJ/XFSqti user@example.com";

#[test]
fn public_key_and_configuration_keep_the_same_identity_and_options() {
    let key = parse_ssh_key_metadata(PUBLIC_KEY.as_bytes(), true).unwrap();
    assert_eq!(key.algorithm, "ssh-ed25519");
    assert_eq!(key.comment, "user@example.com");
    let authorized = parse_authorized_keys_metadata(&format!("no-pty {PUBLIC_KEY}\n")).unwrap();
    assert_eq!(authorized[0].fingerprint, key.fingerprint);
    assert!(authorized[0]
        .configuration
        .as_ref()
        .unwrap()
        .starts_with("no-pty "));
    let hosts =
        parse_known_hosts_metadata(&format!("@revoked example.org {PUBLIC_KEY}\n")).unwrap();
    assert_eq!(hosts[0].fingerprint, key.fingerprint);
    assert!(hosts[0]
        .configuration
        .as_ref()
        .unwrap()
        .starts_with("@revoked example.org "));
}

#[test]
fn empty_configurations_are_valid_and_damaged_keys_are_rejected() {
    assert!(parse_authorized_keys_metadata("# no entries\n")
        .unwrap()
        .is_empty());
    assert!(parse_known_hosts_metadata("").unwrap().is_empty());
    assert!(parse_ssh_key_metadata(b"ssh-ed25519 invalid", true).is_err());
    assert!(parse_ssh_key_metadata(b"invalid private key", false).is_err());
}

#[test]
fn malformed_configuration_entries_return_provider_errors() {
    assert!(parse_authorized_keys_metadata("no-pty ssh-ed25519 invalid\n").is_err());
    assert!(parse_known_hosts_metadata("@revoked example.org ssh-ed25519 invalid\n").is_err());
}
