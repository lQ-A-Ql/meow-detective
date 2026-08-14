//! Host-side Linux logon bypass for emulation sessions.
//!
//! The service replaces one `/etc/shadow` password hash and applies a bounded
//! in-place rewrite exclusively through the session COW. ext4 and XFS roots
//! are supported directly or through persisted LVM mappings. XFS edits are
//! accepted only after the internal log is clean and `fs-xfs` has proved the
//! existing allocation safe for an in-place rewrite.

mod rewrite;
mod volume;

use std::sync::Arc;

use evidence_emulation::CowDisk;
use transport::dto::{EmulationLinuxAccountDto, EmulationLinuxBypassResultDto};

use crate::emulation_bypass::{BypassCaseContext, EmulationBypassError};
use rewrite::{apply_rewrite_plan, plan_shadow_rewrite, validate_rewrite_plan};
use volume::{open_linux_partition, LinuxPartition};

const SHADOW_PATH: &str = "etc/shadow";
const MAX_SHADOW_BYTES: u64 = 8 * 1024 * 1024;
pub const LINUX_BYPASS_PASSWORD: &str = "123456";
const SHA512_PASSWORD_HASH: &str = "$6$meow1234$Ece2JtWkjNGCiGYoIvqBZ8teI2U1Lmd73FwcHlczR6zRf0q8ET2EdwZ6ZaEz0WZ196VlNUTZk240LtfFdViux1";
const SHA256_PASSWORD_HASH: &str = "$5$meow1234$qQo/HTqGwuXnYwUW/4dOt0XW4nIwccjEttTNDrymHn2";
const MD5_PASSWORD_HASH: &str = "$1$meow123$b2lFkt4IaVGRkj8fDdzcD/";
const SM3_PASSWORD_HASH: &str = "$sm3$meow123456789012$hPU6PNpmDoZFQQrP9OmB8xLIg3eMh/p.Pla8NO8HyG4";
const YESCRYPT_PASSWORD_HASH: &str =
    "$y$j9T$SjOxytwbNDX3I5UCfJi0G1$gtzGm7SguXS3HSonfCo2nheVt..hc2TFSp6XE4v7BV4";
const BCRYPT_2A_PASSWORD_HASH: &str =
    "$2a$12$pns0Q7.Aa1geuiYHrwDxJ.ZzaerL4ZO1hYjAIVJUE.4ZQliufC9nq";
const BCRYPT_2B_PASSWORD_HASH: &str =
    "$2b$12$pns0Q7.Aa1geuiYHrwDxJ.ZzaerL4ZO1hYjAIVJUE.4ZQliufC9nq";
const BCRYPT_2Y_PASSWORD_HASH: &str =
    "$2y$12$pns0Q7.Aa1geuiYHrwDxJ.ZzaerL4ZO1hYjAIVJUE.4ZQliufC9nq";
const DES_PASSWORD_HASH: &str = "meYmEekhPnz3w";

