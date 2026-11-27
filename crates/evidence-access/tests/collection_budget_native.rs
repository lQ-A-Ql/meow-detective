use std::{
    fs::File,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

use domain::DataSourceKind;
use evidence_access::datasource_service::{ImageCollectionPlan, LvmDiscoverySource};
use source_kernel::resource_budget::{global_pool, Resources};

#[test]
fn native_shared_collection_plan_releases_its_single_reservation_after_the_last_owner() {
    let root = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/lvm-native/shared-ext4");
    let mut sources = Vec::new();
    for index in 0..2 {
        let path = root.path().join(format!("pv-{index}.raw"));
        std::io::copy(
            &mut flate2::read::GzDecoder::new(
                File::open(fixture.join(format!("pv-{index}.raw.gz"))).unwrap(),
            ),
            &mut File::create(&path).unwrap(),
        )
        .unwrap();
        sources.push(LvmDiscoverySource::new(path, DataSourceKind::Raw));
    }
    assert_eq!(global_pool().snapshot().0, Resources::default());
    let plan = Arc::new(ImageCollectionPlan::prepare(&sources, &AtomicBool::new(false)).unwrap());
    let retained = global_pool().snapshot().0;
    assert!(retained.bytes >= 65536 && retained.bytes <= 16 * 1024 * 1024);
    assert_eq!(retained.bytes, retained.metadata);
    assert_eq!(retained.readers, 0);
    assert_eq!(retained.handles, 0);
    let borrowed = Arc::clone(&plan);
    assert_eq!(global_pool().snapshot().0, retained);
    drop(plan);
    assert_eq!(global_pool().snapshot().0, retained);
    borrowed
        .member_probe(0, &sources[0].source_path, &DataSourceKind::Raw)
        .unwrap();
    let factory = borrowed
        .verified_factory(
            &sources[0].source_path,
            &DataSourceKind::Raw,
            &AtomicBool::new(false),
        )
        .unwrap();
    let mut reader = factory.open_reader().unwrap();
    drop(borrowed);
    drop(factory);
    assert!(global_pool().snapshot().0.bytes > 0);
    let mut byte = [0];
    std::io::Read::read_exact(&mut reader, &mut byte).unwrap();
    drop(reader);
    assert_eq!(global_pool().snapshot().0, Resources::default());
}
