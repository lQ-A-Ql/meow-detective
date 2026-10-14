use super::model::{error, lines, record, GitRecord};
use crate::LinuxArtifactError;

pub(super) fn parse(text: &str) -> Result<Vec<GitRecord>, LinuxArtifactError> {
    let mut section = String::new();
    let mut name = String::new();
    let mut output = Vec::new();
    for (line, offset, raw) in lines(text)? {
        let value = raw.trim();
        if value.is_empty() || value.starts_with(['#', ';']) {
            continue;
        }
        if let Some(value) = value.strip_prefix('[') {
            let value = value
                .strip_suffix(']')
                .ok_or_else(|| error("unterminated config section"))?
                .trim();
            let parts = value.split_once(char::is_whitespace);
            section = parts.map_or(value, |part| part.0).to_ascii_lowercase();
            name = parts
                .map(|part| unquote(part.1.trim()))
                .transpose()?
                .unwrap_or_default();
            continue;
        }
        let (key, raw) = value
            .split_once('=')
            .map_or((value, "true"), |(key, value)| (key.trim(), value.trim()));
        if section.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(error("invalid config key or section"));
        }
        let value = unquote(raw)?;
        let mut item = record("GitConfig", line, offset);
        item.fields.insert("section".into(), section.clone());
        item.fields.insert("sectionName".into(), name.clone());
        item.fields.insert("key".into(), key.to_ascii_lowercase());
        item.fields.insert("value".into(), value.clone());
        if section == "remote" {
            item.fields.insert("remoteName".into(), name.clone());
            if key.eq_ignore_ascii_case("url") {
                item.fields.insert("remoteUrl".into(), value.clone());
            }
        }
        item.fields.insert(key.to_ascii_lowercase(), value);
        output.push(item);
    }
    Ok(output)
}

pub(super) fn unquote(value: &str) -> Result<String, LinuxArtifactError> {
    let mut output = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            output.push(match character {
                'n' => '\n',
                't' => '\t',
                'b' => '\u{0008}',
                '\\' | '"' => character,
                _ => return Err(error("invalid config escape")),
            });
            escaped = false;
        } else {
            match character {
                '"' => quoted = !quoted,
                '\\' => escaped = true,
                '#' | ';' if !quoted => break,
                _ => output.push(character),
            }
        }
    }
    if quoted || escaped {
        return Err(error("unterminated config quoted value"));
    }
    Ok(output.trim_end().to_owned())
}
