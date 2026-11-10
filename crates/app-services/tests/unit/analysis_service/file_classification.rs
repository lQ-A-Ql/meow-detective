use super::*;
crate::native_source_test_support!();

fn native_source(entries: &[FileEntry]) -> test_support::NativeSourceFixture {
    let source_id = entries
        .first()
        .map(|entry| entry.data_source_id.clone())
        .expect("at least one file");
    let fixture = test_support::NativeSourceFixture::new(&source_id.0, "case-classification");
    fixture.insert_files(entries);
    fixture.seal();
    fixture
}

fn native_source_with_unknown_encryption(entry: FileEntry) -> test_support::NativeSourceFixture {
    let source_id = entry.data_source_id.clone();
    let fixture = test_support::NativeSourceFixture::new(&source_id.0, "case-classification");
    let record = persistence_fjall::FileEntryRecord {
        id: entry.id.0,
        parent_id: entry.parent_id.map(|id| id.0),
        data_source_id: entry.data_source_id.0,
        path: entry.path,
        name: entry.name,
        entry_type: persistence_fjall::CatalogEntryType::File,
        size: entry.size,
        ext: entry.ext,
        deleted: entry.deleted,
        hidden: entry.hidden,
        system: entry.system,
        read_only: entry.read_only,
        archive: entry.archive,
        encrypted: None,
        created: entry.created_at,
        modified: entry.modified_at,
        accessed: entry.accessed_at,
        changed: entry.changed_at,
        hash_sha256: entry.hash_sha256,
        partition_index: None,
        unix_mode: entry.unix_mode,
    };
    let store = persistence_sqlite::source_fjall::open_store_for_write(&fixture.connection)
        .unwrap()
        .unwrap();
    persistence_fjall::CatalogRecordStore::from_source_store((*store).clone())
        .upsert(&record, persistence_fjall::PersistMode::SyncAll)
        .unwrap();
    fixture.seal();
    fixture
}

fn entry(name: &str, path: &str, ext: &str, size: u64) -> FileEntry {
    FileEntry {
        id: FileEntryId(format!("file:{name}")),
        parent_id: None,
        data_source_id: domain::DataSourceId("ds-1".to_string()),
        path: path.to_string(),
        name: name.to_string(),
        entry_type: domain::EntryType::File,
        size: Some(size),
        ext: if ext.is_empty() {
            None
        } else {
            Some(ext.to_string())
        },
        deleted: false,
        hidden: false,
        system: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
        encrypted: false,
    }
}

#[test]
fn magic_zip_disambiguates_office_by_extension() {
    let docx = entry("report.docx", "[P3]/Users/a/report.docx", "docx", 100);
    let c = classify_entry(&docx, Some(b"PK\x03\x04rest"));
    assert_eq!(
        (c.file_type, c.family, c.subcategory, c.via_magic),
        (Some("ZIP"), "documents", "Word 文档", true)
    );

    let xlsx = entry("book.xlsx", "[P3]/Users/a/book.xlsx", "xlsx", 100);
    let c = classify_entry(&xlsx, Some(b"PK\x03\x04rest"));
    assert_eq!((c.family, c.subcategory), ("documents", "Excel 文档"));

    let zip = entry("pack.zip", "[P3]/Users/a/pack.zip", "zip", 100);
    let c = classify_entry(&zip, Some(b"PK\x03\x04rest"));
    assert_eq!((c.family, c.subcategory), ("archives", "ZIP 压缩包"));
}

#[test]
fn magic_covers_forensic_key_formats() {
    let cases: &[(&[u8], &str, &str, &str, &str)] = &[
        (b"MZrest", "exe", "executables", "Windows 可执行", "PE"),
        (b"%PDF-1.7", "pdf", "documents", "PDF 文档", "PDF"),
        (b"\xff\xd8\xff\xe0rest", "jpg", "images", "普通图片", "JPEG"),
        (b"\x00\x00\x01\x00rest", "ico", "images", "图标", "ICO"),
        (b"CMMMrest", "db", "images", "缩略图缓存", "THUMBCACHE"),
        (
            b"SQLite format 3\0",
            "sqlite",
            "databases",
            "SQLite 数据库",
            "SQLite",
        ),
        (b"regfrest", "dat", "system", "注册表配置单元", "REG"),
        (
            b"EVF\x09\x0d\x0a\xff\x00rest",
            "e01",
            "forensics",
            "E01 镜像",
            "E01",
        ),
        (
            b"LVF\x09\x0d\x0a\xff\x00rest",
            "ex01",
            "forensics",
            "Ex01 镜像",
            "Ex01",
        ),
        (b"KDMVrest", "vmdk", "forensics", "VMDK 磁盘", "VMDK"),
        (b"!BDNrest", "pst", "documents", "邮件文档", "PST"),
        (b"L\x00\x00\x00rest", "lnk", "documents", "快捷方式", "LNK"),
        (b"SCCArest", "pf", "system", "预取文件", "PF"),
        (
            b"Rar!\x1a\x07\x01\x00rest",
            "rar",
            "archives",
            "RAR 压缩包",
            "RAR",
        ),
    ];
    for (header, ext, family, subcategory, file_type) in cases {
        let e = entry(
            &format!("sample.{ext}"),
            &format!("[P3]/x/sample.{ext}"),
            ext,
            10,
        );
        let c = classify_entry(&e, Some(header));
        assert_eq!(
            (c.file_type, c.family, c.subcategory, c.via_magic),
            (Some(*file_type), *family, *subcategory, true),
            "header {header:?}"
        );
    }
}

