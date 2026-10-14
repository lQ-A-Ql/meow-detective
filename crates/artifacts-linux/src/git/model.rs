use crate::LinuxArtifactError;
use std::collections::BTreeMap;

pub(super) const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_RECORDS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRecord {
    pub kind: String,
    pub fields: BTreeMap<String, String>,
}

pub(crate) fn record(kind: &str, line: usize, offset: usize) -> GitRecord {
    GitRecord {
        kind: kind.to_owned(),
        fields: BTreeMap::from([
            ("lineNumber".to_owned(), line.to_string()),
            ("byteOffset".to_owned(), offset.to_string()),
            ("offsetScope".to_owned(), "decodedText".to_owned()),
        ]),
    }
}

pub(crate) fn error(message: &str) -> LinuxArtifactError {
    LinuxArtifactError::ParseError {
        parser: "git.metadata",
        message: message.to_owned(),
    }
}

pub(super) fn object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(crate) fn lines(text: &str) -> Result<Vec<(usize, usize, &str)>, LinuxArtifactError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(error("Git metadata exceeds 1 MiB"));
    }
    let mut offset = 0;
    let mut output = Vec::new();
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if index == MAX_RECORDS || line.len() > 64 * 1024 {
            return Err(error("Git metadata exceeds line limits"));
        }
        output.push((index + 1, offset, line.trim_end_matches(['\r', '\n'])));
        offset += line.len();
    }
    Ok(output)
}
