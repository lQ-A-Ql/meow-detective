//! Release-only WinPE guest payload is embedded in the one downloadable EXE.
use std::{path::PathBuf, sync::OnceLock};

const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/winpe-maintenance.exe"));
static PATH: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn path() -> Option<PathBuf> {
    PATH.get().cloned()
}

pub(crate) fn initialize() -> Result<(), Box<dyn std::error::Error>> {
    if PAYLOAD.is_empty() {
        return Ok(());
    }
    let directory = dirs::cache_dir()
        .ok_or("no application cache directory")?
        .join("Meow_Detective")
        .join("embedded-tools");
    std::fs::create_dir_all(&directory)?;
    portable_updater::reject_links(&directory)?;
    let target = directory.join(format!("{}-maintenance.exe", env!("MEOW_WINPE_SHA256")));
    portable_updater::reject_links(&target)?;
    if !target.is_file() || portable_updater::hash_file(&target)? != env!("MEOW_WINPE_SHA256") {
        use std::io::Write;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
        temporary.write_all(PAYLOAD)?;
        temporary.as_file().sync_all()?;
        temporary.persist(&target)?;
    }
    let _ = PATH.set(target);
    Ok(())
}
