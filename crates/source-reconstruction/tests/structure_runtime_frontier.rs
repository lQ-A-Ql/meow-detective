use std::collections::BTreeSet;

use domain::structure::{
    ByteCoverage, ByteRange, CapabilityIntegrity, CapabilityKind, CapabilityRef, TargetCoverage,
};
use source_reconstruction::structure_runtime::frontier::{
    BindingPrerequisite, CapabilityEntry, CapabilityIndex, Frontier, FrontierError,
    FrontierResource, PendingBindings,
};

fn entry(
    target: &str,
    revision: u64,
    scope: &str,
    kind: CapabilityKind,
    roles: &[&str],
) -> CapabilityEntry {
    let range = ByteRange::new(4096, 512).unwrap();
    CapabilityEntry {
        capability: CapabilityRef::new(
            target.into(),
            format!("{target}-view"),
            kind,
            revision,
            CapabilityIntegrity::NativeVerified {
                verifier_id: "native-metadata-verifier-v1".into(),
                witness_sha256: "a".repeat(64),
            },
            TargetCoverage::Bytes(ByteCoverage {
                target_size: 65536,
                requested: vec![range],
                verified: vec![range],
                native_holes: Vec::new(),
                unknown: Vec::new(),
                unreadable: Vec::new(),
            }),
        )
        .unwrap(),
        scope_id: scope.into(),
        native_roles: roles.iter().map(|role| (*role).into()).collect(),
    }
}

fn member(target: &str, scope: &str, roles: &[&str]) -> CapabilityEntry {
    entry(target, 1, scope, CapabilityKind::MemberMetadata, roles)
}

fn prerequisite(scope: &str, kind: CapabilityKind, role: &str) -> BindingPrerequisite {
    BindingPrerequisite {
        scope_id: scope.into(),
        kind,
        native_role: role.into(),
    }
}

fn member_prerequisite(scope: &str, role: &str) -> BindingPrerequisite {
    prerequisite(scope, CapabilityKind::MemberMetadata, role)
}

#[test]
fn constructors_reject_zero_limits_for_every_bounded_collection() {
    assert!(matches!(
        CapabilityIndex::new(0),
        Err(FrontierError::InvalidLimit {
            resource: FrontierResource::CapabilityEntries,
        })
    ));
    assert!(matches!(
        CapabilityIndex::with_limits(1, 0),
        Err(FrontierError::InvalidLimit {
            resource: FrontierResource::CapabilityRoles,
        })
    ));
    assert!(matches!(
        Frontier::new(0),
        Err(FrontierError::InvalidLimit {
            resource: FrontierResource::QueuedCapabilities,
        })
    ));
    assert!(matches!(
        PendingBindings::new(0),
        Err(FrontierError::InvalidLimit {
            resource: FrontierResource::PendingCandidates,
        })
    ));
    assert!(matches!(
        PendingBindings::with_limits(1, 0),
        Err(FrontierError::InvalidLimit {
            resource: FrontierResource::PendingPrerequisites,
        })
    ));
}

#[test]
fn capability_publication_is_immutable_and_idempotent_even_at_capacity() {
    let original = member("member-1", "collection-1", &["data"]);
    let id = original.capability.id.clone();
    let mut index = CapabilityIndex::new(1).unwrap();
    assert!(index.is_empty());
    assert!(index.get("not-published").unwrap().is_none());
    assert!(index.publish(original.clone()).unwrap());
    assert!(!index.publish(original.clone()).unwrap());
    assert_eq!(index.len(), 1);
    assert_eq!(index.get(&id).unwrap(), Some(&original));
    let matching = index
        .matching(&member_prerequisite("collection-1", "data"))
        .unwrap();
    assert_eq!(matching, vec![&original]);
}

#[test]
fn same_capability_id_cannot_change_its_scope_roles_or_payload() {
    let original = member("member-1", "collection-1", &["data"]);
    let mut index = CapabilityIndex::new(1).unwrap();
    index.publish(original.clone()).unwrap();
    let mut other_scope = original.clone();
    other_scope.scope_id = "collection-2".into();
    let mut other_roles = original.clone();
    other_roles.native_roles = ["parity".into()].into_iter().collect();
    let mut other_payload = original.clone();
    other_payload.capability.target_id = "different-member".into();
    for changed in [other_scope, other_roles, other_payload] {
        assert!(matches!(
            index.publish(changed),
            Err(FrontierError::CapabilityConflict)
        ));
        assert_eq!(index.len(), 1);
        assert_eq!(index.get(&original.capability.id).unwrap(), Some(&original));
    }
    assert!(index
        .matching(&member_prerequisite("collection-2", "data"))
        .unwrap()
        .is_empty());
    assert!(index
        .matching(&member_prerequisite("collection-1", "parity"))
        .unwrap()
        .is_empty());
}

