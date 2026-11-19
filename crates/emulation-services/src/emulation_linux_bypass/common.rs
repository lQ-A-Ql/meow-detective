//! Shared read-only helpers for the Linux shadow bypass flow: bounded file
//! reads, password-hash scheme resolution, account ordering and expiry parsing.
//!
//! Everything here is pure read-only: no `CowDisk`, no overlay write, no
//! `invalidate` call.

use crate::emulation_bypass::EmulationBypassError;

use super::volume::LinuxPartition;

pub(crate) const SHADOW_PATH: &str = "etc/shadow";
pub(crate) const PASSWD_PATH: &str = "etc/passwd";
pub(crate) const LOGIN_DEFS_PATH: &str = "etc/login.defs";
const MAX_SHADOW_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PASSWD_BYTES: u64 = 8 * 1024 * 1024;
const MAX_LOGIN_DEFS_BYTES: u64 = 1024 * 1024;

pub(crate) const SHA512_PASSWORD_HASH: &str = "$6$meow1234$Ece2JtWkjNGCiGYoIvqBZ8teI2U1Lmd73FwcHlczR6zRf0q8ET2EdwZ6ZaEz0WZ196VlNUTZk240LtfFdViux1";
pub(crate) const SHA256_PASSWORD_HASH: &str =
    "$5$meow1234$qQo/HTqGwuXnYwUW/4dOt0XW4nIwccjEttTNDrymHn2";
pub(crate) const MD5_PASSWORD_HASH: &str = "$1$meow123$b2lFkt4IaVGRkj8fDdzcD/";
pub(crate) const SM3_PASSWORD_HASH: &str =
    "$sm3$meow123456789012$hPU6PNpmDoZFQQrP9OmB8xLIg3eMh/p.Pla8NO8HyG4";
pub(crate) const YESCRYPT_PASSWORD_HASH: &str =
    "$y$j9T$SjOxytwbNDX3I5UCfJi0G1$gtzGm7SguXS3HSonfCo2nheVt..hc2TFSp6XE4v7BV4";
pub(crate) const BCRYPT_2A_PASSWORD_HASH: &str =
    "$2a$12$pns0Q7.Aa1geuiYHrwDxJ.ZzaerL4ZO1hYjAIVJUE.4ZQliufC9nq";
pub(crate) const BCRYPT_2B_PASSWORD_HASH: &str =
    "$2b$12$pns0Q7.Aa1geuiYHrwDxJ.ZzaerL4ZO1hYjAIVJUE.4ZQliufC9nq";
pub(crate) const BCRYPT_2Y_PASSWORD_HASH: &str =
    "$2y$12$pns0Q7.Aa1geuiYHrwDxJ.ZzaerL4ZO1hYjAIVJUE.4ZQliufC9nq";
pub(crate) const DES_PASSWORD_HASH: &str = "meYmEekhPnz3w";

pub(crate) fn read_shadow(partition: &LinuxPartition) -> Result<String, EmulationBypassError> {
    read_bounded_file(partition, SHADOW_PATH, MAX_SHADOW_BYTES, "shadow")
}

pub(crate) fn read_passwd(partition: &LinuxPartition) -> Result<String, EmulationBypassError> {
    read_bounded_file(partition, PASSWD_PATH, MAX_PASSWD_BYTES, "passwd")
}

pub(crate) fn read_login_defs(partition: &LinuxPartition) -> Result<String, EmulationBypassError> {
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

pub(crate) fn replacement_password_hash(
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

pub(crate) fn configured_hash_replacement(login_defs: &str) -> Option<&'static str> {
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

pub(crate) fn account_sort_key(username: &str, passwd: Option<&str>) -> (u8, u32, String) {
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

pub(crate) fn is_interactive_shell(shell: &str) -> bool {
    !matches!(
        shell.trim(),
        "" | "/bin/false" | "/usr/bin/false" | "/sbin/nologin" | "/usr/sbin/nologin"
    )
}

pub(crate) fn shadow_expiry_day(
    shadow: &str,
    username: &str,
) -> Result<Option<u64>, EmulationBypassError> {
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

pub(crate) fn current_unix_day() -> Result<u64, EmulationBypassError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| EmulationBypassError::Unsupported("host clock is before UNIX epoch".into()))?
        .as_secs()
        .checked_div(86_400)
        .ok_or_else(|| EmulationBypassError::Unsupported("invalid UNIX day divisor".into()))
}
