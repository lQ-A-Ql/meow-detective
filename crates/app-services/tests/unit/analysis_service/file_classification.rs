use super::*;

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
