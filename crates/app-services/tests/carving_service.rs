#[path = "support/evidence_case.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "support/import_ntfs_fixture.rs"]
#[allow(dead_code)]
mod ntfs_fixture;
use app_services::carving_service::*;
use domain::{DataSourceId, DataSourceKind};
use sha2::{Digest, Sha256};
use std::{
    io::Write,
    sync::{atomic::AtomicBool, Mutex},
};
use transport::dto::{
    CarvedExportRequestDto, CarvedRangeRequestDto, CarvedSearchRequestDto, PngCarvingRequestDto,
};
static SERIAL: Mutex<()> = Mutex::new(());

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(data);
    bytes.extend_from_slice(&crc32fast::hash(&bytes[4..]).to_be_bytes());
    bytes
}
fn png() -> Vec<u8> {
    let mut header = [0u8; 13];
    header[..4].copy_from_slice(&1u32.to_be_bytes());
    header[4..8].copy_from_slice(&1u32.to_be_bytes());
    header[8] = 8;
    header[9] = 6;
    let mut compressor =
        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    compressor.write_all(&[0, 255, 0, 0, 255]).unwrap();
    [
        evidence_core::carving::PNG_SIGNATURE.to_vec(),
        chunk(b"IHDR", &header),
        chunk(b"IDAT", &compressor.finish().unwrap()),
        chunk(b"IEND", &[]),
    ]
    .concat()
}
fn request(source: &str) -> PngCarvingRequestDto {
    PngCarvingRequestDto {
        data_source_id: source.into(),
        offset: None,
        length: None,
    }
}

#[test]
fn writer_failure_rolls_back_artifacts_and_stops_the_bounded_producer() {
    let _serial = SERIAL.lock().unwrap();
    let case = fixture::EvidenceCase::new();
    let image = png();
    let path = case.temp.path().join("source.raw");
    std::fs::write(&path, image.repeat(260)).unwrap();
    case.active.with_conn(|conn| {
        let source=case.source(conn,"source",&path,DataSourceKind::Raw);
        source.execute_batch("CREATE TRIGGER reject_carving_node BEFORE INSERT ON graph_nodes BEGIN SELECT RAISE(FAIL,'injected graph failure'); END;").unwrap();drop(source);
        let started=std::time::Instant::now();
        assert!(run_png_carving(conn,&case.active.case_root,&case.active.meta.id,&request("source"),&AtomicBool::new(false),&|_,_|{}).is_err());
        assert!(started.elapsed()<std::time::Duration::from_secs(5));
        let source=app_services::source_db::open_registered_source_db_read_only(conn,&case.active.case_root,&DataSourceId("source".into())).unwrap();
        assert_eq!(source.query_row("SELECT COUNT(*) FROM artifacts",[],|r|r.get::<_,u32>(0)).unwrap(),0);
        assert_eq!(source.query_row("SELECT COUNT(*) FROM graph_nodes",[],|r|r.get::<_,u32>(0)).unwrap(),0);Ok(())
    }).unwrap();
}

