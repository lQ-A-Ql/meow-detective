use super::*;

fn source(id: &str) -> DataSource {
    DataSource {
        id: DataSourceId(id.into()),
        name: "RBD catalog".into(),
        kind: DataSourceKind::CephRbd,
        source_path: "ceph-rbd://cluster/image".into(),
        imported_at: chrono::Utc::now(),
        provenance: Default::default(),
    }
}

#[test]
fn catalog_stream_summary_exceeds_one_hundred_thousand_rows() {
    let fixture = NativeSourceFixture::new("catalog-stream-large", "case-1");
    let source = source(&fixture.source_id);
    let repo = persistence_source::repositories::file_repo::FileRepo::new(&fixture.connection);
    repo.insert_batch(&[catalog_dir(&source, "root", None, "", "root")])
        .unwrap();
    let files = 100_005u64;
    for first in (0..files).step_by(500) {
        let batch = (first..(first + 500).min(files))
            .map(|index| {
                let id = format!("file-{index:06}");
                catalog_file(&source, &id, "root", &format!("files/{index:06}"), &id)
            })
            .collect::<Vec<_>>();
        repo.insert_batch(&batch).unwrap();
    }
    fixture.seal();
    let summary =
        catalog_manifest::summarize_source_connection(&fixture.connection, source.clone()).unwrap();
    assert_eq!(summary.file_count, files + 1);
    assert_eq!(summary.directory_count, 1);
    assert_eq!(summary.total_size, files * 42);
    let repeated =
        catalog_manifest::summarize_source_connection(&fixture.connection, source).unwrap();
    assert_eq!(summary.catalog_digest, repeated.catalog_digest);
}

#[test]
fn catalog_stream_digest_is_stable_and_tracks_parent_metadata() {
    let fixture = NativeSourceFixture::new("catalog-stream-small", "case-1");
    let source = source(&fixture.source_id);
    let rows = vec![
        catalog_dir(&source, "root", None, "", "root"),
        catalog_dir(&source, "other", None, "alternate", "alternate"),
        catalog_file(&source, "z-file", "root", "same/path", "first"),
        catalog_file(&source, "a-file", "root", "same/path", "second"),
    ];
    let repo = persistence_source::repositories::file_repo::FileRepo::new(&fixture.connection);
    repo.insert_batch(&rows).unwrap();
    fixture.seal();
    let first =
        catalog_manifest::summarize_source_connection(&fixture.connection, source.clone()).unwrap();
    let reordered = NativeSourceFixture::new(&fixture.source_id, "case-1");
    let reordered_repo =
        persistence_source::repositories::file_repo::FileRepo::new(&reordered.connection);
    for entry in rows.into_iter().rev() {
        reordered_repo.insert_batch(&[entry]).unwrap();
    }
    reordered.seal();
    let repeated =
        catalog_manifest::summarize_source_connection(&reordered.connection, source.clone())
            .unwrap();
    assert_eq!(first.catalog_digest, repeated.catalog_digest);
    persistence_source::detail::update_catalog_parent(
        &fixture.connection,
        "a-file",
        Some("other".into()),
        &source.id.0,
    )
    .unwrap();
    let moved =
        catalog_manifest::summarize_source_connection(&fixture.connection, source.clone()).unwrap();
    assert_ne!(first.catalog_digest, moved.catalog_digest);
    repo.update_root_metadata(&catalog_dir(&source, "other", None, "alternate", "renamed"))
        .unwrap();
    let renamed =
        catalog_manifest::summarize_source_connection(&fixture.connection, source.clone()).unwrap();
    assert_ne!(moved.catalog_digest, renamed.catalog_digest);
    persistence_source::detail::update_catalog_parent(
        &fixture.connection,
        "a-file",
        Some("missing".into()),
        &source.id.0,
    )
    .unwrap();
    let orphaned =
        catalog_manifest::summarize_source_connection(&fixture.connection, source).unwrap();
    assert_ne!(renamed.catalog_digest, orphaned.catalog_digest);
    assert_eq!(orphaned.file_count, 4);
}