#[test]
fn capability_matching_requires_the_exact_scope_kind_and_native_role() {
    let entries = [
        member("member-1", "collection-1", &["data"]),
        member("member-2", "collection-1", &["parity"]),
        member("member-3", "collection-2", &["data"]),
        entry(
            "container-1",
            1,
            "collection-1",
            CapabilityKind::ContainerMetadata,
            &["data"],
        ),
    ];
    let mut index = CapabilityIndex::new(4).unwrap();
    for entry in &entries {
        index.publish(entry.clone()).unwrap();
    }
    assert_eq!(
        index
            .matching(&member_prerequisite("collection-1", "data"))
            .unwrap(),
        vec![&entries[0]]
    );
    assert_eq!(
        index
            .matching(&member_prerequisite("collection-2", "data"))
            .unwrap(),
        vec![&entries[2]]
    );
    assert_eq!(
        index
            .matching(&prerequisite(
                "collection-1",
                CapabilityKind::ContainerMetadata,
                "data"
            ))
            .unwrap(),
        vec![&entries[3]]
    );
    assert!(index
        .matching(&member_prerequisite("unpublished-collection", "data"))
        .unwrap()
        .is_empty());
}

#[test]
fn matching_returns_all_immutable_versions_in_deterministic_id_order() {
    let first = entry(
        "member-1",
        1,
        "collection-1",
        CapabilityKind::MemberMetadata,
        &["data"],
    );
    let second = entry(
        "member-1",
        2,
        "collection-1",
        CapabilityKind::MemberMetadata,
        &["data"],
    );
    assert_ne!(first.capability.id, second.capability.id);
    let mut left = CapabilityIndex::new(2).unwrap();
    let mut right = CapabilityIndex::new(2).unwrap();
    left.publish(first.clone()).unwrap();
    left.publish(second.clone()).unwrap();
    right.publish(second.clone()).unwrap();
    right.publish(first.clone()).unwrap();
    let prerequisite = member_prerequisite("collection-1", "data");
    let left_entries = left.matching(&prerequisite).unwrap();
    let right_entries = right.matching(&prerequisite).unwrap();
    assert_eq!(left_entries, right_entries);
    assert_eq!(left_entries.len(), 2);
    assert!(left_entries[0].capability.id < left_entries[1].capability.id);
}

#[test]
fn index_capacity_rejection_preserves_entries_and_secondary_indices() {
    let original = member("member-1", "collection-1", &["data"]);
    let rejected = member("member-2", "collection-2", &["parity"]);
    let mut index = CapabilityIndex::new(1).unwrap();
    index.publish(original.clone()).unwrap();
    assert!(matches!(
        index.publish(rejected.clone()),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::CapabilityEntries,
            required: 2,
            limit: 1,
        })
    ));
    assert_eq!(index.len(), 1);
    assert_eq!(index.get(&original.capability.id).unwrap(), Some(&original));
    assert!(index.get(&rejected.capability.id).unwrap().is_none());
    assert!(index
        .matching(&member_prerequisite("collection-2", "parity"))
        .unwrap()
        .is_empty());
}

#[test]
fn index_role_associations_have_their_own_bound_and_reject_atomically() {
    let original = member("member-1", "collection-1", &["data"]);
    let rejected = member("member-2", "collection-1", &["parity", "replica"]);
    let mut index = CapabilityIndex::with_limits(3, 2).unwrap();
    index.publish(original.clone()).unwrap();
    assert!(matches!(
        index.publish(rejected.clone()),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::CapabilityRoles,
            required: 3,
            limit: 2,
        })
    ));
    assert_eq!(index.len(), 1);
    assert!(index.get(&rejected.capability.id).unwrap().is_none());
    for role in ["parity", "replica"] {
        assert!(index
            .matching(&member_prerequisite("collection-1", role))
            .unwrap()
            .is_empty());
    }
    let fitting = member("member-3", "collection-1", &["parity"]);
    assert!(index.publish(fitting.clone()).unwrap());
    assert_eq!(
        index
            .matching(&member_prerequisite("collection-1", "parity"))
            .unwrap(),
        vec![&fitting]
    );
}

