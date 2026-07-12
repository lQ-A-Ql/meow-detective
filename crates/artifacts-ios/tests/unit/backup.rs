use super::*;
use rusqlite::Connection;
use std::io::Read;

fn make_manifest_db(files: &[(&str, &str, &str, i32)]) -> Vec<u8> {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE Files (
                    fileID TEXT,
                    domain TEXT,
                    relativePath TEXT,
                    flags INTEGER,
                    file BLOB
                );",
        )
        .expect("create table");
        for (id, domain, path, flags) in files {
            conn.execute(
                "INSERT INTO Files VALUES (?1, ?2, ?3, ?4, NULL)",
                rusqlite::params![id, domain, path, flags],
            )
            .expect("insert");
        }
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    buf
}

#[test]
fn parse_manifest_basic() {
    let db = make_manifest_db(&[
        (
            "abc123",
            "HomeDomain",
            "Library/Preferences/com.apple.plist",
            0,
        ),
        ("def456", "AppDomain-com.example", "Documents/notes.txt", 1),
    ]);
    let files = parse_manifest(&db).expect("parse manifest");
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].file_id, "abc123");
    assert_eq!(files[0].domain, "HomeDomain");
    assert_eq!(
        files[0].relative_path,
        "Library/Preferences/com.apple.plist"
    );
    assert_eq!(files[0].flags, Some(0));
    assert_eq!(files[1].file_id, "def456");
    assert_eq!(files[1].domain, "AppDomain-com.example");
    assert_eq!(files[1].flags, Some(1));
}

#[test]
fn parse_manifest_empty_db() {
    let db = make_manifest_db(&[]);
    let files = parse_manifest(&db).expect("parse manifest");
    assert!(files.is_empty());
}

#[test]
fn parse_manifest_not_a_db() {
    let result = parse_manifest(b"this is not a sqlite database");
    assert!(result.is_err());
}

#[test]
fn parse_manifest_many_files() {
    let entries: Vec<_> = (0..50)
        .map(|i| {
            (
                format!("hash{:04x}", i),
                "HomeDomain".to_string(),
                format!("path/to/file_{}.txt", i),
                if i % 4 == 0 { 4 } else { 0 },
            )
        })
        .collect();
    let refs: Vec<_> = entries
        .iter()
        .map(|(a, b, c, d)| (a.as_str(), b.as_str(), c.as_str(), *d))
        .collect();
    let db = make_manifest_db(&refs);
    let files = parse_manifest(&db).expect("parse manifest");
    assert_eq!(files.len(), 50);
    assert_eq!(files[4].flags, Some(4)); // i=4 → directory flag
}
