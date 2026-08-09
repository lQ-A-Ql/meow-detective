use super::*;

const SHADOW: &str = "root:$6$saltsalt$abcdefghij:19000:0:99999:7:::\n\
                      daemon:*:19000:0:99999:7:::\n\
                      lockeduser:!$6$abc:19000:0:99999:7:::\n\
                      nopass::19000:0:99999:7:::\n";

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
fn clears_only_the_target_hash_and_preserves_all_other_bytes() {
    let cleared = clear_shadow_password(SHADOW, "root")
        .expect("edit parses")
        .expect("edit applies");
    assert!(cleared.starts_with("root::19000:0:99999:7:::\n"));
    assert!(cleared.contains("daemon:*:19000:0:99999:7:::"));
    assert!(cleared.len() < SHADOW.len());
    assert!(cleared.ends_with(":::\n"), "trailing newline preserved");
}

#[test]
fn refuses_absent_or_already_passwordless_accounts() {
    assert_eq!(clear_shadow_password(SHADOW, "nobody").unwrap(), None);
    assert_eq!(clear_shadow_password(SHADOW, "nopass").unwrap(), None);
    assert!(clear_shadow_password(SHADOW, "bad:name").is_err());
    assert!(clear_shadow_password(SHADOW, "").is_err());
}

#[test]
fn handles_missing_trailing_newline() {
    let content = "root:$6$abc:1:2:3:::";
    let cleared = clear_shadow_password(content, "root").unwrap().unwrap();
    assert_eq!(cleared, "root::1:2:3:::");
}
