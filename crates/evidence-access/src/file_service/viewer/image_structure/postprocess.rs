//! Post-scan structure finishing and bounded hashing.

use super::limits;
use super::report_helpers::push_finding;
use super::source::{ScanError, ScanReader};
use super::types::{ImageAnalysisReport, ImageFinding, ImageFormat, ImageIssueSeverity};

pub(super) fn finish_structure(report: &mut ImageAnalysisReport, file_size: u64) {
    let Some(structure) = report.structure.as_mut() else {
        return;
    };
    let Some(declared_end) = structure.declared_end else {
        return;
    };
    if declared_end > file_size {
        structure.truncated = true;
        structure.trailing_bytes = None;
        structure.findings.push(ImageFinding {
            code: "image.truncated",
            severity: ImageIssueSeverity::Error,
            offset: Some(declared_end),
            detail: Some(format!(
                "declared end {declared_end} exceeds file size {file_size}"
            )),
        });
        return;
    }
    let trailing = file_size - declared_end;
    structure.trailing_bytes = Some(trailing);
    if trailing > 0 {
        structure.findings.push(ImageFinding {
            code: "image.trailing-data",
            severity: if report.format == Some(ImageFormat::Tiff) {
                ImageIssueSeverity::Info
            } else {
                ImageIssueSeverity::Warning
            },
            offset: Some(declared_end),
            detail: Some(format!("{trailing} trailing bytes after the declared end")),
        });
    }
}

pub(super) fn apply_hashes(reader: &mut ScanReader<'_>, report: &mut ImageAnalysisReport) {
    let Some(structure) = report.structure.as_ref() else {
        return;
    };
    let suspicious = structure.truncated
        || structure.findings.iter().any(|finding| {
            matches!(
                finding.severity,
                ImageIssueSeverity::Warning | ImageIssueSeverity::Error
            )
        });
    if !suspicious {
        return;
    }
    let file_size = reader.size();
    if file_size > limits::WHOLE_HASH_LIMIT {
        push_finding(
            report,
            "image.hash.skipped-oversized",
            ImageIssueSeverity::Info,
            None,
            Some(format!(
                "whole-file hash skipped for a {file_size}-byte image"
            )),
        );
        return;
    }
    let trailing = report
        .structure
        .as_ref()
        .and_then(|structure| structure.declared_end.map(|end| (end, file_size - end)))
        .filter(|(_, length)| *length > 0 && *length <= limits::TRAILING_HASH_LIMIT as u64);
    if let Some((start, length)) = trailing {
        if let Ok(bytes) = reader.read_unbudgeted(start, length as usize) {
            if let Some(structure) = report.structure.as_mut() {
                structure.hashes.trailing_sha256 = Some(sha256_hex(&bytes));
                structure.hashes.trailing_hashed_bytes = Some(length);
            }
        }
    }
    if file_size <= limits::WHOLE_HASH_LIMIT {
        if let Ok(hash) = hash_whole_file(reader, file_size) {
            if let Some(structure) = report.structure.as_mut() {
                structure.hashes.whole_file_sha256 = Some(hash);
            }
        }
    }
}

fn hash_whole_file(reader: &mut ScanReader<'_>, file_size: u64) -> Result<String, ScanError> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut offset = 0u64;
    while offset < file_size {
        let length = (file_size - offset).min(64 * 1024) as usize;
        let bytes = reader.read_unbudgeted(offset, length)?;
        hasher.update(&bytes);
        offset += length as u64;
    }
    Ok(hex::encode(hasher.finalize()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}
