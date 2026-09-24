use std::path::{Path, PathBuf};

use super::LinuxEvidenceSetSkippedEntry;

pub(super) fn collect_evidence_set_image_candidates(
    root_path: &Path,
) -> std::io::Result<(Vec<PathBuf>, Vec<LinuxEvidenceSetSkippedEntry>)> {
    let mut candidates = Vec::new();
    let mut skipped = Vec::new();
    collect_inner(root_path, root_path, &mut candidates, &mut skipped)?;
    skipped.sort_by(|left, right| {
        left.relative_path
            .cmp(&right.relative_path)
            .then_with(|| left.reason.cmp(&right.reason))
    });
    Ok((candidates, skipped))
}

fn collect_inner(
    root_path: &Path,
    directory: &Path,
    candidates: &mut Vec<PathBuf>,
    skipped: &mut Vec<LinuxEvidenceSetSkippedEntry>,
) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            skipped.push(skipped_entry(
                root_path,
                directory,
                format!("read_dir:{error}"),
            ));
            return Ok(());
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                skipped.push(skipped_entry(
                    root_path,
                    directory,
                    format!("directory_entry:{error}"),
                ));
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                skipped.push(skipped_entry(
                    root_path,
                    &path,
                    format!("file_type:{error}"),
                ));
                continue;
            }
        };
        if file_type.is_dir() {
            collect_inner(root_path, &path, candidates, skipped)?;
        } else if file_type.is_file() {
            if is_secondary_e01_segment(&path) {
                skipped.push(skipped_entry(
                    root_path,
                    &path,
                    "secondary_e01_segment".to_string(),
                ));
            } else if is_evidence_set_image_candidate(&path) {
                candidates.push(path);
            }
        }
    }
    Ok(())
}

fn is_evidence_set_image_candidate(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(extension.as_str(), "e01" | "ewf" | "raw" | "dd" | "img")
}

fn is_secondary_e01_segment(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let Some(digits) = extension.strip_prefix('e') else {
        return false;
    };
    digits.len() == 2 && digits.chars().all(|ch| ch.is_ascii_digit()) && digits != "01"
}

fn skipped_entry(root_path: &Path, path: &Path, reason: String) -> LinuxEvidenceSetSkippedEntry {
    LinuxEvidenceSetSkippedEntry {
        relative_path: path.strip_prefix(root_path).unwrap_or(path).to_path_buf(),
        reason,
    }
}
