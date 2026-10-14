use super::model::{error, lines, object_id, record, GitRecord};
use crate::LinuxArtifactError;

pub(super) fn parse(path: &str, text: &str) -> Result<Vec<GitRecord>, LinuxArtifactError> {
    let normalized = path.replace('\\', "/");
    let folded = normalized.to_ascii_lowercase();
    if let Some(index) = folded.find("/.git/refs/") {
        let value = text.trim();
        if !object_id(value) {
            return Err(error("invalid loose ref object ID"));
        }
        let reference = &normalized[index + "/.git/".len()..];
        let mut output = record("GitRef", 1, 0);
        output.fields.insert("ref".into(), reference.into());
        output
            .fields
            .insert("commitHash".into(), value.to_ascii_lowercase());
        output
            .fields
            .insert(reference.into(), value.to_ascii_lowercase());
        return Ok(vec![output]);
    }
    let mut output: Vec<GitRecord> = Vec::new();
    for (line, offset, value) in lines(text)? {
        if value.is_empty() || value.starts_with('#') {
            continue;
        }
        if let Some(peeled) = value.strip_prefix('^') {
            if !object_id(peeled) {
                return Err(error("invalid packed peeled object ID"));
            }
            let prior = output
                .last_mut()
                .ok_or_else(|| error("peeled ref without a parent"))?;
            prior
                .fields
                .insert("peeledHash".into(), peeled.to_ascii_lowercase());
            continue;
        }
        let mut parts = value.split_whitespace();
        let hash = parts
            .next()
            .ok_or_else(|| error("packed ref without object ID"))?;
        let reference = parts
            .next()
            .ok_or_else(|| error("packed ref without ref name"))?;
        if !object_id(hash) || parts.next().is_some() || !reference.starts_with("refs/") {
            return Err(error("invalid packed ref"));
        }
        let mut item = record("GitRef", line, offset);
        item.fields.insert("ref".into(), reference.into());
        item.fields
            .insert("commitHash".into(), hash.to_ascii_lowercase());
        item.fields
            .insert(reference.into(), hash.to_ascii_lowercase());
        output.push(item);
    }
    Ok(output)
}
