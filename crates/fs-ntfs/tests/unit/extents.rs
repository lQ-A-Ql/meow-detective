use super::*;
use crate::DataRun;

fn extent(lowest_vcn: u64, highest_vcn: u64, clusters: u64) -> DataAttributeExtent {
    DataAttributeExtent::NonResident {
        lowest_vcn,
        highest_vcn,
        allocated_size: clusters * 4096,
        real_size: clusters * 4096,
        attr_flags: 0,
        compression_unit_exp: 0,
        runs: vec![DataRun {
            lcn: Some(10),
            cluster_count: clusters,
        }],
    }
}

#[test]
fn contiguous_extent_chain_is_accepted() {
    validate_extent_chain(&[extent(0, 1, 2), extent(2, 3, 2)]).unwrap();
}

#[test]
fn extent_vcn_gap_is_rejected() {
    let error = validate_extent_chain(&[extent(0, 0, 1), extent(2, 2, 1)]).unwrap_err();
    assert!(error.to_string().contains("gap or overlap"));
}

#[test]
fn duplicate_extent_vcn_is_rejected() {
    let error = validate_extent_chain(&[extent(0, 0, 1), extent(0, 0, 1)]).unwrap_err();
    assert!(error.to_string().contains("gap or overlap"));
}

#[test]
fn mixed_resident_and_nonresident_extents_are_rejected() {
    let error = validate_extent_chain(&[
        DataAttributeExtent::Resident { data: Vec::new() },
        extent(0, 0, 1),
    ])
    .unwrap_err();
    assert!(error.to_string().contains("resident NTFS attribute"));
}
