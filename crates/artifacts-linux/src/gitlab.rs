//! GitLab configuration and structured log records; no Ruby evaluation.

use crate::git::model::{error, lines, record, GitRecord};
use crate::LinuxArtifactError;

pub(crate) fn parse(path: &str, text: &str) -> Result<Vec<GitRecord>, LinuxArtifactError> {
    if path.contains("/etc/gitlab/") {
        config(text)
    } else {
        log(text)
    }
}

fn config(text: &str) -> Result<Vec<GitRecord>, LinuxArtifactError> {
    let mut output = Vec::new();
    for (line, offset, raw) in lines(text)? {
        let value = raw.trim();
        if value.is_empty() || value.starts_with('#') {
            continue;
        }
        let parsed = if let Some(value) = value
            .strip_prefix("external_url")
            .filter(|value| value.starts_with(char::is_whitespace))
        {
            Some(("external_url", ruby_string(value.trim())?))
        } else if let Some((name, value)) = value.split_once('=') {
            let name = name.trim();
            if name.contains("listen_port") {
                Some(("listenPort", ruby_string(value.trim())?))
            } else if name.contains("repositories_storages")
                || name.contains("git_data_dirs")
                || name.ends_with("['path']")
            {
                Some(("repositoryPath", ruby_string(value.trim())?))
            } else {
                None
            }
        } else {
            None
        };
        if let Some((name, value)) = parsed {
            let mut item = record("GitLabConfig", line, offset);
            item.fields.insert(name.into(), value);
            output.push(item);
        }
    }
    Ok(output)
}

fn ruby_string(value: &str) -> Result<String, LinuxArtifactError> {
    let Some(quote @ ('\'' | '"')) = value.chars().next() else {
        let value = value.split('#').next().unwrap_or_default().trim();
        if value.parse::<u16>().is_ok() {
            return Ok(value.into());
        }
        return Err(error("unsupported GitLab configuration expression"));
    };
    let mut escaped = false;
    let mut output = String::new();
    for (index, character) in value[1..].char_indices() {
        if escaped {
            output.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            let rest = value[index + 2..].trim();
            if !rest.is_empty() && !rest.starts_with('#') {
                return Err(error("unsupported GitLab configuration suffix"));
            }
            return Ok(output);
        } else {
            output.push(character);
        }
    }
    Err(error("unterminated GitLab string"))
}

fn log(text: &str) -> Result<Vec<GitRecord>, LinuxArtifactError> {
    let mut output = Vec::new();
    for (line, offset, raw) in lines(text)? {
        if raw.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(raw).map_err(|_| error("invalid GitLab JSON log record"))?;
        let object = value
            .as_object()
            .ok_or_else(|| error("GitLab log record is not an object"))?;
        let mut item = record("GitLabLog", line, offset);
        for (name, keys) in [
            ("user", &["user", "username", "user_id"][..]),
            ("ip", &["remote_ip", "ip", "client_ip"][..]),
            ("action", &["action", "method", "event"][..]),
            ("status", &["status", "status_code"][..]),
            ("project", &["project", "project_id", "project_path"][..]),
            ("timestamp", &["time", "timestamp", "@timestamp"][..]),
            ("path", &["path", "uri"][..]),
        ] {
            if let Some(value) = keys
                .iter()
                .find_map(|key| object.get(*key))
                .filter(|value| !value.is_null())
            {
                item.fields.insert(
                    name.into(),
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                );
            }
        }
        output.push(item);
    }
    Ok(output)
}
