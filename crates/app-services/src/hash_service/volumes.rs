use std::io;
use std::path::Path;

pub(super) fn discover_e01_segments(path: &Path) -> io::Result<Vec<std::path::PathBuf>> {
    let mut segments = vec![path.to_path_buf()];
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let base = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !extension.eq_ignore_ascii_case("e01") && !extension.eq_ignore_ascii_case("ewf") {
        return Ok(segments);
    }
    for index in 2u32.. {
        let candidate = parent.join(format!("{base}.E{index:02}"));
        if candidate.is_file() {
            segments.push(candidate);
        } else {
            break;
        }
    }
    Ok(segments)
}
