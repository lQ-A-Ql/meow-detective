use app_services::cluster_service::{
    mdraid_descriptor_path, write_mdraid_descriptor, MdRaidArrayPlan, MdRaidMemberPlan,
};
use app_services::datasource_service::{LvmLogicalVolumeIdentity, LvmPhysicalVolumeSource};
use domain::DataSourceKind;
use fs_lvm::mdraid::MdRaidSuperblock;

fn plan() -> MdRaidArrayPlan {
    let superblock = MdRaidSuperblock {
        metadata_version: fs_lvm::mdraid::MdRaidMetadataVersion::V1_2,
        major_version: 1,
        feature_map: 0,
        array_uuid: [7; 16],
        name: "array".to_string(),
        level: 0,
        layout: 1,
        size_sectors: 16,
        chunk_sectors: 8,
        raid_disks: 1,
        data_offset_sectors: 2048,
        data_size_sectors: 16,
        super_offset_sectors: 8,
        dev_number: 0,
        device_uuid: [8; 16],
        events: 10,
        recovery_offset_sectors: u64::MAX,
        resync_offset_sectors: u64::MAX,
        reshape_position_sectors: u64::MAX,
        dev_roles: vec![0],
        checksum_valid: true,
    };
    let identity = LvmLogicalVolumeIdentity {
        vg_uuid: "vg-1".to_string(),
        vg_name: "vg".to_string(),
        lv_uuid: "lv-1".to_string(),
        lv_name: "lv".to_string(),
        pv_offsets: vec![0],
        pv_sources: vec![LvmPhysicalVolumeSource {
            source_path: "member.raw".to_string(),
            source_kind: Some(DataSourceKind::Raw),
            offset: 0,
            pv_uuid: "pv-1".to_string(),
            pv_name: None,
        }],
    };
    MdRaidArrayPlan {
        array_uuid: [7; 16],
        name: "array".to_string(),
        level: 0,
        layout: 1,
        chunk_sectors: 8,
        raid_disks: 1,
        min_data_size_sectors: 16,
        capacity_mismatch: false,
        members: vec![MdRaidMemberPlan {
            superblock,
            origin: app_services::cluster_service::MdRaidMemberOrigin::Lvm {
                vg_name: "vg".to_string(),
                vg_uuid: "vg-1".to_string(),
                lv_name: "lv".to_string(),
                lv_uuid: "lv-1".to_string(),
                identity,
            },
        }],
    }
}

#[test]
fn persists_mdraid_descriptor_under_case_derived_storage() {
    let case_root = tempfile::tempdir().expect("case root");
    let plan = plan();
    let path = write_mdraid_descriptor(case_root.path(), &plan).expect("write descriptor");
    assert_eq!(path, mdraid_descriptor_path(case_root.path(), &[7; 16]));
    let stored: MdRaidArrayPlan =
        serde_json::from_slice(&std::fs::read(&path).expect("descriptor payload"))
            .expect("descriptor JSON");
    assert_eq!(stored, plan);
}