pub fn list_linux_accounts(
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<Vec<EmulationLinuxAccountDto>, EmulationBypassError> {
    let partition = open_linux_partition(case_context, partition_index, None)?;
    let shadow = read_shadow(&partition)?;
    Ok(artifacts_linux::parse_shadow_accounts(&shadow)
        .into_iter()
        .map(|account| EmulationLinuxAccountDto {
            username: account.username,
            has_password: account.has_password,
            locked: account.locked,
        })
        .collect())
}

pub fn apply_linux_bypass(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    username: &str,
) -> Result<EmulationLinuxBypassResultDto, EmulationBypassError> {
    let partition = open_linux_partition(case_context, partition_index, Some(disk))?;
    let shadow = read_shadow(&partition)?;
    artifacts_linux::parse_shadow_accounts(&shadow)
        .into_iter()
        .find(|account| account.username == username)
        .ok_or_else(|| {
            EmulationBypassError::Edit(format!("account {username} was not found in /etc/shadow"))
        })?;
    let result = |password_set, already_configured| EmulationLinuxBypassResultDto {
        session_id: String::new(),
        data_source_id: case_context.data_source_id.0.clone(),
        partition_index,
        username: username.to_string(),
        password_set,
        already_configured,
    };
    let password_hash = replacement_password_hash(&shadow, username)?;
    let Some(edited) = artifacts_linux::set_shadow_password_hash(&shadow, username, password_hash)
        .map_err(|error| EmulationBypassError::Edit(error.to_string()))?
    else {
        return Ok(result(false, true));
    };
    let plan = plan_shadow_rewrite(&partition, edited.as_bytes())?;
    validate_rewrite_plan(&partition.mapping, &plan)?;
    if let Err(error) = apply_rewrite_plan(disk, &partition.mapping, &plan) {
        disk.invalidate();
        return Err(error);
    }
    if let Err(error) = verify_shadow_write(
        disk,
        case_context,
        partition_index,
        &edited,
        username,
        password_hash,
        &plan,
    ) {
        disk.invalidate();
        return Err(error);
    }
    Ok(result(true, false))
}

fn replacement_password_hash(
    shadow: &str,
    username: &str,
) -> Result<&'static str, EmulationBypassError> {
    let existing = shadow
        .lines()
        .find_map(|line| {
            let mut fields = line.splitn(3, ':');
            let name = fields.next()?;
            let hash = fields.next()?;
            (name == username).then_some(hash)
        })
        .ok_or_else(|| EmulationBypassError::Edit("account disappeared from /etc/shadow".into()))?;
    let unlocked = existing.trim_start_matches('!');
    let replacement = match unlocked {
        hash if hash.starts_with("$sm3$") => SM3_PASSWORD_HASH,
        hash if hash.starts_with("$6$") => SHA512_PASSWORD_HASH,
        hash if hash.starts_with("$5$") => SHA256_PASSWORD_HASH,
        hash if hash.starts_with("$1$") => MD5_PASSWORD_HASH,
        hash if hash.starts_with("$y$") => YESCRYPT_PASSWORD_HASH,
        hash if hash.starts_with("$2a$") => BCRYPT_2A_PASSWORD_HASH,
        hash if hash.starts_with("$2b$") => BCRYPT_2B_PASSWORD_HASH,
        hash if hash.starts_with("$2y$") => BCRYPT_2Y_PASSWORD_HASH,
        hash if hash.len() == DES_PASSWORD_HASH.len() && !hash.starts_with('$') => {
            DES_PASSWORD_HASH
        }
        _ => {
            return Err(EmulationBypassError::Unsupported(
                "the account password hash scheme cannot be replaced safely".into(),
            ))
        }
    };
    if replacement.len() > existing.len() {
        return Err(EmulationBypassError::Unsupported(
            "the compatible password hash would require /etc/shadow to grow".into(),
        ));
    }
    Ok(replacement)
}

fn read_shadow(partition: &LinuxPartition) -> Result<String, EmulationBypassError> {
    let size = partition
        .fs
        .file_size(SHADOW_PATH)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    if size > MAX_SHADOW_BYTES {
        return Err(EmulationBypassError::Unsupported(format!(
            "shadow file declares {size} bytes, above the {MAX_SHADOW_BYTES}-byte sanity cap"
        )));
    }
    let length = usize::try_from(size)
        .map_err(|_| EmulationBypassError::Unsupported("shadow file is too large".into()))?;
    let bytes = partition
        .fs
        .read_file_range(SHADOW_PATH, 0, length)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    String::from_utf8(bytes)
        .map_err(|error| EmulationBypassError::Edit(format!("shadow is not UTF-8: {error}")))
}

fn verify_shadow_write(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    expected: &str,
    username: &str,
    password_hash: &str,
    plan: &[rewrite::VolumePatch],
) -> Result<(), EmulationBypassError> {
    let partition = open_linux_partition(case_context, partition_index, Some(disk))?;
    rewrite::verify_patch_bytes(disk, &partition.mapping, plan)?;
    let reread = read_shadow(&partition)?;
    if reread != expected {
        return Err(EmulationBypassError::OverlayWrite(
            "overlay shadow read-back does not match the edited content".to_string(),
        ));
    }
    partition.fs.verify_rewrite_state(SHADOW_PATH, expected)?;
    let account_exists = artifacts_linux::parse_shadow_accounts(&reread)
        .into_iter()
        .find(|account| account.username == username)
        .is_some();
    let already_configured =
        artifacts_linux::set_shadow_password_hash(&reread, username, password_hash)
            .map_err(|error| EmulationBypassError::OverlayWrite(error.to_string()))?
            .is_none();
    if account_exists && already_configured {
        Ok(())
    } else {
        Err(EmulationBypassError::OverlayWrite(
            "the edited shadow does not contain the expected account password hash".to_string(),
        ))
    }
}

#[cfg(test)]
#[path = "../tests/unit/emulation_linux_bypass.rs"]
mod tests;