#[test]
fn carving_search_cursor_expires_when_a_new_range_adds_artifacts() {
    let _serial = SERIAL.lock().unwrap();
    let case = fixture::EvidenceCase::new();
    let image = png();
    let path = case.temp.path().join("source.raw");
    std::fs::write(&path, image.repeat(102)).unwrap();
    case.active
        .with_conn(|conn| {
            drop(case.source(conn, "source", &path, DataSourceKind::Raw));
            let mut first = request("source");
            first.length = Some((image.len() * 101).to_string());
            assert_eq!(
                run_png_carving(
                    conn,
                    &case.active.case_root,
                    &case.active.meta.id,
                    &first,
                    &AtomicBool::new(false),
                    &|_, _| {}
                )
                .unwrap()
                .complete,
                101
            );
            let mut search = CarvedSearchRequestDto {
                data_source_id: "source".into(),
                query: String::new(),
                cursor: None,
            };
            let page = search_carved_artifacts(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &search,
            )
            .unwrap();
            assert_eq!(page.items.len(), 100);
            assert!(page.next_cursor.is_some());
            let mut foreign: serde_json::Value =
                transport::paging::decode_opaque_cursor(page.next_cursor.as_ref().unwrap())
                    .unwrap();
            foreign["case"] = serde_json::json!("foreign-case");
            search.cursor = Some(transport::paging::encode_opaque_cursor(&foreign).unwrap());
            assert!(search_carved_artifacts(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &search
            )
            .is_err());
            search.cursor = None;
            let mut last = request("source");
            last.offset = Some((image.len() * 101).to_string());
            last.length = Some(image.len().to_string());
            run_png_carving(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &last,
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            search.cursor = page.next_cursor;
            assert!(search_carved_artifacts(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &search
            )
            .is_err());
            search.cursor = None;
            search.query = "%".into();
            assert!(search_carved_artifacts(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &search
            )
            .unwrap()
            .items
            .is_empty());
            search.query.clear();
            search.cursor = search_carved_artifacts(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &search,
            )
            .unwrap()
            .next_cursor;
            std::fs::write(&path, [image.repeat(102), vec![0]].concat()).unwrap();
            assert!(search_carved_artifacts(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &search
            )
            .is_err());
            Ok(())
        })
        .unwrap();
}

#[test]
fn png_task_persists_idempotent_artifact_graph_and_search_without_file_or_timeline_rows() {
    let _serial = SERIAL.lock().unwrap();
    let case = fixture::EvidenceCase::new();
    let image = png();
    let bytes = [vec![0; 13], image.clone(), vec![0; 31]].concat();
    let path = case.temp.path().join("source.raw");
    std::fs::write(&path, &bytes).unwrap();
    case.active
        .with_conn(|conn| {
            let source = case.source(conn, "source", &path, DataSourceKind::Raw);
            drop(source);
            let result = run_png_carving(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &request("source"),
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            assert_eq!((result.complete, result.rejected), (1, 0));
            let artifact = &result.artifacts[0];
            assert_eq!(artifact.offset, "13");
            assert_eq!(artifact.length, image.len().to_string());
            assert_eq!(artifact.sha256, hex::encode(Sha256::digest(&image)));
            let replay = run_png_carving(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &request("source"),
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            assert_eq!(replay.artifacts[0].artifact_id, artifact.artifact_id);
            let source = app_services::source_db::open_registered_source_db_read_only(
                conn,
                &case.active.case_root,
                &DataSourceId("source".into()),
            )
            .unwrap();
            for (table, count) in [
                ("artifacts", 1),
                ("graph_nodes", 1),
                ("file_entries", 0),
                ("timeline_events", 0),
            ] {
                assert_eq!(
                    source
                        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                            .get::<_, u32>(0))
                        .unwrap(),
                    count
                );
            }
            let found = search_carved_artifacts(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &CarvedSearchRequestDto {
                    data_source_id: "source".into(),
                    query: artifact.sha256[..12].into(),
                    cursor: None,
                },
            )
            .unwrap();
            assert_eq!(found.items.len(), 1);
            let preview = read_carved_artifact_range(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &CarvedRangeRequestDto {
                    artifact_id: artifact.artifact_id.clone(),
                    offset: "0".into(),
                    length: 65536,
                },
            )
            .unwrap();
            use base64::Engine;
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(preview.base64)
                    .unwrap(),
                image
            );
            let destination = case.temp.path().join("candidate.png");
            let export = CarvedExportRequestDto {
                artifact_id: artifact.artifact_id.clone(),
                destination_path: destination.to_string_lossy().into_owned(),
            };
            export_carved_artifact(conn, &case.active.case_root, &case.active.meta.id, &export)
                .unwrap();
            assert_eq!(std::fs::read(&destination).unwrap(), image);
            assert!(export_carved_artifact(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &export
            )
            .is_err());
            let overwrite_evidence = CarvedExportRequestDto {
                artifact_id: artifact.artifact_id.clone(),
                destination_path: path.to_string_lossy().into_owned(),
            };
            assert!(export_carved_artifact(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &overwrite_evidence
            )
            .is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
            Ok(())
        })
        .unwrap();
}

#[test]
fn raw_and_e01_logical_views_yield_identical_png_ranges_and_digests() {
    let _serial = SERIAL.lock().unwrap();
    let case = fixture::EvidenceCase::new();
    let image = png();
    let mut bytes = vec![0; 32768];
    bytes[4093..4093 + image.len()].copy_from_slice(&image);
    let raw = case.temp.path().join("source.raw");
    std::fs::write(&raw, &bytes).unwrap();
    let e01 = case.temp.path().join("source.E01");
    ntfs_fixture::write_e01(&e01, &bytes).unwrap();
    case.active
        .with_conn(|conn| {
            drop(case.source(conn, "raw", &raw, DataSourceKind::Raw));
            drop(case.source(conn, "e01", &e01, DataSourceKind::E01));
            let a = run_png_carving(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &request("raw"),
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            let b = run_png_carving(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &request("e01"),
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            assert_eq!((a.complete, b.complete), (1, 1));
            assert_eq!(a.artifacts[0].offset, b.artifacts[0].offset);
            assert_eq!(a.artifacts[0].length, b.artifacts[0].length);
            assert_eq!(a.artifacts[0].sha256, b.artifacts[0].sha256);
            assert_ne!(a.artifacts[0].artifact_id, b.artifacts[0].artifact_id);
            Ok(())
        })
        .unwrap();
}

#[test]
fn cancellation_invalid_address_and_source_replacement_do_not_publish_false_success() {
    let _serial = SERIAL.lock().unwrap();
    let case = fixture::EvidenceCase::new();
    let path = case.temp.path().join("source.raw");
    std::fs::write(&path, png()).unwrap();
    case.active
        .with_conn(|conn| {
            drop(case.source(conn, "source", &path, DataSourceKind::Raw));
            assert!(run_png_carving(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &request("source"),
                &AtomicBool::new(true),
                &|_, _| {}
            )
            .is_err());
            let mut invalid = request("source");
            invalid.offset = Some("9007199254740993".into());
            assert!(run_png_carving(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &invalid,
                &AtomicBool::new(false),
                &|_, _| {}
            )
            .is_err());
            let result = run_png_carving(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &request("source"),
                &AtomicBool::new(false),
                &|_, _| {},
            )
            .unwrap();
            std::fs::write(&path, vec![0; 512]).unwrap();
            assert!(read_carved_artifact_range(
                conn,
                &case.active.case_root,
                &case.active.meta.id,
                &CarvedRangeRequestDto {
                    artifact_id: result.artifacts[0].artifact_id.clone(),
                    offset: "0".into(),
                    length: 16
                }
            )
            .is_err());
            Ok(())
        })
        .unwrap();
}
