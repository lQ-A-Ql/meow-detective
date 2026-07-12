use std::path::{Path, PathBuf};

/// Build the path for segment N of an E01 image.
pub(crate) fn build_segment_path(first_segment: &Path, segment: u32) -> PathBuf {
    if segment == 1 {
        return first_segment.to_path_buf();
    }
    let extension = first_segment
        .extension()
        .unwrap_or_default()
        .to_string_lossy();
    let file_name = first_segment
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    let parent = first_segment.parent().unwrap_or_else(|| Path::new("."));
    let base_name = file_name
        .trim_end_matches(extension.as_ref())
        .trim_end_matches('.');
    parent.join(format!("{base_name}.E{segment:02}"))
}