#[test]
fn riff_magic_uses_format_tag() {
    let wav = entry("a.wav", "[P3]/x/a.wav", "wav", 10);
    let c = classify_entry(&wav, Some(b"RIFF\x00\x00\x00\x00WAVErest"));
    assert_eq!((c.family, c.subcategory), ("media", "音频"));

    let avi = entry("a.avi", "[P3]/x/a.avi", "avi", 10);
    let c = classify_entry(&avi, Some(b"RIFF\x00\x00\x00\x00AVI rest"));
    assert_eq!((c.family, c.subcategory), ("media", "视频"));
}

#[test]
fn metadata_fallback_marks_non_magic_rows() {
    let log = entry("setup.log", "[P3]/x/setup.log", "log", 10);
    let c = classify_entry(&log, Some(b"plain text no magic"));
    assert_eq!(
        (c.family, c.subcategory, c.via_magic),
        ("documents", "日志文档", false)
    );

    let thumb = entry("thumbcache_96.db", "[P3]/x/thumbcache_96.db", "db", 10);
    let c = classify_entry(&thumb, Some(b"random"));
    assert_eq!((c.family, c.subcategory), ("images", "缩略图缓存"));

    let unknown = entry("blob.bin", "[P3]/x/blob.bin", "bin", 10);
    let c = classify_entry(&unknown, Some(b"random"));
    assert_eq!(
        (c.family, c.subcategory, c.via_magic),
        ("other", "未识别", false)
    );

    let evtx = entry(
        "System.evtx",
        "[P3]/Windows/System32/winevt/Logs/System.evtx",
        "evtx",
        10,
    );
    let c = classify_entry(&evtx, Some(b"ElfFile\0rest"));
    assert_eq!(
        (c.family, c.subcategory, c.via_magic),
        ("documents", "日志文档", true)
    );
}

#[test]
fn classification_board_surfaces_header_read_failures() {
    let mut evidence = entry("evidence.exe", "[P2]/evidence.exe", "exe", 100);
    evidence.id = FileEntryId("file-1".into());
    evidence.data_source_id = domain::DataSourceId("ds-1".into());
    let fixture = native_source(&[evidence]);

    let board = build_file_classification_board(&fixture.connection, 1, |_file_id| {
        Err::<Vec<u8>, _>("evidence read unavailable")
    })
    .expect("classification board");

    assert_eq!(board.magic_classified_count, 0);
    assert_eq!(board.metadata_classified_count, 1);
    assert!(board.warnings.iter().any(|warning| {
        warning.contains("[P2]/evidence.exe") && warning.contains("evidence read unavailable")
    }));
}

#[test]
fn encrypted_files_are_classified_without_reading_content_or_leaking_paths() {
    let mut encrypted = entry("secret.txt", "[P2]/private/secret.txt", "txt", 100);
    encrypted.id = FileEntryId("efs-file".into());
    encrypted.data_source_id = domain::DataSourceId("ds-rbd".into());
    encrypted.encrypted = true;
    let fixture = native_source(&[encrypted]);
    let mut read_calls = 0usize;

    let board = build_file_classification_board(&fixture.connection, 1, |_file_id| {
        read_calls += 1;
        Ok::<Vec<u8>, &str>(b"ciphertext".to_vec())
    })
    .expect("classify encrypted metadata");

    assert_eq!(read_calls, 0, "encrypted content must not reach the reader");
    assert_eq!(board.magic_classified_count, 0);
    assert_eq!(board.metadata_classified_count, 1);
    let warning = board
        .warnings
        .iter()
        .find(|warning| warning.contains("EFS-encrypted"))
        .expect("explicit encrypted-content warning");
    assert!(warning.contains("metadata only"));
    assert!(warning.contains("was not read"));
    assert!(!warning.contains("secret.txt"));
    assert!(!warning.contains("[P2]"));
}

#[test]
fn unknown_encryption_status_is_not_reported_as_efs() {
    let mut unknown = entry("unknown.txt", "[P2]/private/unknown.txt", "txt", 100);
    unknown.id = FileEntryId("unknown-file".into());
    unknown.data_source_id = domain::DataSourceId("ds-unknown".into());
    let fixture = native_source_with_unknown_encryption(unknown);
    let mut read_calls = 0usize;

    let board = build_file_classification_board(&fixture.connection, 1, |_file_id| {
        read_calls += 1;
        Ok::<Vec<u8>, &str>(b"plaintext".to_vec())
    })
    .expect("classify unknown-encryption metadata");

    assert_eq!(read_calls, 0, "unknown content must not reach the reader");
    assert!(board
        .warnings
        .iter()
        .any(|warning| warning.contains("unknown encryption status")));
    assert!(board
        .warnings
        .iter()
        .all(|warning| !warning.contains("EFS-encrypted")));
}
