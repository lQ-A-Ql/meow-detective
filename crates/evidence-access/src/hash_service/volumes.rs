use std::io;
use std::path::Path;

pub(crate) fn discover_e01_segments(path: &Path) -> io::Result<Vec<std::path::PathBuf>> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("e01") && !extension.eq_ignore_ascii_case("ewf") {
        return Ok(vec![path.to_path_buf()]);
    }
    let base = path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "E01 segment base is missing")
        })?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut indexed = Vec::new();
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(index) = e01_segment_index(&name, base) {
            indexed.push((index, entry.path()));
        }
    }
    indexed.sort_by_key(|(index, _)| *index);
    if indexed.first().map(|(index, _)| *index) != Some(1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "E01 segment set does not start at E01",
        ));
    }
    for (expected, (index, _)) in indexed.iter().enumerate() {
        if *index != expected as u32 + 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "E01 segment set contains a gap",
            ));
        }
    }
    Ok(indexed.into_iter().map(|(_, path)| path).collect())
}

fn e01_segment_index(file_name: &str, base: &str) -> Option<u32> {
    if file_name.len() < base.len() {
        return None;
    }
    if !file_name[..base.len()].eq_ignore_ascii_case(base) {
        return None;
    }
    if file_name[base.len()..].eq_ignore_ascii_case(".ewf") {
        return Some(1);
    }
    let suffix = file_name
        .get(base.len()..)?
        .strip_prefix('.')?
        .strip_prefix('E')
        .or_else(|| file_name.get(base.len()..)?.strip_prefix(".e"))?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}
