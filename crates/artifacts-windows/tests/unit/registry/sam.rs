use super::*;
use crate::registry::tests::txlog_fixture::{build_synthetic_log1, SyntheticEntry};
use crate::registry::tests::*;

#[test]
fn extracts_users_groups_and_policy_from_synthetic_hive() {
    let info =
        extract_sam_fields(&synthetic_sam_hive(), "Windows/System32/config/SAM", None).unwrap();
    assert_eq!(info.users.len(), 2);
    assert_eq!(info.groups.len(), 4);
    assert_eq!(
        info.password_policy.as_ref().unwrap().max_password_age_days,
        42
    );
    assert_eq!(
        info.password_policy.as_ref().unwrap().min_password_length,
        8
    );
}

#[test]
fn preserves_sam_rid_and_account_control_semantics() {
    let info =
        extract_sam_fields(&synthetic_sam_hive(), "Windows/System32/config/SAM", None).unwrap();
    let administrator = info
        .users
        .iter()
        .find(|user| user.username == "Administrator")
        .unwrap();
    let guest = info
        .users
        .iter()
        .find(|user| user.username == "Guest")
        .unwrap();
    assert_eq!(administrator.rid, 500);
    assert_eq!(guest.rid, 501);
    assert!(!administrator.account_disabled);
    assert!(guest.account_disabled);
    assert!(!administrator.account_locked);
    assert!(!guest.account_locked);
}

#[test]
fn projects_machine_sid_and_user_memberships() {
    let info =
        extract_sam_fields(&synthetic_sam_hive(), "Windows/System32/config/SAM", None).unwrap();
    let administrator = info
        .users
        .iter()
        .find(|user| user.username == "Administrator")
        .unwrap();
    assert_eq!(
        administrator.sid,
        "S-1-5-21-123456789-123456789-123456789-500"
    );
    assert!(administrator
        .group_memberships
        .iter()
        .any(|membership| membership == "Administrators"));
}

#[test]
fn converts_sam_filetimes() {
    let info =
        extract_sam_fields(&synthetic_sam_hive(), "Windows/System32/config/SAM", None).unwrap();
    let administrator = info
        .users
        .iter()
        .find(|user| user.username == "Administrator")
        .unwrap();
    assert!(administrator.last_login.is_some());
    assert!(administrator.password_last_set.is_some());
}

#[test]
fn empty_hive_is_nonfatal_and_warns() {
    let info = extract_sam_fields(&empty_hive("SAM"), "not/sam", None).unwrap();
    assert!(info.users.is_empty());
    assert!(info
        .warnings
        .iter()
        .any(|warning| warning.contains("no user names found")));
}

#[test]
fn short_v_record_is_reported_without_panic() {
    let mut data = synthetic_sam_hive();
    let value_absolute = 0x1000 + 0x5000;
    data[value_absolute..value_absolute + 4].copy_from_slice(&(-8i32).to_le_bytes());
    data[value_absolute + 4..value_absolute + 8].fill(0);
    let value_record_absolute = 0x1000 + 0x1140;
    data[value_record_absolute + 8..value_record_absolute + 12]
        .copy_from_slice(&4u32.to_le_bytes());
    let info = extract_sam_fields(&data, "Windows/System32/config/SAM", None).unwrap();
    // A truncated V degrades the profile fields only; the F value still
    // drives identity, timestamps and account-control flags.
    let administrator = info
        .users
        .iter()
        .find(|user| user.username == "Administrator")
        .unwrap();
    assert_eq!(administrator.rid, 500);
    assert!(!administrator.account_disabled);
    assert!(administrator.last_login.is_some());
}

#[test]
fn txlog_overrides_password_policy() {
    let data = synthetic_sam_hive();
    let replacement = make_domain_account_f_blob(90, 2, 14, 30, 3, 60, 45);
    let log = build_synthetic_log1(&[SyntheticEntry {
        operation: 2,
        sequence_number: 50,
        timestamp: Some(0x01db_a000_0000_0000),
        key_path: r"SAM\Domains\Account".to_string(),
        value_name: Some("F".to_string()),
        data_before: None,
        data_after: Some(replacement),
    }]);
    let info =
        extract_sam_fields_with_txlog(&data, "Windows/System32/config/SAM", None, Some(&log), None)
            .unwrap();
    let policy = info.password_policy.unwrap();
    assert_eq!(policy.max_password_age_days, 90);
    assert_eq!(policy.min_password_length, 14);
    assert!(info.txlog_applied);
    assert!(info
        .txlog_timestamps
        .iter()
        .any(|timestamp| timestamp.field_name == "passwordPolicy" && timestamp.txlog_used));
}

#[test]
fn corrupt_txlog_is_nonfatal() {
    let info = extract_sam_fields_with_txlog(
        &synthetic_sam_hive(),
        "Windows/System32/config/SAM",
        None,
        Some(b"bad"),
        None,
    )
    .unwrap();
    assert_eq!(info.users.len(), 2);
    assert!(info
        .warnings
        .iter()
        .any(|warning| warning.contains("LOG1 parse failed")));
}

#[test]
fn malformed_or_missing_account_values_remain_nonfatal() {
    let mut unexpected_type = synthetic_sam_hive();
    write_vk(
        &mut unexpected_type,
        0x1140,
        "V",
        1,
        0x8000_0004,
        0x4242_4242,
    );
    let info = extract_sam_fields(&unexpected_type, "Windows/System32/config/SAM", None).unwrap();
    assert!(info
        .warnings
        .iter()
        .any(|warning| warning.contains("unexpected type")));

    let mut missing_policy = synthetic_sam_hive();
    write_nk(
        &mut missing_policy,
        0x180,
        "Account",
        &[("Users", 0x200), ("Aliases", 0x500)],
        &[],
    );
    let info = extract_sam_fields(&missing_policy, "Windows/System32/config/SAM", None).unwrap();
    assert!(info.password_policy.is_none());
    assert_eq!(info.users.len(), 2);
    assert_eq!(info.groups.len(), 4);
}
