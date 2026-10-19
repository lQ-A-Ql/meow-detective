use crate::UpdateError;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn rollback_pending(root: &Path) -> bool {
    root.join(".meow-update/prepared").is_file()
}

pub fn reject_links(path: &Path) -> Result<(), UpdateError> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    return Err(UpdateError::Invalid("reparse point in program path"));
                }
            }
            if meta.is_symlink() {
                return Err(UpdateError::Invalid("symbolic link in program path"));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn workspace(target: &Path) -> Result<PathBuf, UpdateError> {
    reject_links(target)?;
    let root = target
        .parent()
        .ok_or(UpdateError::Invalid("missing program directory"))?;
    let workspace = root.join(".meow-update");
    reject_links(&workspace)?;
    if target.extension().and_then(|value| value.to_str()) != Some("exe") {
        return Err(UpdateError::Invalid("update target must be an executable"));
    }
    Ok(workspace)
}

pub fn apply(target: &Path, incoming: &Path) -> Result<(), UpdateError> {
    let workspace = workspace(target)?;
    if workspace.join("prepared").exists() {
        return Err(UpdateError::PendingRecovery);
    }
    let backup = workspace.join("previous.exe");
    let mut saved = fs::File::options()
        .write(true)
        .create_new(true)
        .open(&backup)?;
    std::io::copy(&mut fs::File::open(target)?, &mut saved)?;
    saved.sync_all()?;
    let mut marker = fs::File::options()
        .write(true)
        .create_new(true)
        .open(workspace.join("prepared"))?;
    marker.write_all(b"meow-portable-transaction-v1\n")?;
    marker.sync_all()?;
    if let Err(error) = replace(incoming, target) {
        recover(target)?;
        return Err(error);
    }
    Ok(())
}

fn replace(source: &Path, target: &Path) -> Result<(), UpdateError> {
    let parent = target
        .parent()
        .ok_or(UpdateError::Invalid("missing program directory"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut fs::File::open(source)?, temporary.as_file_mut())?;
    temporary.as_file().sync_all()?;
    if target.exists() {
        fs::remove_file(target)?;
    }
    temporary
        .persist(target)
        .map_err(|error| UpdateError::Io(error.error))?;
    Ok(())
}

pub fn recover(target: &Path) -> Result<(), UpdateError> {
    let workspace = workspace(target)?;
    if !workspace.join("prepared").is_file() {
        return Ok(());
    }
    if fs::read(workspace.join("prepared"))? != b"meow-portable-transaction-v1\n" {
        return Err(UpdateError::Invalid("unknown recovery marker"));
    }
    let backup = workspace.join("previous.exe");
    reject_links(&backup)?;
    replace(&backup, target)?;
    fs::remove_file(workspace.join("prepared"))?;
    fs::remove_file(backup)?;
    Ok(())
}

pub fn clean_workspace(root: &Path) -> Result<(), UpdateError> {
    let workspace = root.join(".meow-update");
    reject_links(&workspace)?;
    if workspace.join("prepared").exists() {
        return Err(UpdateError::PendingRecovery);
    }
    for name in [
        "previous.exe",
        "update.exe",
        "update.sig",
        "ready",
        "booted",
        "error.txt",
        "guard",
    ] {
        let file = workspace.join(name);
        reject_links(&file)?;
        match fs::remove_file(file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    fs::remove_dir(workspace)?;
    Ok(())
}
