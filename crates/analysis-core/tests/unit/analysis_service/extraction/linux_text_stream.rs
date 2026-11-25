use super::*;
use std::io::Write;

crate::native_source_test_support!();

fn candidate(size: u64, path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: domain::FileEntryId("syslog".into()),
        data_source_id: "linux-stream".into(),
        partition_index: None,
        path: path.into(),
        size,
        encrypted: false,
        content_identity: "stream-input".into(),
        companions: Vec::new(),
        modified_at: Some(chrono::Utc::now()),
        evidence_kind: "log".into(),
        parser: "linux.text_log".into(),
        category: "LinuxArtifacts".into(),
    }
}

#[test]
fn file_backed_plain_and_gzip_logs_publish_every_line_after_ten_thousand() {
    for gzip in [false, true] {
        let source = test_support::NativeSourceFixture::new("linux-stream", "case");
        let path = source
            .source_dir
            .join(if gzip { "syslog.gz" } else { "syslog" });
        let file = std::fs::File::create(&path).unwrap();
        let mut writer: Box<dyn Write> = if gzip {
            Box::new(flate2::write::GzEncoder::new(
                file,
                flate2::Compression::default(),
            ))
        } else {
            Box::new(file)
        };
        for index in 1..=12001 {
            writeln!(writer, "2026-11-25T00:00:00Z host service: event {index}").unwrap();
        }
        let octet_line = b"2026-11-25T00:00:00Z host service: octet \xff\n";
        writer.write_all(octet_line).unwrap();
        writer.flush().unwrap();
        drop(writer);
        let input = std::fs::File::open(&path).unwrap();
        let candidate = candidate(
            input.metadata().unwrap().len(),
            if gzip {
                "var/log/syslog.gz"
            } else {
                "var/log/syslog"
            },
        );
        let cancel = AtomicBool::new(false);
        let generation = crate::analysis_service::extraction::native_output::begin_streaming(
            &source.connection,
            "linux-stream",
            &[],
            &cancel,
            |_| {},
            |_| {},
        )
        .unwrap();
        let result = persist_reader(
            &generation,
            &candidate,
            Box::new(input),
            &LinuxLogTimeContext::utc(),
            &cancel,
        )
        .unwrap();
        assert_eq!(result.artifact_count, 12002);
        assert!(result.warnings.is_empty());
        assert_eq!(
            persistence_source::repositories::artifact_repo::ArtifactRepo::new(&source.connection)
                .count()
                .unwrap(),
            0
        );
        generation.rebuild_derived_metrics(&cancel, |_| {}).unwrap();
        let mut staged = generation.seal(&cancel, |_, _| {}).unwrap();
        staged.bind(&source.connection).unwrap();
        staged.commit();
        assert_eq!(
            persistence_source::repositories::artifact_repo::ArtifactRepo::new(&source.connection)
                .count()
                .unwrap(),
            12002
        );
        let store = persistence_source::detail::require_source_store(&source.connection).unwrap();
        let records =
            persistence_source::fjall::ArtifactRecordStore::from_source_store((*store).clone());
        let snapshot = records.read_snapshot();
        let mut cursor = None;
        let mut tail = false;
        let mut octets = false;
        loop {
            let page = records
                .list_by_source_object(
                    &snapshot,
                    "linux-stream",
                    "syslog",
                    cursor.as_deref(),
                    persistence_source::fjall::ScanDirection::Forward,
                    512,
                )
                .unwrap();
            for record in page.records {
                if record.attrs["attrs"]["lineNumber"] == 12001 {
                    assert!(record.attrs["attrs"]["byteOffset"].as_u64().unwrap() > 0);
                    tail = true;
                }
                if record.attrs["attrs"]["lineNumber"] == 12002 {
                    use base64::Engine;
                    assert_eq!(
                        record.attrs["attrs"]["rawLineBase64"],
                        base64::engine::general_purpose::STANDARD.encode(octet_line)
                    );
                    assert_eq!(
                        record.attrs["attrs"]["byteOffsetScope"],
                        if gzip {
                            "gzipDecodedContent"
                        } else {
                            "sourceFileContent"
                        }
                    );
                    octets = true;
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert!(tail);
        assert!(octets);
    }
}

#[test]
fn truncated_input_cannot_publish_streamed_outputs() {
    let source = test_support::NativeSourceFixture::new("linux-stream", "case");
    let path = source.source_dir.join("syslog");
    let mut file = std::fs::File::create(&path).unwrap();
    for index in 0..600 {
        writeln!(file, "2026-11-25T00:00:00Z host service: event {index}").unwrap();
    }
    drop(file);
    let input = std::fs::File::open(&path).unwrap();
    let candidate = candidate(input.metadata().unwrap().len() + 1, "var/log/syslog");
    let cancel = AtomicBool::new(false);
    let generation = crate::analysis_service::extraction::native_output::begin_streaming(
        &source.connection,
        "linux-stream",
        &[],
        &cancel,
        |_| {},
        |_| {},
    )
    .unwrap();
    assert!(persist_reader(
        &generation,
        &candidate,
        Box::new(input),
        &LinuxLogTimeContext::utc(),
        &cancel
    )
    .is_err());
    drop(generation);
    assert_eq!(
        persistence_source::repositories::artifact_repo::ArtifactRepo::new(&source.connection)
            .count()
            .unwrap(),
        0
    );
}
