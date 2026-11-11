use super::aggregate::LinuxSystemInputs;
use crate::analysis_service::candidates::LinuxArtifactCatalogSummary;
use crate::analysis_service::extraction::attr_mapping::{
    optional_bool_attr, optional_string_attr, optional_u32_attr, string_attr,
};
use domain::Artifact;
use std::cmp::Ordering;
use std::collections::HashMap;
use transport::dto::{LinuxAccountDto, LinuxSystemInfoDto};

/// Build the unpaged host overview: os-release identity, `/etc/hostname`,
/// kernel versions, and passwd/shadow account details.
pub(super) fn derive_linux_system_info(
    catalog: &LinuxArtifactCatalogSummary,
    inputs: &LinuxSystemInputs,
) -> Option<LinuxSystemInfoDto> {
    // `/etc/os-release` is authoritative; `/usr/lib/os-release` is its
    // distribution-provided fallback. The aggregate selects that precedence
    // while scanning the source once.
    let (os_pretty_name, os_id, os_version_id) = match &inputs.os_release {
        Some(row) => (
            optional_string_attr(&row.attrs, "prettyName"),
            optional_string_attr(&row.attrs, "osId"),
            optional_string_attr(&row.attrs, "versionId"),
        ),
        None => (None, None, None),
    };
    let has_identity = os_pretty_name.is_some()
        || os_id.is_some()
        || os_version_id.is_some()
        || inputs.hostname.is_some();
    // Kernel paths are only supporting evidence. A random nested `lib/modules`
    // directory must not fabricate a host overview when no Linux host artifact
    // (os-release, hostname, passwd, or shadow) was extracted.
    if !has_identity && inputs.account_count == 0 && inputs.accounts.is_empty() {
        return None;
    }

    Some(LinuxSystemInfoDto {
        os_pretty_name,
        os_id,
        os_version_id,
        hostname: inputs
            .hostname
            .as_ref()
            .map(|(_, hostname)| hostname.clone()),
        account_count: inputs.account_count,
        user_account_count: inputs.user_account_count,
        locked_account_count: inputs.locked_account_count,
        kernel_versions: load_kernel_versions(catalog),
        accounts: merge_linux_accounts(&inputs.accounts),
    })
}

fn load_kernel_versions(catalog: &LinuxArtifactCatalogSummary) -> Vec<String> {
    let mut versions = catalog.kernel_image_versions.clone();
    if versions.is_empty() {
        versions = catalog.kernel_module_versions.clone();
    }
    versions.sort_by(|left, right| compare_kernel_versions(right, left));
    versions.dedup();
    versions
}

/// Compare kernel version strings chunk by chunk: digit runs compare
/// numerically (`5.14.0` > `5.9.0`), everything else compares bytewise.
fn compare_kernel_versions(left: &str, right: &str) -> Ordering {
    let mut left_chars = left.chars().peekable();
    let mut right_chars = right.chars().peekable();
    loop {
        match (left_chars.peek().copied(), right_chars.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(left_char), Some(right_char)) => {
                if left_char.is_ascii_digit() && right_char.is_ascii_digit() {
                    let ordering = take_number(&mut left_chars).cmp(&take_number(&mut right_chars));
                    if ordering != Ordering::Equal {
                        return ordering;
                    }
                } else {
                    let ordering = left_char.cmp(&right_char);
                    if ordering != Ordering::Equal {
                        return ordering;
                    }
                    left_chars.next();
                    right_chars.next();
                }
            }
        }
    }
}

fn take_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u64 {
    let mut value = 0u64;
    while let Some(digit) = chars.peek().and_then(|c| c.to_digit(10)) {
        value = value.saturating_mul(10).saturating_add(u64::from(digit));
        chars.next();
    }
    value
}

/// Merge passwdAccount rows (identity fields) with shadowAccount rows
/// (password state) by username. Sorted by uid ascending; shadow-only
/// accounts (no uid) go last, ordered by name.
fn merge_linux_accounts(rows: &[Artifact]) -> Vec<LinuxAccountDto> {
    let mut accounts: Vec<LinuxAccountDto> = Vec::new();
    let mut index_by_username: HashMap<String, usize> = HashMap::new();
    for row in rows {
        let Some(username) = optional_string_attr(&row.attrs, "username") else {
            continue;
        };
        let index = *index_by_username
            .entry(username.clone())
            .or_insert_with(|| {
                accounts.push(LinuxAccountDto {
                    username,
                    uid: None,
                    gid: None,
                    home: None,
                    shell: None,
                    locked: None,
                    has_password: None,
                });
                accounts.len() - 1
            });
        let account = &mut accounts[index];
        match string_attr(&row.attrs, "configKind").as_str() {
            "passwdAccount" => {
                account.uid = optional_u32_attr(&row.attrs, "uid");
                account.gid = optional_u32_attr(&row.attrs, "gid");
                account.home = optional_string_attr(&row.attrs, "home");
                account.shell = optional_string_attr(&row.attrs, "shell");
            }
            "shadowAccount" => {
                account.locked = optional_bool_attr(&row.attrs, "locked");
                account.has_password = optional_bool_attr(&row.attrs, "hasPassword");
            }
            _ => {}
        }
    }
    accounts.sort_by(|left, right| match (left.uid, right.uid) {
        (Some(left_uid), Some(right_uid)) => left_uid
            .cmp(&right_uid)
            .then_with(|| left.username.cmp(&right.username)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.username.cmp(&right.username),
    });
    accounts
}
