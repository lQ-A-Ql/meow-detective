use ceph_wire::{
    format_cephfs_journal_data_object_name, format_cephfs_journal_pointer_object_name,
    plan_cephfs_journal_range, CephFsJournalLayout, CephFsJournalObjectExtent, CephWireError,
};

const KIB: u32 = 1024;

#[test]
fn generic_striping_matches_ceph_file_to_extents_order() {
    let layout = CephFsJournalLayout {
        stripe_unit: 64 * KIB,
        stripe_count: 2,
        object_size: 128 * KIB,
        pool_id: 7,
    };
    let extents = plan_cephfs_journal_range(layout, 0, 320 * KIB as usize).unwrap();
    assert_eq!(
        extents,
        vec![
            extent(0, 0, 0),
            extent(64 * KIB as u64, 1, 0),
            extent(128 * KIB as u64, 0, 64 * KIB as u64),
            extent(192 * KIB as u64, 1, 64 * KIB as u64),
            extent(256 * KIB as u64, 2, 0),
        ]
    );
}

#[test]
fn single_stripe_uses_object_size_as_effective_stripe_unit() {
    let layout = CephFsJournalLayout {
        stripe_unit: 64 * KIB,
        stripe_count: 1,
        object_size: 256 * KIB,
        pool_id: 7,
    };
    let extents = plan_cephfs_journal_range(layout, 64 * KIB as u64, 256 * KIB as usize).unwrap();
    assert_eq!(extents.len(), 2);
    assert_eq!(extents[0].object_index, 0);
    assert_eq!(extents[0].object_offset, 64 * KIB as u64);
    assert_eq!(extents[0].length, 192 * KIB as usize);
    assert_eq!(extents[1].object_index, 1);
    assert_eq!(extents[1].object_offset, 0);
}

#[test]
fn journal_names_are_rank_bound_and_ranges_are_checked() {
    assert_eq!(
        format_cephfs_journal_pointer_object_name(0).as_deref(),
        Some("400.00000000")
    );
    assert_eq!(
        format_cephfs_journal_data_object_name(0, 0x200, 3).as_deref(),
        Some("200.00000003")
    );
    assert!(format_cephfs_journal_data_object_name(0, 0x201, 0).is_none());
    assert!(format_cephfs_journal_pointer_object_name(0x100).is_none());

    let layout = CephFsJournalLayout {
        stripe_unit: 64 * KIB,
        stripe_count: 1,
        object_size: 64 * KIB,
        pool_id: 7,
    };
    assert!(matches!(
        plan_cephfs_journal_range(layout, u64::MAX, 2),
        Err(CephWireError::CephFsJournalRangeOverflow)
    ));
}

fn extent(logical_offset: u64, object_index: u32, object_offset: u64) -> CephFsJournalObjectExtent {
    CephFsJournalObjectExtent {
        logical_offset,
        object_index,
        object_offset,
        length: 64 * KIB as usize,
    }
}
