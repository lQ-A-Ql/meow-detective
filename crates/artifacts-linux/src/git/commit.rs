use super::model::{error, object_id, record, GitRecord, MAX_TEXT_BYTES};
use crate::LinuxArtifactError;

pub(super) fn parse(text: &str) -> Result<Vec<GitRecord>, LinuxArtifactError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(error("commit body exceeds limit"));
    }
    let (headers, message) = text
        .split_once("\n\n")
        .ok_or_else(|| error("commit has no message separator"))?;
    let mut output = record("GitCommit", 1, 0);
    let mut parents = Vec::new();
    for line in headers.lines() {
        if line.starts_with(' ') {
            continue;
        } // Multi-line signatures are retained in the source object.
        let (key, value) = line
            .split_once(' ')
            .ok_or_else(|| error("invalid commit header"))?;
        match key {
            "tree" if object_id(value) => {
                output.fields.insert("tree".into(), value.into());
            }
            "parent" if object_id(value) => parents.push(value),
            "author" | "committer" => {
                let mut parts = value.rsplitn(3, ' ');
                let zone = parts
                    .next()
                    .ok_or_else(|| error("missing commit timezone"))?;
                let timestamp = parts.next().ok_or_else(|| error("missing commit time"))?;
                let identity = parts
                    .next()
                    .ok_or_else(|| error("missing commit identity"))?;
                if timestamp.parse::<i64>().is_err()
                    || zone.len() != 5
                    || !zone.as_bytes()[1..].iter().all(u8::is_ascii_digit)
                    || !zone.starts_with(['+', '-'])
                {
                    return Err(error("invalid commit time"));
                }
                output.fields.insert(key.into(), identity.into());
                output.fields.insert(format!("{key}Time"), timestamp.into());
                output.fields.insert(format!("{key}Timezone"), zone.into());
            }
            "tree" | "parent" => return Err(error("invalid commit object ID")),
            _ => {}
        }
    }
    if !output.fields.contains_key("tree")
        || !output.fields.contains_key("author")
        || !output.fields.contains_key("committer")
    {
        return Err(error("required commit headers missing"));
    }
    output.fields.insert("parents".into(), parents.join(" "));
    output
        .fields
        .insert("message".into(), message.trim_end_matches('\n').into());
    Ok(vec![output])
}
