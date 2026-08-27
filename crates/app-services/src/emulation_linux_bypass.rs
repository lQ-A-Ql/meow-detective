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
const PASSWD_PATH: &str = "etc/passwd";
const LOGIN_DEFS_PATH: &str = "etc/login.defs";
const MAX_SHADOW_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PASSWD_BYTES: u64 = 8 * 1024 * 1024;
const MAX_LOGIN_DEFS_BYTES: u64 = 1024 * 1024;
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

struct ShadowVerification<'a> {
    expected: &'a str,
    username: &'a str,
    password_hash: &'a str,
    current_day: Option<u64>,
    plan: &'a [rewrite::VolumePatch],
}

pub fn list_linux_accounts(
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<Vec<EmulationLinuxAccountDto>, EmulationBypassError> {
    let partition = open_linux_partition(case_context, partition_index, None)?;
    let shadow = read_shadow(&partition)?;
    let passwd = read_passwd(&partition).ok();
    let mut accounts = artifacts_linux::parse_shadow_accounts(&shadow)
        .into_iter()
        .map(|account| EmulationLinuxAccountDto {
            username: account.username,
            has_password: account.has_password,
            locked: account.locked,
        })
        .collect::<Vec<_>>();
    // Put local interactive users before service accounts and root. The UI
    // still requires an explicit investigator selection, but this ordering
    // makes the account that can actually reach a display manager visible
    // first on typical server images.
    accounts.sort_by_key(|account| account_sort_key(&account.username, passwd.as_deref()));
    Ok(accounts)
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
    validate_login_policy(&partition, &shadow, username)?;
    let result = |password_set, already_configured| EmulationLinuxBypassResultDto {
        session_id: String::new(),
        data_source_id: case_context.data_source_id.0.clone(),
        partition_index,
        username: username.to_string(),
        password_set,
        already_configured,
    };
    let login_defs = read_login_defs(&partition).ok();
    let password_hash = replacement_password_hash(&shadow, username, login_defs.as_deref())?;
    let current_day = partition
        .fs
        .supports_file_resize()
        .then(current_unix_day)
        .transpose()?;
    let edited = match current_day {
        Some(day) => {
            artifacts_linux::set_shadow_login_password(&shadow, username, password_hash, day)
        }
        None => artifacts_linux::set_shadow_password_hash(&shadow, username, password_hash),
    }
    .map_err(|error| EmulationBypassError::Edit(error.to_string()))?;
    let Some(edited) = edited else {
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
        ShadowVerification {
            expected: &edited,
            username,
            password_hash,
            current_day,
            plan: &plan,
        },
    ) {
        disk.invalidate();
        return Err(error);
    }
    Ok(result(true, false))
}

fn replacement_password_hash(
    shadow: &str,
    username: &str,
    login_defs: Option<&str>,
) -> Result<&'static str, EmulationBypassError> {
    let existing = shadow_hash(shadow, username)
        .ok_or_else(|| EmulationBypassError::Edit("account disappeared from /etc/shadow".into()))?;
    hash_replacement(existing)
        .or_else(|| login_defs.and_then(configured_hash_replacement))
        .or_else(|| peer_hash_replacement(shadow, username))
        .ok_or_else(|| {
            EmulationBypassError::Unsupported(
                "the account password hash scheme cannot be determined safely".into(),
            )
        })
}

fn shadow_hash<'a>(shadow: &'a str, username: &str) -> Option<&'a str> {
    shadow.lines().find_map(|line| {
        let mut fields = line.splitn(3, ':');
        let name = fields.next()?;
        let hash = fields.next()?;
        (name == username).then_some(hash)
    })
}

fn hash_replacement(hash: &str) -> Option<&'static str> {
    let unlocked = hash.trim_start_matches('!');
    match unlocked {
        hash if hash.starts_with("$sm3$") => SM3_PASSWORD_HASH,
        hash if hash.starts_with("$6$") => SHA512_PASSWORD_HASH,
        hash if hash.starts_with("$5$") => SHA256_PASSWORD_HASH,
        hash if hash.starts_with("$1$") => MD5_PASSWORD_HASH,
        hash if hash.starts_with("$y$") => YESCRYPT_PASSWORD_HASH,
        hash if hash.starts_with("$2a$") => BCRYPT_2A_PASSWORD_HASH,
        hash if hash.starts_with("$2b$") => BCRYPT_2B_PASSWORD_HASH,
        hash if hash.starts_with("$2y$") => BCRYPT_2Y_PASSWORD_HASH,
        hash if hash.len() == DES_PASSWORD_HASH.len() && !hash.starts_with('$') && hash != "*" => {
            DES_PASSWORD_HASH
        }
        _ => return None,
    }
    .into()
}

fn configured_hash_replacement(login_defs: &str) -> Option<&'static str> {
    login_defs.lines().find_map(|line| {
        let setting = line
            .split('#')
            .next()?
            .split_whitespace()
            .collect::<Vec<_>>();
        if setting.len() != 2 || !setting[0].eq_ignore_ascii_case("ENCRYPT_METHOD") {
            return None;
        }
        match setting[1].to_ascii_uppercase().as_str() {
            "SM3" => Some(SM3_PASSWORD_HASH),
            "SHA512" => Some(SHA512_PASSWORD_HASH),
            "SHA256" => Some(SHA256_PASSWORD_HASH),
            "YESCRYPT" => Some(YESCRYPT_PASSWORD_HASH),
            "BCRYPT" => Some(BCRYPT_2B_PASSWORD_HASH),
            "MD5" => Some(MD5_PASSWORD_HASH),
            "DES" => Some(DES_PASSWORD_HASH),
            _ => None,
        }
    })
}

