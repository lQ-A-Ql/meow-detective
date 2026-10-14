use crate::parse_git_text;

#[test]
fn parses_head_and_refs() {
    let head = parse_git_text("/repo/.git/HEAD", "ref: refs/heads/main\n");
    assert_eq!(head[0].kind, "GitHead");
    let refs = parse_git_text(
        "/repo/.git/packed-refs",
        "# pack\n0123456789012345678901234567890123456789 refs/heads/main\n",
    );
    assert_eq!(
        refs[0].fields["refs/heads/main"],
        "0123456789012345678901234567890123456789"
    );
}

#[test]
fn preserves_reflog_message_without_silent_truncation() {
    let records = parse_git_text(
        "/repo/.git/logs/HEAD",
        &format!(
            "{}\t{}\n",
            "a".repeat(40) + " " + &"b".repeat(40) + " user 1 +0000",
            "x".repeat(5000)
        ),
    );
    assert_eq!(records[0].fields["message"], "x".repeat(5000));
    assert_eq!(records[0].fields["actor"], "user");
    assert_eq!(records[0].fields["timestamp"], "1");
}

#[test]
fn accepts_windows_paths_and_sha256_object_ids() {
    let object_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let records = parse_git_text(r"C:\Repo\.GIT\refs\heads\main", &format!("{object_id}\n"));
    assert_eq!(records[0].kind, "GitRef");
    assert_eq!(records[0].fields["refs/heads/main"], object_id);
}

#[test]
fn config_keeps_remote_sections_and_duplicate_values_separate() {
    let records = crate::parse_git_text_checked("/repo/.git/config", "[remote \"origin\"]\nurl = https://one.invalid/r.git\nurl = https://two.invalid/r.git\n[remote \"backup\"]\nurl = \"https://three.invalid/r.git\" # comment\n").unwrap();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0].fields["remoteName"], "origin");
    assert_eq!(records[1].fields["remoteUrl"], "https://two.invalid/r.git");
    assert_eq!(records[2].fields["remoteName"], "backup");
    assert_eq!(
        records[2].fields["remoteUrl"],
        "https://three.invalid/r.git"
    );
    assert_eq!(records[2].fields["lineNumber"], "5");
}

#[test]
fn commit_keeps_all_parents_identity_time_and_multiline_message() {
    let text = format!("tree {}\nparent {}\nparent {}\nauthor Example Author <author@example.invalid> 1700000000 +0800\ncommitter Example Committer <commit@example.invalid> 1700000001 -0100\n\nSubject\n\nFull body\nSecond line\n", "a".repeat(40), "b".repeat(40), "c".repeat(40));
    let record = crate::parse_git_text_checked("/repo/.git/objects/aa/bb", &text)
        .unwrap()
        .remove(0);
    assert_eq!(
        record.fields["author"],
        "Example Author <author@example.invalid>"
    );
    assert_eq!(record.fields["committerTime"], "1700000001");
    assert_eq!(
        record.fields["parents"],
        format!("{} {}", "b".repeat(40), "c".repeat(40))
    );
    assert_eq!(
        record.fields["message"],
        "Subject\n\nFull body\nSecond line"
    );
}

#[test]
fn malformed_refs_non_utf8_and_oversized_gzip_are_typed_errors() {
    assert!(crate::parse_git_text_checked("/repo/.git/refs/heads/main", "bad-hash").is_err());
    assert!(crate::parse_git_bytes_checked("/repo/.git/HEAD", &[0xff]).is_err());
    use std::io::Write;
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&vec![b'a'; 1024 * 1024 + 1]).unwrap();
    assert!(crate::parse_git_bytes_checked(
        "/var/log/gitlab/production_json.log.1.gz",
        &gzip.finish().unwrap()
    )
    .is_err());
}

#[test]
fn gitlab_config_and_json_logs_have_source_line_offsets() {
    let records = crate::parse_git_text_checked(
        "/etc/gitlab/gitlab.rb",
        "# config\nexternal_url 'https://git.example.invalid'\nnginx['listen_port'] = 8443\n",
    )
    .unwrap();
    assert_eq!(records[0].kind, "GitLabConfig");
    assert_eq!(
        records[0].fields["external_url"],
        "https://git.example.invalid"
    );
    assert_eq!(records[0].fields["lineNumber"], "2");
    assert_eq!(records[0].fields["byteOffset"], "9");
    assert_eq!(records[1].fields["listenPort"], "8443");
    let records = crate::parse_git_text_checked("/var/log/gitlab/gitlab-rails/production_json.log", "{\"time\":\"2026-10-14T00:00:00Z\",\"username\":\"sample-user\",\"remote_ip\":\"192.0.2.1\",\"action\":\"show\",\"status\":200,\"project_id\":42}\n").unwrap();
    assert_eq!(records[0].kind, "GitLabLog");
    assert_eq!(records[0].fields["user"], "sample-user");
    assert_eq!(records[0].fields["ip"], "192.0.2.1");
    assert_eq!(records[0].fields["status"], "200");
    assert_eq!(records[0].fields["project"], "42");
}

#[test]
fn loose_commit_declared_length_is_verified() {
    use std::io::Write;
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    zlib.write_all(b"commit 999\0tree a\n\nmessage").unwrap();
    assert!(
        crate::parse_git_bytes_checked("/repo/.git/objects/aa/bb", &zlib.finish().unwrap())
            .is_err()
    );
}
