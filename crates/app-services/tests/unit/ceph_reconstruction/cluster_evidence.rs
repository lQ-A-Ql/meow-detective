use super::*;

fn evidence(
    source_id: &str,
    inventory_id: &str,
    osd_id: Option<u32>,
    fsid: Option<&str>,
) -> InventoryEvidence {
    InventoryEvidence {
        source_id: source_id.to_string(),
        inventory_id: inventory_id.to_string(),
        identity: ReplicaIdentity::from_inventory(
            osd_id,
            format!("osd-{inventory_id}"),
            fsid.map(str::to_string),
        ),
    }
}

fn policy(expected_count: u32) -> RbdReplicaPolicy {
    if expected_count == 3 {
        RbdReplicaPolicy::strict_legacy()
    } else {
        RbdReplicaPolicy::trusted_pool(8, expected_count, 1, "unit-test", None, None)
            .expect("valid pool evidence")
    }
}

#[test]
fn complete_identity_set_is_verified() {
    let report = assess_inventory_coverage(
        &[
            evidence("source-a", "inventory-a", Some(1), Some("fsid")),
            evidence("source-b", "inventory-b", Some(2), Some("fsid")),
            evidence("source-c", "inventory-c", Some(3), Some("fsid")),
        ],
        &policy(3),
    );
    assert_eq!(report.state, InventoryCoverageState::Complete);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn missing_identity_is_indeterminate_not_complete() {
    let report = assess_inventory_coverage(
        &[
            evidence("source-a", "inventory-a", Some(1), Some("fsid")),
            evidence("source-b", "inventory-b", None, Some("fsid")),
        ],
        &policy(2),
    );
    assert_eq!(report.state, InventoryCoverageState::Indeterminate);
    assert!(report
        .diagnostics
        .iter()
        .any(|message| message.contains("lack complete")));
}

#[test]
fn duplicate_osd_or_conflicting_fsid_is_conflicted() {
    let report = assess_inventory_coverage(
        &[
            evidence("source-a", "inventory-a", Some(1), Some("fsid-a")),
            evidence("source-b", "inventory-b", Some(1), Some("fsid-b")),
        ],
        &policy(2),
    );
    assert_eq!(report.state, InventoryCoverageState::Conflicted);
    assert_eq!(report.duplicate_osd_ids, vec![1]);
    assert_eq!(report.ceph_fsids, vec!["fsid-a", "fsid-b"]);
}

#[test]
fn count_mismatch_is_incomplete_even_when_identity_is_known() {
    let report = assess_inventory_coverage(
        &[evidence("source-a", "inventory-a", Some(1), Some("fsid"))],
        &policy(3),
    );
    assert_eq!(report.state, InventoryCoverageState::Incomplete);
    assert_eq!(report.observed_count, 1);
}

#[test]
fn trusted_pool_policy_uses_pool_size_and_preserves_provenance() {
    let policy =
        RbdReplicaPolicy::trusted_pool(8, 2, 1, "osdmap:epoch-17", Some(17), Some("A".repeat(64)))
            .expect("valid pool evidence");
    let report = assess_inventory_coverage(
        &[
            evidence("source-a", "inventory-a", Some(1), Some("fsid")),
            evidence("source-b", "inventory-b", Some(2), Some("fsid")),
        ],
        &policy,
    );

    assert_eq!(report.expected_count, 2);
    assert_eq!(report.policy.storage_key(), "trusted_pool_size");
    assert_eq!(report.policy_source(), "pool_map_or_trusted_artifact");
    let pool = report.policy.pool_evidence().expect("pool evidence");
    assert_eq!(pool.pool_id(), 8);
    assert_eq!(pool.epoch(), Some(17));
    assert_eq!(pool.evidence_digest(), Some("a".repeat(64).as_str()));
    assert_eq!(report.state, InventoryCoverageState::Complete);
}

#[test]
fn trusted_pool_policy_rejects_unverifiable_geometry() {
    assert!(RbdReplicaPolicy::trusted_pool(8, 0, 1, "osdmap", None, None).is_err());
    assert!(RbdReplicaPolicy::trusted_pool(8, 2, 3, "osdmap", None, None).is_err());
    assert!(RbdReplicaPolicy::trusted_pool(8, 2, 1, "", None, None).is_err());
    assert!(RbdReplicaPolicy::trusted_pool(8, 2, 1, r"C:\secret\map", None, None).is_err());
    assert!(RbdReplicaPolicy::trusted_pool(8, 2, 1, "osdmap\0", None, None).is_err());
    assert!(RbdReplicaPolicy::trusted_pool(8, 2, 1, "osdmap", None, Some("bad".into())).is_err());
}

#[test]
fn trusted_pool_policy_is_bound_to_the_rbd_data_pool() {
    let policy = RbdReplicaPolicy::trusted_pool(8, 2, 1, "osdmap", Some(11), None)
        .expect("valid pool evidence");
    assert!(policy.validate_for_pool(8).is_ok());
    assert!(matches!(
        policy.validate_for_pool(9),
        Err(ReplicaPolicyError::PoolMismatch)
    ));
}