fn peer_hash_replacement(shadow: &str, username: &str) -> Option<&'static str> {
    shadow.lines().find_map(|line| {
        let mut fields = line.splitn(3, ':');
        let name = fields.next()?;
        let hash = fields.next()?;
        (name != username).then(|| hash_replacement(hash)).flatten()
    })
}

fn read_shadow(partition: &LinuxPartition) -> Result<String, EmulationBypassError> {
    read_bounded_file(partition, SHADOW_PATH, MAX_SHADOW_BYTES, "shadow")
}

fn read_passwd(partition: &LinuxPartition) -> Result<String, EmulationBypassError> {
    read_bounded_file(partition, PASSWD_PATH, MAX_PASSWD_BYTES, "passwd")
}

fn read_login_defs(partition: &LinuxPartition) -> Result<String, EmulationBypassError> {
    read_bounded_file(
        partition,
        LOGIN_DEFS_PATH,
        MAX_LOGIN_DEFS_BYTES,
        "login.defs",
    )
}

fn read_bounded_file(
    partition: &LinuxPartition,
    path: &str,
    maximum: u64,
    label: &str,
) -> Result<String, EmulationBypassError> {
    let size = partition
        .fs
        .file_size(path)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    if size > maximum {
        return Err(EmulationBypassError::Unsupported(format!(
            "{label} file declares {size} bytes, above the {maximum}-byte sanity cap"
        )));
    }
    let length = usize::try_from(size)
        .map_err(|_| EmulationBypassError::Unsupported(format!("{label} file is too large")))?;
    let bytes = partition
        .fs
        .read_file_range(path, 0, length)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    String::from_utf8(bytes)
        .map_err(|error| EmulationBypassError::Edit(format!("{label} is not UTF-8: {error}")))
}

fn account_sort_key(username: &str, passwd: Option<&str>) -> (u8, u32, String) {
    let Some(passwd) = passwd else {
        return (2, u32::MAX, username.to_string());
    };
    let Some(account) = artifacts_linux::parse_passwd(passwd)
        .ok()
        .and_then(|accounts| {
            accounts
                .into_iter()
                .find(|account| account.username == username)
        })
    else {
        return (2, u32::MAX, username.to_string());
    };
    let interactive_shell = is_interactive_shell(&account.shell);
    let rank = if interactive_shell && account.uid >= 1000 {
        0
    } else if interactive_shell && account.uid == 0 {
        1
    } else if interactive_shell {
        2
    } else {
        3
    };
    (rank, account.uid, account.username)
}

fn is_interactive_shell(shell: &str) -> bool {
    !matches!(
        shell.trim(),
        "" | "/bin/false" | "/usr/bin/false" | "/sbin/nologin" | "/usr/sbin/nologin"
    )
}

fn validate_login_policy(
    partition: &LinuxPartition,
    shadow: &str,
    username: &str,
) -> Result<(), EmulationBypassError> {
    if let Ok(passwd) = read_passwd(partition) {
        if let Ok(accounts) = artifacts_linux::parse_passwd(&passwd) {
            if let Some(account) = accounts
                .into_iter()
                .find(|account| account.username == username)
            {
                if !is_interactive_shell(&account.shell) {
                    return Err(EmulationBypassError::Unsupported(format!(
                        "account {username} uses a non-interactive shell and cannot log in through the guest login manager"
                    )));
                }
            }
        }
    }
    if partition.fs.supports_file_resize() {
        return Ok(());
    }
    let Some(expire_day) = shadow_expiry_day(shadow, username)? else {
        return Ok(());
    };
    if expire_day != 0 && expire_day <= current_unix_day()? {
        return Err(EmulationBypassError::Unsupported(format!(
            "account {username} is expired and this filesystem cannot resize /etc/shadow safely"
        )));
    }
    Ok(())
}

fn shadow_expiry_day(shadow: &str, username: &str) -> Result<Option<u64>, EmulationBypassError> {
    let Some(value) = shadow.lines().find_map(|line| {
        let fields = line.split(':').collect::<Vec<_>>();
        (fields.first() == Some(&username))
            .then(|| fields.get(7).copied())
            .flatten()
    }) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    value.parse::<u64>().map(Some).map_err(|_| {
        EmulationBypassError::Edit("target account has an invalid expiration day".into())
    })
}

fn current_unix_day() -> Result<u64, EmulationBypassError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EmulationBypassError::Unsupported("host clock is before UNIX epoch".into()))?
        .as_secs()
        .checked_div(86_400)
        .ok_or_else(|| EmulationBypassError::Unsupported("invalid UNIX day divisor".into()))
}

fn verify_shadow_write(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    verification: ShadowVerification<'_>,
) -> Result<(), EmulationBypassError> {
    let partition = open_linux_partition(case_context, partition_index, Some(disk))?;
    rewrite::verify_patch_bytes(disk, &partition.mapping, verification.plan)?;
    let reread = read_shadow(&partition)?;
    if reread != verification.expected {
        return Err(EmulationBypassError::OverlayWrite(
            "overlay shadow read-back does not match the edited content".to_string(),
        ));
    }
    partition
        .fs
        .verify_rewrite_state(SHADOW_PATH, verification.expected)?;
    let account_exists = artifacts_linux::parse_shadow_accounts(&reread)
        .into_iter()
        .any(|account| account.username == verification.username);
    let repeated = match verification.current_day {
        Some(day) => artifacts_linux::set_shadow_login_password(
            &reread,
            verification.username,
            verification.password_hash,
            day,
        ),
        None => artifacts_linux::set_shadow_password_hash(
            &reread,
            verification.username,
            verification.password_hash,
        ),
    };
    let already_configured = repeated
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
