use super::*;
use std::fs::File;
use std::path::PathBuf;

#[test]
fn native_cross_pv_binding_preserves_distinct_sources_at_identical_offsets() {
    let workspace = tempfile::tempdir().unwrap();
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/lvm-native");
    let mut evidence = Vec::new();
    for index in 0..2 {
        let path = workspace.path().join(format!("member-{index}.raw"));
        let compressed =
            File::open(fixture_root.join(format!("label_sectors.pv{index}.img.zst"))).unwrap();
        std::io::copy(
            &mut ruzstd::decoding::StreamingDecoder::new(compressed).unwrap(),
            &mut File::create(&path).unwrap(),
        )
        .unwrap();
        let mut reader =
            datasource_service::open_evidence_reader(&path, &DataSourceKind::Raw).unwrap();
        evidence.push(inspect_pv(&mut reader, &path, &DataSourceKind::Raw, 0).unwrap());
    }
    let pool = open_complete_pool(&evidence).unwrap().unwrap();
    let sources = pool_sources(&evidence, &pool).unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].offset, sources[1].offset);
    assert_ne!(sources[0].source_path, sources[1].source_path);
    assert_ne!(sources[0].pv_uuid, sources[1].pv_uuid);
    for (source, pv) in sources.iter().zip(&pool.volume_group().physical_volumes) {
        assert!(same_uuid(&source.pv_uuid, &pv.uuid));
        assert_eq!(source.pv_name.as_deref(), Some(pv.name.as_str()));
    }
    let first = evidence[0].volume_group.as_ref().unwrap();
    let mut conflicting = first.clone();
    conflicting.logical_volumes[0].segments[0].extent_count += 1;
    assert!(matches!(
        fs_lvm::metadata::select_committed_metadata([first, &conflicting]),
        Err(fs_lvm::LvmError::ConflictingMetadata { .. }),
    ));
    let mut older = first.clone();
    older.seqno -= 1;
    assert_eq!(
        fs_lvm::metadata::select_committed_metadata([&older, first]).unwrap(),
        Some(first),
    );
    evidence.push(PvEvidence {
        source: evidence[0].source.clone(),
        label: evidence[0].label.clone(),
        volume_group: evidence[0].volume_group.clone(),
    });
    assert!(open_complete_pool(&evidence).is_err());
}