#[test]
fn role_capacity_failure_reports_the_exact_requirement_for_a_multi_role_entry() {
    let original = member("member-1", "collection-1", &["data"]);
    let rejected = member(
        "member-2",
        "collection-1",
        &["parity", "replica", "metadata", "journal"],
    );
    let mut index = CapabilityIndex::with_limits(2, 2).unwrap();
    let mut frontier = Frontier::new(2).unwrap();
    index
        .publish_to_frontier(original.clone(), &mut frontier)
        .unwrap();
    assert!(matches!(
        index.publish_batch_to_frontier(vec![rejected.clone()], &mut frontier),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::CapabilityRoles,
            required: 5,
            limit: 2,
        })
    ));
    assert_eq!(index.len(), 1);
    assert_eq!(index.get(&original.capability.id).unwrap(), Some(&original));
    assert!(index.get(&rejected.capability.id).unwrap().is_none());
    assert_eq!(frontier.len(), 1);
    assert_eq!(frontier.pop(), Some(original.capability.id));
}

#[test]
fn index_validates_scope_roles_capability_identity_and_lookup_inputs() {
    let original = member("member-1", "collection-1", &["data"]);
    let mut index = CapabilityIndex::new(5).unwrap();
    let mut bad_scope = original.clone();
    bad_scope.scope_id = " \n".into();
    let mut bad_role = original.clone();
    bad_role.native_roles.insert("".into());
    let mut bad_identity = original.clone();
    bad_identity.capability.id = "invented-capability-id".into();
    for invalid in [bad_scope, bad_role, bad_identity] {
        assert!(index.publish(invalid).is_err());
        assert!(index.is_empty());
    }
    assert!(index.get("").is_err());
    assert!(index.matching(&member_prerequisite("", "data")).is_err());
    assert!(index
        .matching(&member_prerequisite("collection-1", " \t"))
        .is_err());
    let no_roles = member("member-no-role", "collection-1", &[]);
    index.publish(no_roles.clone()).unwrap();
    assert_eq!(index.get(&no_roles.capability.id).unwrap(), Some(&no_roles));
}

#[test]
fn frontier_preserves_fifo_and_deduplicates_by_immutable_capability_id() {
    let first = member("member-1", "collection-1", &["data"]);
    let second = member("member-2", "collection-1", &["parity"]);
    let mut frontier = Frontier::new(2).unwrap();
    assert!(frontier.is_empty());
    assert_eq!(frontier.pop(), None);
    assert!(frontier.push(&first.capability).unwrap());
    assert!(!frontier.push(&first.capability).unwrap());
    assert!(frontier.push(&second.capability).unwrap());
    assert!(!frontier.push(&second.capability).unwrap());
    assert_eq!(frontier.len(), 2);
    assert_eq!(frontier.pop(), Some(first.capability.id));
    assert_eq!(frontier.pop(), Some(second.capability.id));
    assert!(frontier.is_empty());
}

#[test]
fn frontier_rejects_capacity_without_eviction_and_recovers_capacity_after_pop() {
    let first = member("member-1", "collection-1", &["data"]);
    let second = member("member-2", "collection-1", &["parity"]);
    let mut frontier = Frontier::new(1).unwrap();
    frontier.push(&first.capability).unwrap();
    assert!(matches!(
        frontier.push(&second.capability),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::QueuedCapabilities,
            required: 2,
            limit: 1,
        })
    ));
    assert_eq!(frontier.len(), 1);
    assert_eq!(frontier.pop(), Some(first.capability.id));
    assert!(frontier.push(&second.capability).unwrap());
    assert_eq!(frontier.pop(), Some(second.capability.id));
}

#[test]
fn frontier_validates_a_capability_before_deduplicating_its_id() {
    let mut value = member("member-1", "collection-1", &["data"]).capability;
    let mut frontier = Frontier::new(1).unwrap();
    frontier.push(&value).unwrap();
    value.target_id = "different-member".into();
    assert!(matches!(
        frontier.push(&value),
        Err(FrontierError::InvalidCapability(_))
    ));
    assert_eq!(frontier.len(), 1);
}

#[test]
fn index_and_frontier_publication_is_atomic_on_queue_capacity_failure() {
    let first = member("member-1", "collection-1", &["data"]);
    let second = member("member-2", "collection-1", &["parity"]);
    let mut index = CapabilityIndex::new(2).unwrap();
    let mut frontier = Frontier::new(1).unwrap();
    assert!(index
        .publish_to_frontier(first.clone(), &mut frontier)
        .unwrap());
    assert!(matches!(
        index.publish_to_frontier(second.clone(), &mut frontier),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::QueuedCapabilities,
            required: 2,
            limit: 1,
        })
    ));
    assert_eq!(index.len(), 1);
    assert_eq!(frontier.len(), 1);
    assert!(index.get(&second.capability.id).unwrap().is_none());
    assert!(index
        .matching(&member_prerequisite("collection-1", "parity"))
        .unwrap()
        .is_empty());
    assert_eq!(frontier.pop(), Some(first.capability.id.clone()));
    assert!(index
        .publish_to_frontier(second.clone(), &mut frontier)
        .unwrap());
    assert_eq!(frontier.pop(), Some(second.capability.id));
    assert_eq!(index.len(), 2);
    assert!(!index.publish_to_frontier(first, &mut frontier).unwrap());
    assert!(frontier.is_empty());
}

