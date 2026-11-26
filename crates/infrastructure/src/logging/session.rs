use super::LogError;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

pub(crate) struct LogSession {
    pub id: String,
    pub directory: PathBuf,
    pub lease: File,
}

pub(crate) fn open(root: &Path) -> Result<LogSession, LogError> {
    std::fs::create_dir_all(root)?;
    let root = root.canonicalize()?;
    let id = uuid::Uuid::new_v4().to_string();
    let directory = root.join(&id);
    std::fs::create_dir(&directory)?;
    let lease = File::create_new(directory.join("session.lock"))?;
    lease.lock()?;
    cleanup(&root, &id)?;
    Ok(LogSession {
        id,
        directory,
        lease,
    })
}

fn cleanup(root: &Path, current: &str) -> Result<(), LogError> {
    let mut ended = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == current || uuid::Uuid::parse_str(&name).is_err() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }
        let lease_path = entry.path().join("session.lock");
        if !lease_path.is_file() {
            continue;
        }
        let lease = File::options().read(true).write(true).open(&lease_path)?;
        match lease.try_lock() {
            Ok(()) => ended.push((metadata.modified()?, entry.path(), lease)),
            Err(std::fs::TryLockError::WouldBlock) => {}
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
    }
    ended.sort_by_key(|(modified, _, _)| std::cmp::Reverse(*modified));
    for (_, path, lease) in ended.into_iter().skip(4) {
        if path.canonicalize()?.parent() != Some(root) {
            return Err(LogError::InvalidQuery("log session escaped its root"));
        }
        drop(lease);
        std::fs::remove_dir_all(path)?;
    }
    Ok(())
}
