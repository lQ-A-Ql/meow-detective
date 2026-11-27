use std::path::{Path, PathBuf};

use super::plan::{lvm_volume_fingerprint, LvmPlanError, LvmVolumePlan};

pub fn lvm_descriptor_path(case_root: &Path, _plan: &LvmVolumePlan, fingerprint: &str) -> PathBuf {
    case_root
        .join("derived")
        .join("lvm")
        .join(format!("{fingerprint}.json"))
}

pub fn write_lvm_descriptor(
    case_root: &Path,
    plan: &LvmVolumePlan,
) -> Result<PathBuf, LvmPlanError> {
    plan.validate()?;
    let fingerprint = lvm_volume_fingerprint(plan)?;
    let target = lvm_descriptor_path(case_root, plan, &fingerprint);
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::other("LVM descriptor has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let payload = serde_json::to_vec_pretty(plan)?;
    if target.exists() {
        if std::fs::read(&target)? != payload {
            return Err(LvmPlanError::InvalidContentIdentity);
        }
        return Ok(target);
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut temporary, &payload)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(&target)
        .map_err(|error| error.error)?;
    Ok(target)
}

pub fn read_lvm_descriptor(descriptor_path: &Path) -> Result<LvmVolumePlan, LvmPlanError> {
    let payload = std::fs::read(descriptor_path)?;
    let plan: LvmVolumePlan = serde_json::from_slice(&payload)?;
    plan.validate()?;
    let expected = lvm_descriptor_path(
        descriptor_path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::parent)
            .ok_or(LvmPlanError::InvalidContentIdentity)?,
        &plan,
        &lvm_volume_fingerprint(&plan)?,
    );
    if expected.file_name() != descriptor_path.file_name() {
        return Err(LvmPlanError::InvalidContentIdentity);
    }
    Ok(plan)
}