#[test]
fn index_and_frontier_publication_preserves_queue_on_index_failure() {
    let first = member("member-1", "collection-1", &["data"]);
    let second = member("member-2", "collection-1", &["parity"]);
    let mut index = CapabilityIndex::new(1).unwrap();
    let mut frontier = Frontier::new(2).unwrap();
    index
        .publish_to_frontier(first.clone(), &mut frontier)
        .unwrap();
    assert!(matches!(
        index.publish_to_frontier(second, &mut frontier),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::CapabilityEntries,
            required: 2,
            limit: 1,
        })
    ));
    assert_eq!(frontier.len(), 1);
    assert_eq!(frontier.pop(), Some(first.capability.id));
    assert!(frontier.is_empty());
}

#[test]
fn combined_publication_preserves_queue_on_scope_conflicts_and_role_capacity_failure() {
    let first = member("member-1", "collection-1", &["data"]);
    let second = member("member-2", "collection-1", &["parity"]);
    let mut index = CapabilityIndex::with_limits(2, 1).unwrap();
    let mut frontier = Frontier::new(2).unwrap();
    index
        .publish_to_frontier(first.clone(), &mut frontier)
        .unwrap();
    let mut conflicting = first.clone();
    conflicting.scope_id = "collection-2".into();
    assert!(matches!(
        index.publish_to_frontier(conflicting, &mut frontier),
        Err(FrontierError::CapabilityConflict)
    ));
    assert!(matches!(
        index.publish_to_frontier(second.clone(), &mut frontier),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::CapabilityRoles,
            required: 2,
            limit: 1,
        })
    ));
    assert_eq!(index.len(), 1);
    assert!(index.get(&second.capability.id).unwrap().is_none());
    assert_eq!(frontier.len(), 1);
    assert_eq!(frontier.pop(), Some(first.capability.id));
    assert!(frontier.is_empty());
}

#[test]
fn batch_publication_returns_only_new_ids_in_first_occurrence_order() {
    let existing = member("existing-member", "collection-1", &["existing"]);
    let first = member("first-member", "collection-1", &["data"]);
    let second = member("second-member", "collection-1", &["parity"]);
    let mut index = CapabilityIndex::new(3).unwrap();
    let mut frontier = Frontier::new(3).unwrap();
    index
        .publish_to_frontier(existing.clone(), &mut frontier)
        .unwrap();
    let published = index
        .publish_batch_to_frontier(
            vec![
                existing.clone(),
                second.clone(),
                first.clone(),
                second.clone(),
                first.clone(),
            ],
            &mut frontier,
        )
        .unwrap();
    assert_eq!(
        published,
        vec![second.capability.id.clone(), first.capability.id.clone()]
    );
    assert_eq!(index.len(), 3);
    assert_eq!(frontier.len(), 3);
    assert_eq!(index.get(&first.capability.id).unwrap(), Some(&first));
    assert_eq!(index.get(&second.capability.id).unwrap(), Some(&second));
    assert_eq!(frontier.pop(), Some(existing.capability.id));
    assert_eq!(frontier.pop(), Some(second.capability.id));
    assert_eq!(frontier.pop(), Some(first.capability.id));
    assert!(frontier.is_empty());
}

#[test]
fn batch_empty_and_existing_only_publications_are_idempotent_at_capacity() {
    let existing = member("existing-member", "collection-1", &["data"]);
    let mut index = CapabilityIndex::new(1).unwrap();
    let mut frontier = Frontier::new(1).unwrap();
    index
        .publish_to_frontier(existing.clone(), &mut frontier)
        .unwrap();
    for entries in [
        Vec::new(),
        vec![existing.clone()],
        vec![existing.clone(), existing.clone()],
    ] {
        assert!(index
            .publish_batch_to_frontier(entries, &mut frontier)
            .unwrap()
            .is_empty());
        assert_eq!(index.len(), 1);
        assert_eq!(frontier.len(), 1);
    }
    assert_eq!(frontier.pop(), Some(existing.capability.id.clone()));
    assert!(index
        .publish_batch_to_frontier(vec![existing], &mut frontier)
        .unwrap()
        .is_empty());
    assert!(frontier.is_empty());
}

