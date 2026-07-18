use super::*;

#[test]
fn normalization_removes_partition_and_lvm_roots_without_losing_linux_root() {
    let cases = [
        ("Partition 2 (XFS) - cl/root/etc/passwd", "/etc/passwd"),
        (
            "[P2]/cl/root/var/log/auth.log.1.gz",
            "/var/log/auth.log.1.gz",
        ),
        (
            "cl/root/home/alice/.bash_history",
            "/home/alice/.bash_history",
        ),
        ("cl/root/root/.bash_history", "/root/.bash_history"),
    ];

    for (input, expected) in cases {
        assert_eq!(normalize_evidence_path(input), expected);
    }
}

#[test]
fn normalization_preserves_windows_paths_after_partition_marker_removal() {
    assert_eq!(
        normalize_evidence_path(r"[P0]\Windows\System32\config\SYSTEM"),
        "/windows/system32/config/system"
    );
}

#[test]
fn linux_category_selection_excludes_windows_definitions_before_scan() {
    let definitions = selected_evidence_category_defs(&["LinuxArtifacts"]);
    let categories = definitions
        .iter()
        .map(|definition| definition.category)
        .collect::<Vec<_>>();

    assert_eq!(categories, vec!["LinuxArtifacts"]);
    assert!(!categories.contains(&"Registry"));
    assert!(!categories.contains(&"EventLogs"));
    assert!(!categories.contains(&"BrowserHistory"));
}
