use super::model::{error, lines, object_id, record, GitRecord};
use crate::LinuxArtifactError;

pub(super) fn parse(text: &str) -> Result<Vec<GitRecord>, LinuxArtifactError> {
    let mut output = Vec::new();
    for (line, offset, value) in lines(text)? {
        let Some((header, message)) = value.split_once('\t') else {
            return Err(error("reflog entry has no message separator"));
        };
        let mut fields = header.splitn(3, ' ');
        let old = fields
            .next()
            .ok_or_else(|| error("missing reflog old ID"))?;
        let new = fields
            .next()
            .ok_or_else(|| error("missing reflog new ID"))?;
        let rest = fields
            .next()
            .ok_or_else(|| error("missing reflog identity"))?;
        let mut parts = rest.rsplitn(3, ' ');
        let timezone = parts
            .next()
            .ok_or_else(|| error("missing reflog timezone"))?;
        let timestamp = parts.next().ok_or_else(|| error("missing reflog time"))?;
        let actor = parts.next().ok_or_else(|| error("missing reflog actor"))?;
        if !object_id(old) || !object_id(new) || timestamp.parse::<i64>().is_err() {
            return Err(error("invalid reflog identity or time"));
        }
        let mut item = record("GitReflog", line, offset);
        for (name, value) in [
            ("oldHash", old),
            ("commitHash", new),
            ("actor", actor),
            ("timestamp", timestamp),
            ("timezone", timezone),
            ("message", message),
        ] {
            item.fields.insert(name.into(), value.into());
        }
        output.push(item);
    }
    Ok(output)
}