#[test]
fn batch_invalid_second_capability_does_not_publish_the_first_capability() {
    let existing = member("existing-member", "collection-1", &["existing"]);
    let first = member("first-member", "collection-1", &["data"]);
    let mut invalid = member("invalid-member", "collection-1", &["parity"]);
    invalid.capability.id = "invalid-capability-id".into();
    let mut index = CapabilityIndex::new(3).unwrap();
    let mut frontier = Frontier::new(3).unwrap();
    index
        .publish_to_frontier(existing.clone(), &mut frontier)
        .unwrap();
    assert!(matches!(
        index.publish_batch_to_frontier(vec![first.clone(), invalid.clone()], &mut frontier),
        Err(FrontierError::InvalidCapability(_))
    ));
    assert_eq!(index.len(), 1);
    assert_eq!(index.get(&existing.capability.id).unwrap(), Some(&existing));
    assert!(index.get(&first.capability.id).unwrap().is_none());
    assert!(index.get(&invalid.capability.id).unwrap().is_none());
    assert!(index
        .matching(&member_prerequisite("collection-1", "data"))
        .unwrap()
        .is_empty());
    assert_eq!(frontier.len(), 1);
    assert_eq!(frontier.pop(), Some(existing.capability.id));
    assert!(frontier.is_empty());
    assert!(index
        .publish_to_frontier(first.clone(), &mut frontier)
        .unwrap());
    assert_eq!(frontier.pop(), Some(first.capability.id));
}

#[test]
fn batch_scope_conflict_against_existing_id_does_not_publish_earlier_entries() {
    let existing = member("existing-member", "collection-1", &["existing"]);
    let first = member("first-member", "collection-1", &["data"]);
    let mut conflict = existing.clone();
    conflict.scope_id = "different-collection".into();
    let mut index = CapabilityIndex::new(2).unwrap();
    let mut frontier = Frontier::new(2).unwrap();
    index
        .publish_to_frontier(existing.clone(), &mut frontier)
        .unwrap();
    assert!(matches!(
        index.publish_batch_to_frontier(vec![first.clone(), conflict], &mut frontier),
        Err(FrontierError::CapabilityConflict)
    ));
    assert_eq!(index.len(), 1);
    assert_eq!(index.get(&existing.capability.id).unwrap(), Some(&existing));
    assert!(index.get(&first.capability.id).unwrap().is_none());
    assert_eq!(frontier.len(), 1);
    assert_eq!(frontier.pop(), Some(existing.capability.id));
}

#[test]
fn batch_duplicate_id_conflicts_preserve_the_entire_original_state() {
    let first = member("first-member", "collection-1", &["data"]);
    let mut other_roles = first.clone();
    other_roles.native_roles = ["parity".into()].into_iter().collect();
    let mut other_payload = first.clone();
    other_payload.capability.target_id = "changed-target".into();
    for conflict in [other_roles, other_payload] {
        let mut index = CapabilityIndex::new(2).unwrap();
        let mut frontier = Frontier::new(2).unwrap();
        assert!(matches!(
            index.publish_batch_to_frontier(vec![first.clone(), conflict], &mut frontier),
            Err(FrontierError::CapabilityConflict)
        ));
        assert!(index.is_empty());
        assert!(index
            .matching(&member_prerequisite("collection-1", "data"))
            .unwrap()
            .is_empty());
        assert!(frontier.is_empty());
    }
}

#[test]
fn batch_entry_capacity_is_checked_for_the_combined_batch_before_publication() {
    let first = member("first-member", "collection-1", &["data"]);
    let second = member("second-member", "collection-1", &["parity"]);
    let mut index = CapabilityIndex::with_limits(1, 2).unwrap();
    let mut frontier = Frontier::new(2).unwrap();
    assert!(matches!(
        index.publish_batch_to_frontier(vec![first.clone(), second.clone()], &mut frontier),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::CapabilityEntries,
            required: 2,
            limit: 1,
        })
    ));
    assert!(index.is_empty());
    assert!(index.get(&first.capability.id).unwrap().is_none());
    assert!(index.get(&second.capability.id).unwrap().is_none());
    assert!(frontier.is_empty());
    assert_eq!(
        index
            .publish_batch_to_frontier(vec![first.clone()], &mut frontier)
            .unwrap(),
        vec![first.capability.id]
    );
}

