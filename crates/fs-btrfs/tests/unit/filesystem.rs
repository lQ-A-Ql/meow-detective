use super::{join_path, normalize_path};

#[test]
fn normalizes_mixed_separators() {
    assert_eq!(normalize_path(r"\@rootfs\\etc\"), "@rootfs/etc");
    assert_eq!(normalize_path("/"), "");
}

#[test]
fn joins_paths_without_duplicate_separators() {
    assert_eq!(join_path("", "etc"), "etc");
    assert_eq!(join_path("@rootfs", "etc"), "@rootfs/etc");
}
