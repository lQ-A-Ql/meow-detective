#[path = "support/evidence_case.rs"]
mod fixture;
use app_services::{content_index_service, search_service};
use std::sync::atomic::{AtomicBool, Ordering};
use transport::commands::SearchFilesRequest;
fn setup() -> fixture::EvidenceCase {
    let case = fixture::EvidenceCase::new();
    let evidence = case.temp.path().join("content-source");
    std::fs::create_dir(&evidence).unwrap();
    let utf16 = [0xff, 0xfe]
        .into_iter()
        .chain(
            "phrase beyond preview"
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        )
        .collect::<Vec<_>>();
    let text = format!("{} distinctive token", "prefix ".repeat(200));
    let files = [
        ("a", "named.txt", text.into_bytes()),
        ("b", "binary.txt", vec![0; 100]),
        ("c", "large.txt", vec![b'x'; 1024 * 1024 + 1]),
        ("d", "encoding.bin", vec![0xff, 0x80, 0x81]),
        ("e", "utf16.bin", utf16),
    ];
    case.active.with_conn(|conn| {
        let source=case.source(conn,"source",&evidence,domain::DataSourceKind::LogicalDirectory);
        for (id,path,bytes) in files { std::fs::write(evidence.join(path),&bytes).unwrap(); fixture::entry(&source,"source",id,path,bytes.len() as u64); }
        fixture::entry(&source,"source","f","missing.txt",20);
        source.execute("INSERT INTO file_entries(id,data_source_id,path,name,entry_type,size) VALUES('g','source','empty','empty','directory',0)",[]).unwrap();
        source.execute("INSERT INTO artifacts(id,case_id,data_source_id,artifact_type,source_object_id,extractor_id,title,summary,attrs) VALUES('artifact',?1,'source','test','a','test','artifactonly','structured annotation','{}')",[&case.active.meta.id.0]).unwrap();
        let index=search::SearchIndex::create(&app_services::source_db::registered_source_index_dir(conn,&case.active.case_root,&domain::DataSourceId("source".into())).unwrap()).unwrap();
        let mut writer=index.metadata_writer().unwrap();
        writer.add_documents(&[search::SearchFileDocument {file_id:"a".into(),path:"named.txt".into(),name:"named.txt".into(),extension:"txt".into(),entry_type:"file".into(),size:Some(1),modified_at:None,deleted:false,hidden:false,system:false,encrypted:false}]).unwrap(); writer.commit().unwrap(); Ok(())
    }).unwrap();
    case
}
fn search(
    conn: &rusqlite::Connection,
    case: &fixture::EvidenceCase,
    query: &str,
) -> Vec<transport::dto::SearchFileHitDto> {
    search_service::search_files_for_case(
        conn,
        &case.active.case_root,
        &case.active.meta.id,
        &SearchFilesRequest {
            query: query.into(),
            match_path: true,
            entry_type: Default::default(),
            extensions: vec![],
            data_source_ids: vec![],
            sort_key: Default::default(),
            sort_direction: Default::default(),
            offset: 0,
            limit: 100,
            cursor: None,
        },
    )
    .unwrap()
    .items
}
#[test]
fn complete_generation_reports_every_skip_reason_and_truthful_field_matches() {
    let case = setup();
    case.active
        .with_conn(|conn| {
            let summary = content_index_service::build(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                "source",
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            assert_eq!(
                (
                    summary.indexed,
                    summary.binary,
                    summary.oversized,
                    summary.unreadable,
                    summary.unsupported_encoding,
                    summary.not_applicable,
                    summary.expected
                ),
                (2, 1, 1, 1, 1, 1, 7)
            );
            assert!(summary.complete);
            let content = search(conn, &case, "\"distinctive token\"");
            assert_eq!(content.len(), 1);
            assert_eq!(content[0].matched_sources, ["content"]);
            assert!(content[0]
                .snippet
                .as_deref()
                .unwrap()
                .contains("distinctive token"));
            let name = search(conn, &case, "named");
            assert_eq!(name.len(), 1);
            assert_eq!(name[0].match_source, "filename");
            assert!(!name[0]
                .matched_sources
                .iter()
                .any(|source| source == "content"));
            let artifact = search(conn, &case, "artifactonly");
            assert_eq!(artifact.len(), 1);
            assert_eq!(artifact[0].matched_sources, ["artifactField"]);
            assert_eq!(search(conn, &case, "\"phrase beyond\"").len(), 1);
            let coverage = content_index_service::coverage::get(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                "source",
            )
            .unwrap();
            assert!(coverage.summary.complete);
            assert!(!coverage.stale);
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM content_index_entries", [], |row| row
                    .get::<_, u64>(
                    0
                ))
                .unwrap(),
                7
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn cancellation_preserves_the_published_index_and_retries_from_checkpoint() {
    let case = setup();
    case.active
        .with_conn(|conn| {
            let cancel = AtomicBool::new(false);
            let result = content_index_service::build(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                "source",
                &cancel,
                &|_, _| cancel.store(true, Ordering::Release),
            );
            assert_eq!(result.unwrap_err().category, "cancelled");
            assert_eq!(search(conn, &case, "named").len(), 1);
            assert!(search(conn, &case, "distinctive").is_empty());
            let before = conn
                .query_row("SELECT COUNT(*) FROM content_index_entries", [], |row| {
                    row.get::<_, u64>(0)
                })
                .unwrap();
            assert_eq!(before, 7);
            let resumed = content_index_service::build(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                "source",
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            assert!(resumed.complete);
            assert_eq!(resumed.expected, 7);
            assert_eq!(search(conn, &case, "distinctive").len(), 1);
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM content_index_entries", [], |row| row
                    .get::<_, u64>(
                    0
                ))
                .unwrap(),
                before
            );
            Ok(())
        })
        .unwrap();
}
#[test]
fn changed_input_marks_published_coverage_stale_and_rebuilds_without_old_text() {
    let case = setup();
    case.active
        .with_conn(|conn| {
            content_index_service::build(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                "source",
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            let path = case.temp.path().join("content-source/named.txt");
            let changed = std::fs::read_to_string(&path)
                .unwrap()
                .replace("distinctive", "replacement");
            std::fs::write(path, changed).unwrap();
            assert!(
                content_index_service::coverage::get(
                    conn,
                    &case.active.case_root,
                    &case.active.meta.id,
                    "source"
                )
                .unwrap()
                .stale
            );
            content_index_service::build(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                "source",
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            assert!(search(conn, &case, "distinctive").is_empty());
            assert_eq!(search(conn, &case, "replacement").len(), 1);
            Ok(())
        })
        .unwrap();
}