#[test]
fn batch_role_capacity_is_checked_cumulatively_and_rejection_keeps_secondary_indices_empty() {
    let first = member("first-member", "collection-1", &["data"]);
    let second = member("second-member", "collection-1", &["parity"]);
    let mut index = CapabilityIndex::with_limits(2, 1).unwrap();
    let mut frontier = Frontier::new(2).unwrap();
    assert!(matches!(
        index.publish_batch_to_frontier(vec![first.clone(), second], &mut frontier),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::CapabilityRoles,
            required: 2,
            limit: 1,
        })
    ));
    assert!(index.is_empty());
    for role in ["data", "parity"] {
        assert!(index
            .matching(&member_prerequisite("collection-1", role))
            .unwrap()
            .is_empty());
    }
    assert!(frontier.is_empty());
    assert!(index
        .publish_to_frontier(first.clone(), &mut frontier)
        .unwrap());
    assert_eq!(frontier.pop(), Some(first.capability.id));
}

#[test]
fn batch_queue_capacity_rejection_preserves_existing_fifo_and_all_indices() {
    let existing = member("existing-member", "collection-1", &["existing"]);
    let first = member("first-member", "collection-1", &["data"]);
    let second = member("second-member", "collection-1", &["parity"]);
    let mut index = CapabilityIndex::new(3).unwrap();
    let mut frontier = Frontier::new(2).unwrap();
    index
        .publish_to_frontier(existing.clone(), &mut frontier)
        .unwrap();
    assert!(matches!(
        index.publish_batch_to_frontier(vec![first.clone(), second.clone()], &mut frontier),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::QueuedCapabilities,
            required: 3,
            limit: 2,
        })
    ));
    assert_eq!(index.len(), 1);
    assert!(index.get(&first.capability.id).unwrap().is_none());
    assert!(index.get(&second.capability.id).unwrap().is_none());
    assert_eq!(frontier.len(), 1);
    assert_eq!(frontier.pop(), Some(existing.capability.id));
    assert!(frontier.is_empty());
    assert_eq!(
        index
            .publish_batch_to_frontier(vec![first.clone(), second.clone()], &mut frontier)
            .unwrap(),
        vec![first.capability.id.clone(), second.capability.id.clone()]
    );
    assert_eq!(frontier.pop(), Some(first.capability.id));
    assert_eq!(frontier.pop(), Some(second.capability.id));
}

#[test]
fn batch_reuses_an_already_queued_new_id_without_reserving_an_extra_queue_slot() {
    let first = member("first-member", "collection-1", &["data"]);
    let second = member("second-member", "collection-1", &["parity"]);
    let mut index = CapabilityIndex::new(2).unwrap();
    let mut frontier = Frontier::new(2).unwrap();
    frontier.push(&second.capability).unwrap();
    assert_eq!(
        index
            .publish_batch_to_frontier(vec![first.clone(), second.clone()], &mut frontier)
            .unwrap(),
        vec![first.capability.id.clone(), second.capability.id.clone()]
    );
    assert_eq!(index.len(), 2);
    assert_eq!(frontier.len(), 2);
    assert_eq!(frontier.pop(), Some(second.capability.id));
    assert_eq!(frontier.pop(), Some(first.capability.id));
}

#[test]
fn pending_candidates_register_exact_prerequisite_sets_idempotently() {
    let prerequisites = [
        member_prerequisite("collection-1", "data"),
        member_prerequisite("collection-1", "parity"),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    let mut pending = PendingBindings::with_limits(1, 2).unwrap();
    assert!(pending.is_empty());
    assert!(pending.get("unknown").unwrap().is_none());
    assert!(pending
        .register("array-1".into(), prerequisites.clone())
        .unwrap());
    assert!(!pending
        .register("array-1".into(), prerequisites.clone())
        .unwrap());
    assert_eq!(pending.len(), 1);
    assert_eq!(pending.get("array-1").unwrap(), Some(&prerequisites));
    assert!(matches!(
        pending.register(
            "array-1".into(),
            [member_prerequisite("collection-1", "replica")]
                .into_iter()
                .collect()
        ),
        Err(FrontierError::CandidateConflict)
    ));
    assert_eq!(pending.get("array-1").unwrap(), Some(&prerequisites));
}

#[test]
fn unrelated_publications_never_wake_pending_candidates() {
    let mut pending = PendingBindings::new(1).unwrap();
    pending
        .register(
            "array-1".into(),
            [member_prerequisite("collection-1", "data")]
                .into_iter()
                .collect(),
        )
        .unwrap();
    for unrelated in [
        member("member-1", "collection-2", &["data"]),
        member("member-2", "collection-1", &["parity"]),
        entry(
            "container-1",
            1,
            "collection-1",
            CapabilityKind::ContainerMetadata,
            &["data"],
        ),
        member("member-3", "collection-1", &[]),
    ] {
        assert!(pending.on_publish(&unrelated).unwrap().is_empty());
        assert_eq!(pending.len(), 1);
    }
}

#[test]
fn related_publication_returns_each_candidate_once_without_claiming_binding_completion() {
    let data = member_prerequisite("collection-1", "data");
    let parity = member_prerequisite("collection-1", "parity");
    let mut pending = PendingBindings::with_limits(2, 3).unwrap();
    pending
        .register(
            "array-2".into(),
            [data.clone(), parity.clone()].into_iter().collect(),
        )
        .unwrap();
    pending
        .register("array-1".into(), [data.clone()].into_iter().collect())
        .unwrap();
    let expected = ["array-1".into(), "array-2".into()]
        .into_iter()
        .collect::<BTreeSet<String>>();
    let publication = member("member-1", "collection-1", &["data", "parity"]);
    assert_eq!(pending.on_publish(&publication).unwrap(), expected);
    assert_eq!(pending.on_publish(&publication).unwrap(), expected);
    assert_eq!(
        pending.get("array-2").unwrap(),
        Some(&[data, parity].into_iter().collect())
    );
    assert_eq!(pending.len(), 2);
}

#[test]
fn pending_candidate_capacity_failure_does_not_register_reverse_index_entries() {
    let mut pending = PendingBindings::new(1).unwrap();
    let first = [member_prerequisite("collection-1", "data")]
        .into_iter()
        .collect::<BTreeSet<_>>();
    pending.register("array-1".into(), first.clone()).unwrap();
    assert!(matches!(
        pending.register(
            "array-2".into(),
            [member_prerequisite("collection-1", "parity")]
                .into_iter()
                .collect()
        ),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::PendingCandidates,
            required: 2,
            limit: 1,
        })
    ));
    assert_eq!(pending.get("array-1").unwrap(), Some(&first));
    assert!(pending.get("array-2").unwrap().is_none());
    assert!(pending
        .on_publish(&member("member-parity", "collection-1", &["parity"]))
        .unwrap()
        .is_empty());
}

