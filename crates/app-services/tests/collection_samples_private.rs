use app_services::cluster_service;
use evidence_access::{
    datasource_service::{ImageCollectionPlan, LvmDiscoverySource},
    mdraid,
};
use std::{
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::Instant,
};

#[test]
#[ignore = "requires private E01 collection roots"]
fn private_collection_native_identity_inventory() {
    for variable in [
        "FORENSICS_K8S_E01_ROOT",
        "FORENSICS_PVE_CLUSTER_ROOT",
        "FORENSICS_RAID_E01_ROOT",
    ] {
        let root = PathBuf::from(std::env::var_os(variable).expect("set private source root"));
        let plan = cluster_service::plan_linux_evidence_set_import(root, None).unwrap();
        for (index, source) in plan.members.iter().enumerate() {
            let mut reader = evidence_access::datasource_service::open_evidence_reader(
                &source.source_path,
                &source.source_kind,
            )
            .unwrap();
            let probe =
                evidence_access::datasource_service::detect_image_filesystem(&mut reader).unwrap();
            eprintln!(
                "identity inventory: sample={variable} member={index} logical_bytes={}",
                reader.info().size
            );
            for candidate in probe.candidates.iter().filter(|candidate| {
                candidate.kind == evidence_access::datasource_service::ImageFilesystemKind::LvmPool
            }) {
                let label = fs_lvm::label::parse_pv_label(&mut *reader, candidate.offset).unwrap();
                if label.metadata_areas.is_empty() {
                    eprintln!("native PV: uuid={} metadata_copy=absent", label.pv_uuid);
                    continue;
                }
                let vg = fs_lvm::metadata::parse_metadata_from_regions(
                    &mut *reader,
                    &label.metadata_areas,
                    candidate.offset,
                )
                .unwrap();
                eprintln!(
                    "native PV: uuid={} VG={} vg_uuid={} seqno={} required_pvs={} volumes={:?}",
                    label.pv_uuid,
                    vg.name,
                    vg.id,
                    vg.seqno,
                    vg.physical_volumes.len(),
                    vg.logical_volumes
                        .iter()
                        .map(|volume| &volume.name)
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

fn exercise(variable: &str, members: usize) {
    let root = PathBuf::from(std::env::var_os(variable).expect("set the private collection root"));
    let input =
        cluster_service::plan_linux_evidence_set_import(root, Some("content-bound-plan".into()))
            .unwrap();
    assert_eq!(input.members.len(), members);
    let sources = input
        .members
        .iter()
        .map(|member| LvmDiscoverySource::new(&member.source_path, member.source_kind.clone()))
        .collect::<Vec<_>>();
    for source in &sources {
        let factory =
            evidence_access::source_runtime::open_factory(&source.source_path, &source.source_kind)
                .unwrap();
        eprintln!(
            "private collection input: kind={} logical_bytes={}",
            source.source_kind,
            factory.logical_size()
        );
    }
    let started = Instant::now();
    let cancel = AtomicBool::new(false);
    let plan = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    eprintln!(
        "private shared plan prepared: members={} elapsed_ms={}",
        members,
        started.elapsed().as_millis()
    );
    let arrays = mdraid::discover_mdraid_from_collection(&plan, &cancel).unwrap();
    assert_eq!(
        arrays,
        mdraid::discover_mdraid_from_collection(&plan, &cancel).unwrap()
    );
    for array in &arrays {
        eprintln!(
            "private MD: level={} observed={} expected={} uuid={}",
            array.level,
            array.members.len(),
            array.raid_disks,
            hex::encode(array.array_uuid)
        );
        let mut reader = mdraid::open_mdraid_reader(array).unwrap();
        let mut header = [0; 4096];
        reader.read_exact(&mut header).unwrap();
    }
    for source in &sources {
        let content = plan
            .source_content(&source.source_path, &source.source_kind)
            .unwrap();
        assert_eq!(content.logical_sha256.len(), 64);
        let mut verified = plan
            .verified_factory(&source.source_path, &source.source_kind, &cancel)
            .unwrap()
            .open_reader()
            .unwrap();
        let mut independent = evidence_access::datasource_service::open_evidence_reader(
            &source.source_path,
            &source.source_kind,
        )
        .unwrap();
        for offset in [
            0,
            content.logical_size / 2,
            content.logical_size.saturating_sub(4096),
        ] {
            let count = (content.logical_size - offset).min(4096) as usize;
            let mut actual = vec![0; count];
            let mut expected = vec![0; count];
            verified.seek(SeekFrom::Start(offset)).unwrap();
            independent.seek(SeekFrom::Start(offset)).unwrap();
            verified.read_exact(&mut actual).unwrap();
            independent.read_exact(&mut expected).unwrap();
            assert_eq!(actual, expected);
        }
    }
    plan.verify_content(&cancel).unwrap();
    eprintln!(
        "private shared plan verified: members={} arrays={} elapsed_ms={}",
        members,
        arrays.len(),
        started.elapsed().as_millis()
    );
}

#[test]
#[ignore = "requires FORENSICS_RAID_E01_ROOT private RAID collection"]
fn raid_shared_plan_binds_content_and_reuses_nested_md_members() {
    exercise("FORENSICS_RAID_E01_ROOT", 4);
}

#[test]
#[ignore = "requires FORENSICS_K8S_E01_ROOT private Kubernetes collection"]
fn kubernetes_shared_plan_binds_all_four_images() {
    exercise("FORENSICS_K8S_E01_ROOT", 4);
}

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT private PVE and Ceph collection"]
fn pve_ceph_shared_plan_binds_all_six_images() {
    exercise("FORENSICS_PVE_CLUSTER_ROOT", 6);
}
