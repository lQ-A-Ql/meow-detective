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
fn bounds_reflog_message() {
    let records = parse_git_text(
        "/repo/.git/logs/HEAD",
        &format!(
            "{}\t{}\n",
            "a".repeat(40) + " " + &"b".repeat(40) + " user 1 +0000",
            "x".repeat(5000)
        ),
    );
    assert!(records[0].fields["entry_0_message"].len() <= 512);
}

#[test]
fn accepts_windows_paths_and_sha256_object_ids() {
    let object_id = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let records = parse_git_text(
        r"C:\Repo\.GIT\refs\heads\main",
        &format!("{object_id} refs/heads/main\n"),
    );
    assert_eq!(records[0].kind, "GitRef");
    assert_eq!(records[0].fields["refs/heads/main"], object_id);
}