#[test]
fn pending_prerequisite_capacity_failure_preserves_all_existing_candidates() {
    let mut pending = PendingBindings::with_limits(3, 2).unwrap();
    let first = [member_prerequisite("collection-1", "data")]
        .into_iter()
        .collect::<BTreeSet<_>>();
    pending.register("array-1".into(), first.clone()).unwrap();
    assert!(matches!(
        pending.register(
            "array-2".into(),
            [
                member_prerequisite("collection-1", "parity"),
                member_prerequisite("collection-1", "replica")
            ]
            .into_iter()
            .collect()
        ),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::PendingPrerequisites,
            required: 3,
            limit: 2,
        })
    ));
    assert_eq!(pending.len(), 1);
    assert_eq!(pending.get("array-1").unwrap(), Some(&first));
    assert!(pending
        .on_publish(&member("member-parity", "collection-1", &["parity"]))
        .unwrap()
        .is_empty());
}

#[test]
fn prerequisite_replacement_updates_reverse_indices_and_returns_reserved_capacity() {
    let data = member_prerequisite("collection-1", "data");
    let parity = member_prerequisite("collection-1", "parity");
    let mut pending = PendingBindings::with_limits(2, 2).unwrap();
    pending
        .register(
            "array-1".into(),
            [data.clone(), parity.clone()].into_iter().collect(),
        )
        .unwrap();
    assert!(pending
        .replace("array-1", [parity.clone()].into_iter().collect())
        .unwrap());
    assert!(!pending
        .replace("array-1", [parity].into_iter().collect())
        .unwrap());
    pending
        .register("array-2".into(), [data].into_iter().collect())
        .unwrap();
    assert_eq!(
        pending
            .on_publish(&member("member-data", "collection-1", &["data"]))
            .unwrap(),
        ["array-2".into()].into_iter().collect()
    );
    assert_eq!(
        pending
            .on_publish(&member("member-parity", "collection-1", &["parity"]))
            .unwrap(),
        ["array-1".into()].into_iter().collect()
    );
}

#[test]
fn oversized_replacement_keeps_prior_prerequisites_and_reverse_indices() {
    let original = [member_prerequisite("collection-1", "data")]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut pending = PendingBindings::with_limits(1, 1).unwrap();
    pending
        .register("array-1".into(), original.clone())
        .unwrap();
    assert!(matches!(
        pending.replace(
            "array-1",
            [
                member_prerequisite("collection-1", "parity"),
                member_prerequisite("collection-1", "replica")
            ]
            .into_iter()
            .collect()
        ),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::PendingPrerequisites,
            required: 2,
            limit: 1,
        })
    ));
    assert_eq!(pending.get("array-1").unwrap(), Some(&original));
    assert_eq!(
        pending
            .on_publish(&member("member-data", "collection-1", &["data"]))
            .unwrap(),
        ["array-1".into()].into_iter().collect()
    );
    assert!(pending
        .on_publish(&member("member-parity", "collection-1", &["parity"]))
        .unwrap()
        .is_empty());
}

