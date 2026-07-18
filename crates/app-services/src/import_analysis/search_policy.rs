use domain::{DataSourcePlatform, FileEntry};

use super::ContentBudget;

const TEXT_EXTENSIONS: &[&str] = &["txt", "log", "csv", "json", "xml", "html", "htm", "md"];
const LINUX_FORENSIC_TEXT_BASENAMES: &[&str] = &[
    "crypttab",
    "fstab",
    "group",
    "gshadow",
    "hostname",
    "hosts",
    "machine-id",
    "mtab",
    "networks",
    "os-release",
    "passwd",
    "protocols",
    "services",
    "shadow",
    "shells",
    "sudoers",
];

pub(super) fn should_index_file(file: &FileEntry, platform: DataSourcePlatform) -> bool {
    let Some(size) = file.size else {
        return false;
    };
    if size > infrastructure::constants::IMPORT_TEXT_INDEX_FILE_LIMIT_BYTES {
        return false;
    }

    is_text_extension(file) || is_linux_forensic_text_basename(file, platform)
}

pub(super) fn search_budget_allows_file(
    budget: &ContentBudget,
    file: &FileEntry,
    platform: DataSourcePlatform,
) -> bool {
    budget.allowed_extensions.is_empty()
        || budget
            .allowed_extensions
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(normalized_extension(file)))
        || is_linux_forensic_text_basename(file, platform)
}

pub(super) fn mime_hint_for_entry(
    file: &FileEntry,
    platform: DataSourcePlatform,
) -> Option<&'static str> {
    (is_text_extension(file) || is_linux_forensic_text_basename(file, platform))
        .then_some("text/plain")
}

pub(super) fn normalized_extension(file: &FileEntry) -> &str {
    file.ext
        .as_deref()
        .or_else(|| file.name.rsplit_once('.').map(|(_, ext)| ext))
        .unwrap_or("")
        .trim_start_matches('.')
}

fn is_text_extension(file: &FileEntry) -> bool {
    let extension = normalized_extension(file);
    TEXT_EXTENSIONS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(extension))
}

fn is_linux_forensic_text_basename(file: &FileEntry, platform: DataSourcePlatform) -> bool {
    if platform != DataSourcePlatform::Linux || !normalized_extension(file).is_empty() {
        return false;
    }
    let basename = file.name.trim().to_ascii_lowercase();
    LINUX_FORENSIC_TEXT_BASENAMES
        .iter()
        .any(|candidate| *candidate == basename)
}
