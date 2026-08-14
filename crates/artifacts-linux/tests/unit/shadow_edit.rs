use super::*;

const SHADOW: &str = "root:$6$saltsalt$abcdefghij:19000:0:99999:7:::\n\
                      daemon:*:19000:0:99999:7:::\n\
                      lockeduser:!$6$abc:19000:0:99999:7:::\n\
                      nopass::19000:0:99999:7:::\n";
const PASSWORD_HASH: &str = "$6$meow1234$Ece2JtWkjNGCiGYoIvqBZ8teI2U1Lmd73FwcHlczR6zRf0q8ET2EdwZ6ZaEz0WZ196VlNUTZk240LtfFdViux1";

#[test]
fn parses_account_password_states() {
    let accounts = parse_shadow_accounts(SHADOW);
    assert_eq!(accounts.len(), 4);
    assert!(accounts[0].has_password && !accounts[0].locked);
    assert_eq!(accounts[1].username, "daemon");
    assert!(!accounts[1].has_password && !accounts[1].locked);
    assert!(accounts[2].locked && !accounts[2].has_password);
    assert!(!accounts[3].has_password);
}

#[test]
fn sets_only_the_target_hash_and_preserves_all_other_bytes() {
    let edited = set_shadow_password_hash(SHADOW, "root", PASSWORD_HASH)
        .expect("edit parses")
        .expect("edit applies");
    assert!(edited.starts_with(&format!("root:{PASSWORD_HASH}:19000:0:99999:7:::\n")));
    assert!(edited.contains("daemon:*:19000:0:99999:7:::"));
    assert!(edited.ends_with(":::\n"), "trailing newline preserved");
}

#[test]
fn refuses_absent_or_already_configured_accounts() {
    assert_eq!(
        set_shadow_password_hash(SHADOW, "nobody", PASSWORD_HASH).unwrap(),
        None
    );
    let configured = format!("root:{PASSWORD_HASH}:1:2:3:::");
    assert_eq!(
        set_shadow_password_hash(&configured, "root", PASSWORD_HASH).unwrap(),
        None
    );
    assert!(set_shadow_password_hash(SHADOW, "bad:name", PASSWORD_HASH).is_err());
    assert!(set_shadow_password_hash(SHADOW, "", PASSWORD_HASH).is_err());
    assert!(set_shadow_password_hash(SHADOW, "root", "").is_err());
    assert!(set_shadow_password_hash(SHADOW, "root", "!locked").is_err());
}

#[test]
fn sets_password_for_empty_and_locked_fields() {
    let passwordless = set_shadow_password_hash(SHADOW, "nopass", PASSWORD_HASH)
        .unwrap()
        .unwrap();
    assert!(passwordless.contains(&format!("nopass:{PASSWORD_HASH}:")));
    let unlocked = set_shadow_password_hash(SHADOW, "lockeduser", PASSWORD_HASH)
        .unwrap()
        .unwrap();
    assert!(unlocked.contains(&format!("lockeduser:{PASSWORD_HASH}:")));
}

#[test]
fn handles_missing_trailing_newline() {
    let content = "root:$6$abc:1:2:3:::";
    let edited = set_shadow_password_hash(content, "root", PASSWORD_HASH)
        .unwrap()
        .unwrap();
    assert_eq!(edited, format!("root:{PASSWORD_HASH}:1:2:3:::"));
}
