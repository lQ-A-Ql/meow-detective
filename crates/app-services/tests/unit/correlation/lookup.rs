use crate::correlation::rules::{
    basename, find_best_file_by_name, find_best_file_by_path, normalize_path, FileLookup,
};
use crate::correlation::{deleted_preference_score, path_suffix_key};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};

fn file(id: &str, path: &str, name: &str, entry_type: EntryType, deleted: bool) -> FileEntry {
    FileEntry {
        id: FileEntryId(id.to_string()),
        parent_id: None,
        data_source_id: DataSourceId("ds-1".to_string()),
        path: path.to_string(),
        name: name.to_string(),
        entry_type,
        size: Some(1024),
        ext: None,
        deleted,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

fn catalog() -> Vec<FileEntry> {
    vec![
        file(
            "win-cmd",
            "C:/Windows/System32/cmd.exe",
            "cmd.exe",
            EntryType::File,
            false,
        ),
        file(
            "desktop-cmd",
            "C:/Users/Admin/Desktop/cmd.exe",
            "cmd.exe",
            EntryType::File,
            false,
        ),
        file(
            "partition-sshd",
            "[P1]/usr/sbin/sshd",
            "sshd",
            EntryType::File,
            false,
        ),
        file(
            "partition-backup",
            "[P1]/usr/local/bin/backup.sh",
            "backup.sh",
            EntryType::File,
            false,
        ),
        file(
            "etc-hosts",
            "[P1]/etc/hosts",
            "hosts",
            EntryType::File,
            false,
        ),
        file(
            "dir-notepad",
            "C:/Windows/System32/notepad.exe",
            "notepad.exe",
            EntryType::Directory,
            false,
        ),
        file(
            "deleted-secrets",
            "C:/Users/Admin/Desktop/secrets.txt",
            "secrets.txt",
            EntryType::File,
            true,
        ),
        file(
            "live-secrets",
            "C:/Users/Admin/Desktop/secrets.txt",
            "secrets.txt",
            EntryType::File,
            false,
        ),
        file(
            "short-report",
            "D:/Data/report.docx",
            "report.docx",
            EntryType::File,
            false,
        ),
        file(
            "long-report",
            "D:/Data/Exports/Quarterly/report.docx",
            "Report.DOCX",
            EntryType::File,
            false,
        ),
        file(
            "alpha-shared",
            "X:/a/shared.txt",
            "shared.txt",
            EntryType::File,
            false,
        ),
        file(
            "beta-shared",
            "Y:/b/shared.txt",
            "shared.txt",
            EntryType::File,
            false,
        ),
    ]
}

fn reference_by_path(
    files: &[FileEntry],
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<String> {
    let normalized = normalize_path(candidate);
    if normalized.is_empty() {
        return None;
    }
    let exact = files
        .iter()
        .filter(|entry| entry.is_file())
        .filter(|entry| normalize_path(&entry.path) == normalized)
        .min_by_key(|entry| {
            (
                deleted_preference_score(entry, prefer_deleted),
                entry.path.len(),
            )
        });
    if let Some(entry) = exact {
        return Some(entry.id.0.clone());
    }
    let suffix = path_suffix_key(&normalized);
    if suffix.is_empty() {
        return None;
    }
    files
        .iter()
        .filter(|entry| entry.is_file())
        .filter(|entry| path_suffix_key(&entry.path).ends_with(&suffix))
        .min_by_key(|entry| {
            (
                deleted_preference_score(entry, prefer_deleted),
                entry.path.len(),
            )
        })
        .map(|entry| entry.id.0.clone())
}

fn reference_by_name(
    files: &[FileEntry],
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<String> {
    let normalized = basename(candidate);
    if normalized.is_empty() {
        return None;
    }
    files
        .iter()
        .filter(|entry| entry.is_file())
        .filter(|entry| entry.name.eq_ignore_ascii_case(&normalized))
        .min_by_key(|entry| {
            (
                deleted_preference_score(entry, prefer_deleted),
                entry.path.len(),
            )
        })
        .map(|entry| entry.id.0.clone())
}

fn path_id<'a>(
    lookup: &FileLookup<'a>,
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<&'a str> {
    find_best_file_by_path(lookup, candidate, prefer_deleted).map(|entry| entry.id.0.as_str())
}

fn name_id<'a>(
    lookup: &FileLookup<'a>,
    candidate: &str,
    prefer_deleted: Option<bool>,
) -> Option<&'a str> {
    find_best_file_by_name(lookup, candidate, prefer_deleted).map(|entry| entry.id.0.as_str())
}

#[test]
fn path_index_matches_linear_reference_over_varied_paths() {
    let files = catalog();
    let lookup = FileLookup::new(&files);
    let candidates = [
        "C:/Windows/System32/cmd.exe",
        "c:\\windows\\system32\\CMD.EXE",
        "C:\\Windows\\System32\\cmd.exe",
        "/usr/sbin/sshd",
        "/usr/local/bin/backup.sh",
        "usr/sbin/sshd",
        "bin/sshd",
        "/etc/hosts",
        "C:/missing/file.exe",
        "cmd.exe",
        "notepad.exe",
        "secrets.txt",
        "report.docx",
        "shared.txt",
        "/var/log/syslog",
        "",
    ];
    for prefer_deleted in [None, Some(true), Some(false)] {
        for candidate in candidates {
            assert_eq!(
                path_id(&lookup, candidate, prefer_deleted).map(str::to_string),
                reference_by_path(&files, candidate, prefer_deleted),
                "path lookup diverged for {candidate:?} (prefer_deleted={prefer_deleted:?})"
            );
            assert_eq!(
                name_id(&lookup, candidate, prefer_deleted).map(str::to_string),
                reference_by_name(&files, candidate, prefer_deleted),
                "name lookup diverged for {candidate:?} (prefer_deleted={prefer_deleted:?})"
            );
        }
    }
}

#[test]
fn path_index_applies_deleted_preference_shortest_path_and_slice_order() {
    let files = catalog();
    let lookup = FileLookup::new(&files);

    // Exact path with a deleted and a live twin: preference decides.
    let exact = "C:/Users/Admin/Desktop/secrets.txt";
    assert_eq!(path_id(&lookup, exact, Some(true)), Some("deleted-secrets"));
    assert_eq!(path_id(&lookup, exact, Some(false)), Some("live-secrets"));
    // No preference keeps the first minimum in slice order.
    assert_eq!(path_id(&lookup, exact, None), Some("deleted-secrets"));

    // Shortest path wins for suffix and name matches.
    assert_eq!(name_id(&lookup, "report.docx", None), Some("short-report"));
    assert_eq!(
        path_id(&lookup, "C:/Data/report.docx", None),
        Some("short-report")
    );

    // Equal-length ties keep the first entry of the source slice.
    assert_eq!(name_id(&lookup, "shared.txt", None), Some("alpha-shared"));

    // Directories are never candidates, even with a matching basename.
    assert!(path_id(&lookup, "notepad.exe", None).is_none());
    assert!(name_id(&lookup, "notepad.exe", None).is_none());

    // Partition-prefixed catalog paths still match bare absolute candidates.
    assert_eq!(
        path_id(&lookup, "/usr/sbin/sshd", None),
        Some("partition-sshd")
    );
}

#[test]
fn duplicate_names_and_suffixes_preserve_the_best_file_at_scale() {
    let files = (0..10_000)
        .map(|index| {
            file(
                &format!("file-{index}"),
                &format!("C:/folder-{index:05}/shared.txt"),
                "shared.txt",
                EntryType::File,
                index % 2 == 0,
            )
        })
        .collect::<Vec<_>>();
    let lookup = FileLookup::new(&files);
    for preference in [None, Some(true), Some(false)] {
        assert_eq!(
            path_id(&lookup, "ared.txt", preference).map(str::to_owned),
            reference_by_path(&files, "ared.txt", preference)
        );
        assert_eq!(
            name_id(&lookup, "shared.txt", preference).map(str::to_owned),
            reference_by_name(&files, "shared.txt", preference)
        );
    }
}
