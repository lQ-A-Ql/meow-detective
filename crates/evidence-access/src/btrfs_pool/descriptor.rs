use super::{BtrfsPoolPlan, BtrfsSourceError};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

pub fn write_descriptor(root: &Path, plan: &BtrfsPoolPlan) -> Result<PathBuf, BtrfsSourceError> {
    let fingerprint = plan.fingerprint()?;
    let directory = root.join("derived/btrfs");
    std::fs::create_dir_all(&directory)?;
    let target = directory.join(format!("{fingerprint}.json"));
    let bytes = serde_json::to_vec_pretty(plan)?;
    if target.exists() {
        if std::fs::read(&target)? != bytes {
            return Err(BtrfsSourceError::Identity);
        }
        return Ok(target);
    }
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(&target)
        .map_err(|error| error.error)?;
    Ok(target)
}

pub fn read_descriptor(path: &Path) -> Result<BtrfsPoolPlan, BtrfsSourceError> {
    let plan: BtrfsPoolPlan = serde_json::from_slice(&std::fs::read(path)?)?;
    if path.file_name().and_then(|value| value.to_str())
        != Some(&format!("{}.json", plan.fingerprint()?))
    {
        return Err(BtrfsSourceError::Identity);
    }
    Ok(plan)
}
