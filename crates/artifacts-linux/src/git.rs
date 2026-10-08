use std::collections::BTreeMap;
use std::io::Read;

const MAX_LOOSE_COMMIT_BYTES: u64 = 1024 * 1024;

/// A bounded, line-oriented Git metadata record. Binary pack/index objects are
/// intentionally skipped; loose commit objects are parsed only after the
/// caller has decompressed the zlib payload and supplied the textual body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRecord {
    pub kind: String,
    pub fields: BTreeMap<String, String>,
}

pub fn parse_git_text(path: &str, text: &str) -> Vec<GitRecord> {
    // Evidence paths can preserve the source filesystem's case and separator
    // conventions.  Git metadata names are case-sensitive on disk, but route
    // matching is intentionally normalized, so use a normalized copy only for
    // classification while retaining the original path for provenance.
    let normalized_path = path.replace('\\', "/").to_ascii_lowercase();
    let mut fields = BTreeMap::new();
    let kind =
        if normalized_path.ends_with("/.git/config") || normalized_path.ends_with("/.gitmodules") {
            for line in text.lines().take(512) {
                let line = line.trim();
                if let Some((key, value)) = line.split_once('=') {
                    fields.insert(key.trim().to_string(), value.trim().to_string());
                }
            }
            "GitConfig"
        } else if normalized_path.ends_with("/.git/head") {
            fields.insert("head".into(), text.trim().to_string());
            "GitHead"
        } else if normalized_path.contains("/.git/refs/")
            || normalized_path.ends_with("/.git/packed-refs")
        {
            for line in text.lines().take(4096) {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
                    continue;
                }
                let mut parts = line.split_whitespace();
                if let (Some(hash), Some(reference)) = (parts.next(), parts.next()) {
                    if is_git_object_id(hash) {
                        fields.insert(reference.to_string(), hash.to_ascii_lowercase());
                    }
                }
            }
            "GitRef"
        } else if normalized_path.contains("/.git/logs/") {
            let mut count = 0usize;
            for line in text.lines() {
                if count >= 10_000 {
                    break;
                }
                let mut parts = line.splitn(2, '\t');
                let header = parts.next().unwrap_or_default();
                let message = parts.next().unwrap_or_default().trim();
                let hp: Vec<&str> = header.split_whitespace().collect();
                if hp.len() >= 4 && is_git_object_id(hp[0]) && is_git_object_id(hp[1]) {
                    fields.insert(format!("entry_{count}_old"), hp[0].to_string());
                    fields.insert(format!("entry_{count}_new"), hp[1].to_string());
                    fields.insert(format!("entry_{count}_actor"), hp[2].to_string());
                    fields.insert(
                        format!("entry_{count}_message"),
                        message.chars().take(512).collect(),
                    );
                    count += 1;
                }
            }
            "GitReflog"
        } else {
            for line in text.lines().take(256) {
                if let Some((key, value)) = line.split_once(' ') {
                    if matches!(key, "tree" | "parent" | "author" | "committer") {
                        fields.insert(key.to_string(), value.trim().chars().take(512).collect());
                    }
                } else if !line.trim().is_empty() {
                    fields.insert("message".into(), line.trim().chars().take(1024).collect());
                    break;
                }
            }
            "GitCommit"
        };
    if fields.is_empty() {
        Vec::new()
    } else {
        vec![GitRecord {
            kind: kind.into(),
            fields,
        }]
    }
}

/// Parse a loose Git commit object. The object payload is zlib-compressed and
/// bounded before parsing so malformed objects cannot allocate unbounded data.
pub fn parse_git_bytes(path: &str, bytes: &[u8]) -> Vec<GitRecord> {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    if normalized.contains("/.git/objects/") {
        let mut decoder = flate2::read::ZlibDecoder::new(bytes);
        let mut payload = Vec::new();
        if decoder
            .by_ref()
            .take(MAX_LOOSE_COMMIT_BYTES + 1)
            .read_to_end(&mut payload)
            .is_err()
            || payload.len() as u64 > MAX_LOOSE_COMMIT_BYTES
        {
            return Vec::new();
        }
        let Some(separator) = payload.iter().position(|byte| *byte == 0) else {
            return Vec::new();
        };
        let (header, body) = payload.split_at(separator);
        let body = &body[1..];
        if !header.starts_with(b"commit ") {
            return Vec::new();
        }
        return parse_git_text(path, std::str::from_utf8(body).unwrap_or_default());
    }
    parse_git_text(path, std::str::from_utf8(bytes).unwrap_or_default())
}

fn is_git_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "../tests/unit/git.rs"]
mod tests;