#[test]
fn prerequisite_replacement_reports_the_exact_required_capacity() {
    let first = [member_prerequisite("collection-1", "data")]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let second = [member_prerequisite("collection-1", "parity")]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut pending = PendingBindings::with_limits(2, 2).unwrap();
    pending.register("array-1".into(), first.clone()).unwrap();
    pending.register("array-2".into(), second.clone()).unwrap();
    let replacement = ["data", "replica", "metadata", "journal"]
        .into_iter()
        .map(|role| member_prerequisite("collection-1", role))
        .collect();
    assert!(matches!(
        pending.replace("array-1", replacement),
        Err(FrontierError::CapacityExceeded {
            resource: FrontierResource::PendingPrerequisites,
            required: 5,
            limit: 2,
        })
    ));
    assert_eq!(pending.len(), 2);
    assert_eq!(pending.get("array-1").unwrap(), Some(&first));
    assert_eq!(pending.get("array-2").unwrap(), Some(&second));
    assert_eq!(
        pending
            .on_publish(&member("member-data", "collection-1", &["data"]))
            .unwrap(),
        ["array-1".into()].into_iter().collect()
    );
}

#[test]
fn invalid_replacement_preserves_existing_candidate_and_reverse_index() {
    let prerequisites = [member_prerequisite("collection-1", "data")]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut pending = PendingBindings::new(1).unwrap();
    pending
        .register("array-1".into(), prerequisites.clone())
        .unwrap();
    for replacement in [
        BTreeSet::new(),
        [member_prerequisite("collection-1", " \t")]
            .into_iter()
            .collect(),
    ] {
        assert!(pending.replace("array-1", replacement).is_err());
        assert_eq!(pending.len(), 1);
        assert_eq!(pending.get("array-1").unwrap(), Some(&prerequisites));
        assert_eq!(
            pending
                .on_publish(&member("member-data", "collection-1", &["data"]))
                .unwrap(),
            ["array-1".into()].into_iter().collect()
        );
    }
}

#[test]
fn removing_candidates_cleans_shared_reverse_indices_and_releases_all_capacity() {
    let prerequisite = member_prerequisite("collection-1", "data");
    let prerequisites = [prerequisite].into_iter().collect::<BTreeSet<_>>();
    let mut pending = PendingBindings::new(2).unwrap();
    pending
        .register("array-1".into(), prerequisites.clone())
        .unwrap();
    pending
        .register("array-2".into(), prerequisites.clone())
        .unwrap();
    assert_eq!(
        pending.remove("array-1").unwrap(),
        Some(prerequisites.clone())
    );
    assert_eq!(pending.remove("array-1").unwrap(), None);
    assert_eq!(
        pending
            .on_publish(&member("member-data", "collection-1", &["data"]))
            .unwrap(),
        ["array-2".into()].into_iter().collect()
    );
    pending.remove("array-2").unwrap();
    assert!(pending.is_empty());
    assert!(pending
        .on_publish(&member("member-data", "collection-1", &["data"]))
        .unwrap()
        .is_empty());
    pending
        .register(
            "array-3".into(),
            [
                member_prerequisite("collection-1", "parity"),
                member_prerequisite("collection-1", "replica"),
            ]
            .into_iter()
            .collect(),
        )
        .unwrap();
    assert_eq!(pending.len(), 1);
}

#[test]
fn pending_bindings_fail_fast_on_empty_ids_prerequisites_and_invalid_publications() {
    let valid = [member_prerequisite("collection-1", "data")]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut pending = PendingBindings::new(3).unwrap();
    assert!(pending.register("".into(), valid.clone()).is_err());
    assert!(matches!(
        pending.register("array-1".into(), BTreeSet::new()),
        Err(FrontierError::EmptyPrerequisites)
    ));
    assert!(pending
        .register(
            "array-1".into(),
            [member_prerequisite(" \t", "data")].into_iter().collect()
        )
        .is_err());
    assert!(pending.is_empty());
    assert!(matches!(
        pending.replace("unknown", valid),
        Err(FrontierError::MissingCandidate)
    ));
    assert!(pending.get("").is_err());
    assert!(pending.remove(" \t").is_err());
    let mut invalid = member("member-1", "collection-1", &["data"]);
    invalid.capability.id = "invalid-immutable-id".into();
    assert!(matches!(
        pending.on_publish(&invalid),
        Err(FrontierError::InvalidCapability(_))
    ));
    assert!(pending.is_empty());
}
