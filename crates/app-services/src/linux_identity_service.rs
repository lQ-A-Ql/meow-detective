use crate::file_service::{read_file_bytes_for_case, SourceReadContext};
use artifacts_linux::{
    parse_os_release_bytes, LinuxArtifactError, OsReleaseInfo, MAX_OS_RELEASE_BYTES,
    OS_RELEASE_PATHS,
};
use evidence_core::FileSystemReader;

struct Input {
    mode: Option<u32>,
    bytes: Vec<u8>,
}

pub(crate) fn from_filesystem(
    fs: &dyn FileSystemReader,
) -> Result<Option<OsReleaseInfo>, LinuxArtifactError> {
    read(|path| {
        let (parent, name) = path
            .rsplit_once('/')
            .ok_or_else(|| invalid("invalid source path"))?;
        let children = match fs.list_children(parent) {
            Ok(children) => children,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let Some(node) = children
            .into_iter()
            .find(|node| node.name == name && !node.is_dir)
        else {
            return Ok(None);
        };
        if node.size > MAX_OS_RELEASE_BYTES as u64 {
            return Err(invalid("input exceeds byte limit"));
        }
        let bytes = fs.read_file_range(path, 0, MAX_OS_RELEASE_BYTES + 1)?;
        if bytes.len() as u64 != node.size {
            return Err(invalid("input was truncated"));
        }
        Ok(Some(Input {
            mode: node.unix_mode,
            bytes,
        }))
    })
}

pub(crate) fn from_catalog(
    entries: &[domain::FileEntry],
    context: &mut SourceReadContext<'_>,
) -> Result<Option<OsReleaseInfo>, LinuxArtifactError> {
    use crate::analysis_service::evidence_relative_path;
    let mut roots = std::collections::BTreeSet::new();
    for entry in entries
        .iter()
        .filter(|entry| !entry.deleted && !entry.encrypted)
    {
        let relative = evidence_relative_path(&entry.path);
        for path in OS_RELEASE_PATHS {
            if relative == format!("/{path}") {
                if let Some(root) = entry.path.strip_suffix(path) {
                    roots.insert(root.to_string());
                }
            }
        }
    }
    if roots.len() > 1 {
        return Err(invalid("multiple filesystem roots have OS identities"));
    }
    let Some(root) = roots.into_iter().next() else {
        return Ok(None);
    };
    read(|path| {
        let full_path = format!("{root}{path}");
        let Some(entry) = entries
            .iter()
            .find(|entry| entry.path == full_path && !entry.deleted && !entry.encrypted)
        else {
            return Ok(None);
        };
        let size = entry
            .size
            .ok_or_else(|| invalid("input size is unavailable"))?;
        if size > MAX_OS_RELEASE_BYTES as u64 {
            return Err(invalid("input exceeds byte limit"));
        }
        let bytes =
            read_file_bytes_for_case(&mut *context, &entry.id, 0, MAX_OS_RELEASE_BYTES as u32 + 1)
                .map_err(|error| {
                    tracing::warn!(%error, "OS identity evidence could not be read");
                    invalid("evidence input could not be read")
                })?;
        if bytes.len() as u64 != size {
            return Err(invalid("input was truncated"));
        }
        Ok(Some(Input {
            mode: entry.unix_mode,
            bytes,
        }))
    })
}

fn read(
    mut load: impl FnMut(&str) -> Result<Option<Input>, LinuxArtifactError>,
) -> Result<Option<OsReleaseInfo>, LinuxArtifactError> {
    let mut selected = None;
    for path in OS_RELEASE_PATHS {
        if let Some(input) = load(path)? {
            selected = Some((path.to_string(), input));
            break;
        }
    }
    let Some((mut path, mut input)) = selected else {
        return Ok(None);
    };
    for _ in 0..8 {
        if input.mode.is_some_and(|mode| mode & 0o170000 == 0o120000) {
            let target = std::str::from_utf8(&input.bytes)
                .map_err(|_| invalid("invalid symlink encoding"))?;
            path = resolve(&path, target)?;
            input = load(&path)?.ok_or_else(|| invalid("symlink target is unavailable"))?;
        } else {
            return parse_os_release_bytes(&input.bytes).map(Some);
        }
    }
    Err(invalid("symlink depth exceeds limit"))
}

fn resolve(path: &str, target: &str) -> Result<String, LinuxArtifactError> {
    if target.is_empty() || target.contains('\0') {
        return Err(invalid("invalid symlink target"));
    }
    let mut components = if target.starts_with('/') {
        Vec::new()
    } else {
        path.rsplit_once('/')
            .map(|(parent, _)| parent.split('/').collect::<Vec<_>>())
            .ok_or_else(|| invalid("invalid source path"))?
    };
    for component in target.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(invalid("symlink escapes filesystem root"));
                }
            }
            component => components.push(component),
        }
    }
    Ok(components.join("/"))
}

fn invalid(message: &str) -> LinuxArtifactError {
    LinuxArtifactError::ParseError {
        parser: "os-release",
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/linux_identity_service.rs"]
mod tests;
