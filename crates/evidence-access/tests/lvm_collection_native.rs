use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use domain::DataSourceKind;
use evidence_access::datasource_service::{
    self, CollectionPlanError, ImageCollectionPlan, LvmDiscoverySource, PartitionStatus,
};
use sha2::{Digest, Sha256};

fn sources(root: &Path) -> Vec<LvmDiscoverySource> {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/lvm-native/shared-ext4");
    (0..2)
        .map(|index| {
            let path = root.join(format!("pv-{index}.raw"));
            std::io::copy(
                &mut flate2::read::GzDecoder::new(
                    File::open(fixture.join(format!("pv-{index}.raw.gz"))).unwrap(),
                ),
                &mut File::create(&path).unwrap(),
            )
            .unwrap();
            LvmDiscoverySource::new(path, DataSourceKind::Raw)
        })
        .collect()
}

#[test]
fn native_cross_pv_view_has_one_stable_owner_and_matches_the_kernel_digest() {
    let workspace = tempfile::tempdir().unwrap();
    let mut sources = sources(workspace.path());
    let cancel = AtomicBool::new(false);
    let plan = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    let owner = plan
        .member_probe(0, &sources[0].source_path, &sources[0].source_kind)
        .unwrap();
    let secondary = plan
        .member_probe(1, &sources[1].source_path, &sources[1].source_kind)
        .unwrap();
    assert_eq!(owner.candidates.len(), 1);
    assert!(secondary.candidates.is_empty());
    assert!(plan.is_shared_member(1));
    assert_eq!(secondary.partitions.len(), 1);
    assert_eq!(secondary.partitions[0].status, PartitionStatus::Expanded);
    let identity = owner.candidates[0].lvm_identity.as_ref().unwrap();
    assert_eq!(identity.pv_sources.len(), 2);
    assert_ne!(
        identity.pv_sources[0].source_path,
        identity.pv_sources[1].source_path
    );
    assert_eq!(identity.pv_sources[0].offset, identity.pv_sources[1].offset);
    let pool = fs_lvm::LvmPool::discover(
        identity
            .pv_sources
            .iter()
            .map(|source| {
                datasource_service::open_evidence_reader(
                    Path::new(&source.source_path),
                    source.source_kind.as_ref().unwrap(),
                )
                .unwrap()
            })
            .collect(),
        identity
            .pv_sources
            .iter()
            .map(|source| source.offset)
            .collect(),
    )
    .unwrap();
    let index = pool
        .list_volumes()
        .iter()
        .position(|volume| volume.uuid == identity.lv_uuid)
        .unwrap();
    let mut reader = pool.open_volume_reader(index).unwrap();
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    assert_eq!(
        hex::encode(digest.finalize()),
        "e8c4c41f356b5e202206ad532b467954e3fe3b5826de1f19f236c66f60849586"
    );
    sources.reverse();
    let reversed = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    assert!(reversed
        .member_probe(0, &sources[0].source_path, &sources[0].source_kind)
        .unwrap()
        .candidates
        .is_empty());
    assert_eq!(
        reversed
            .member_probe(1, &sources[1].source_path, &sources[1].source_kind)
            .unwrap()
            .candidates[0]
            .lvm_identity,
        owner.candidates[0].lvm_identity
    );
}

#[test]
fn native_collection_rejects_duplicate_pv_and_invalidates_changed_members() {
    let workspace = tempfile::tempdir().unwrap();
    let mut sources = sources(workspace.path());
    let cancel = AtomicBool::new(false);
    let plan = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    assert!(matches!(
        plan.member_probe(0, &sources[1].source_path, &DataSourceKind::Raw),
        Err(CollectionPlanError::MemberMismatch)
    ));
    let duplicate = workspace.path().join("copy.raw");
    std::fs::copy(&sources[0].source_path, &duplicate).unwrap();
    sources.push(LvmDiscoverySource::new(duplicate, DataSourceKind::Raw));
    assert!(matches!(
        ImageCollectionPlan::prepare(&sources, &cancel),
        Err(CollectionPlanError::DuplicateIdentity)
    ));
    File::options()
        .write(true)
        .open(&sources[1].source_path)
        .unwrap()
        .set_len(1)
        .unwrap();
    assert!(plan.validate_snapshot().is_err());
    assert!(plan
        .member_probe(0, &sources[0].source_path, &DataSourceKind::Raw)
        .is_err());
    assert!(matches!(
        ImageCollectionPlan::prepare(&sources, &AtomicBool::new(true)),
        Err(CollectionPlanError::Cancelled)
    ));
}

#[test]
fn collection_admission_propagates_thin_transaction_failure() {
    let workspace = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../testdata/fixtures/public-small/lvm-native/thin/pv-transaction-conflict.raw.gz",
    );
    let path = workspace.path().join("thin-conflict.raw");
    std::io::copy(
        &mut flate2::read::GzDecoder::new(File::open(fixture).unwrap()),
        &mut File::create(&path).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        ImageCollectionPlan::prepare(
            &[LvmDiscoverySource::new(path, DataSourceKind::Raw)],
            &AtomicBool::new(false)
        ),
        Err(CollectionPlanError::Lvm(
            fs_lvm::LvmError::ThinPoolTransactionMismatch {
                expected: 1,
                actual: 2
            }
        )),
    ));
}
