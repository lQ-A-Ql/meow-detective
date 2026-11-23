//! Read-only planning for the host-side Linux shadow bypass.
//!
//! `plan_linux_bypass` never writes the overlay and never calls
//! `CowDisk::invalidate`. It classifies the partition into a host in-place edit
//! (possibly after an ext4 journal repair) or an explicitly unavailable layout.
//! The apply stage consumes the returned plan.

use std::sync::Arc;

use evidence_emulation::CowDisk;
use transport::dto::{EmulationLinuxBypassBlockerDto, EmulationLinuxBypassMethodDto};

use super::common::{
    current_unix_day, is_interactive_shell, read_login_defs, read_passwd, read_shadow,
    replacement_password_hash, shadow_expiry_day, SHADOW_PATH,
};
use super::rewrite::{fit_shadow_content, plan_ext4_rewrite, VolumePatch};
use super::volume::{
    load_partition_record, map_xfs_rewrite_error, open_linux_partition, record_encrypted,
    record_filesystem_kind, xfs_blocker_to_dto, FilesystemKind, LinuxFilesystem, LinuxPartition,
};
use crate::emulation_bypass::{BypassCaseContext, EmulationBypassError};

/// A verified host in-place edit plan. `patches` is only authoritative when
/// `requires_journal_repair == false`; a dirty-journal ext4 volume defers the
/// patch build until after repair.
pub struct HostInPlacePlan {
    pub requires_journal_repair: bool,
    pub(crate) patches: Vec<VolumePatch>,
}

/// Read-only verdict for one partition/user combination.
pub enum LinuxBypassPlan {
    /// The host can edit `/etc/shadow` in place over the COW overlay.
    HostInPlace {
        partition: u32,
        plan: HostInPlacePlan,
        edited: String,
        password_hash: String,
    },
    /// The client cannot safely edit this layout; report the blocker.
    Unavailable {
        blocker: EmulationLinuxBypassBlockerDto,
    },
}

/// Read-only pre-flight plan. Opens the partition through the overlay view when
/// a session disk is present but performs no write and no `invalidate`.
pub fn plan_linux_bypass(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    username: &str,
) -> Result<LinuxBypassPlan, EmulationBypassError> {
    let (_, record) = load_partition_record(case_context, partition_index)?;
    if record_encrypted(&record) {
        return Ok(unavailable(EmulationLinuxBypassBlockerDto::EncryptedVolume));
    }
    let Some(kind) = record_filesystem_kind(&record) else {
        return Ok(unavailable(
            EmulationLinuxBypassBlockerDto::UnsupportedLayout,
        ));
    };

    let partition = open_linux_partition(case_context, partition_index, Some(disk))?;
    let shadow = read_shadow(&partition)?;
    let passwd = read_passwd(&partition).ok();
    if let Err(blocker) = probe_account(&shadow, passwd.as_deref(), username) {
        return Ok(unavailable(blocker));
    }

    let login_defs = read_login_defs(&partition).ok();
    let password_hash = match replacement_password_hash(&shadow, username, login_defs.as_deref()) {
        Ok(hash) => hash,
        Err(_) => {
            return Ok(unavailable(
                EmulationLinuxBypassBlockerDto::HashSchemeUnknown,
            ))
        }
    };

    reject_expired_non_resizable(&partition, &shadow, username)?;

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

    // Already configured: the target hash is present; the apply stage reports
    // `already_configured` without writing anything.
    let Some(edited) = edited else {
        return Ok(LinuxBypassPlan::HostInPlace {
            partition: partition_index,
            plan: HostInPlacePlan {
                requires_journal_repair: false,
                patches: Vec::new(),
            },
            edited: shadow,
            password_hash: password_hash.to_string(),
        });
    };

    match kind {
        FilesystemKind::Ext4 => plan_ext4(&partition, partition_index, edited, password_hash),
        FilesystemKind::Xfs => plan_xfs(&partition, partition_index, edited, password_hash),
    }
}

fn unavailable(blocker: EmulationLinuxBypassBlockerDto) -> LinuxBypassPlan {
    LinuxBypassPlan::Unavailable { blocker }
}

/// Pure account/passwd probe shared by the planner and unit tests.
pub(crate) fn probe_account(
    shadow: &str,
    passwd: Option<&str>,
    username: &str,
) -> Result<(), EmulationLinuxBypassBlockerDto> {
    let account_present = artifacts_linux::parse_shadow_accounts(shadow)
        .into_iter()
        .any(|account| account.username == username);
    if !account_present {
        return Err(EmulationLinuxBypassBlockerDto::AccountNotFound);
    }
    if let Some(passwd) = passwd {
        if let Ok(accounts) = artifacts_linux::parse_passwd(passwd) {
            if let Some(account) = accounts
                .into_iter()
                .find(|account| account.username == username)
            {
                if !is_interactive_shell(&account.shell) {
                    return Err(EmulationLinuxBypassBlockerDto::UnsupportedLayout);
                }
            }
        }
    }
    Ok(())
}

/// Expired accounts on a non-resizable filesystem remain a hard read-only error
/// rather than a UI blocker.
fn reject_expired_non_resizable(
    partition: &LinuxPartition,
    shadow: &str,
    username: &str,
) -> Result<(), EmulationBypassError> {
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

fn plan_ext4(
    partition: &LinuxPartition,
    partition_index: u32,
    edited: String,
    password_hash: &'static str,
) -> Result<LinuxBypassPlan, EmulationBypassError> {
    let LinuxFilesystem::Ext4(fs) = &partition.fs else {
        unreachable!("plan_ext4 is only invoked for an Ext4 partition");
    };
    let requires_repair = ext4_journal_requires_repair(fs)?;
    if requires_repair {
        return Ok(LinuxBypassPlan::HostInPlace {
            partition: partition_index,
            plan: HostInPlacePlan {
                requires_journal_repair: true,
                patches: Vec::new(),
            },
            edited,
            password_hash: password_hash.to_string(),
        });
    }
    let fitted = fit_shadow_content(partition, edited)?;
    let patches = plan_ext4_rewrite(fs, fitted.as_bytes())?
        .into_iter()
        .map(|(volume_offset, bytes)| VolumePatch {
            volume_offset,
            bytes,
        })
        .collect();
    Ok(LinuxBypassPlan::HostInPlace {
        partition: partition_index,
        plan: HostInPlacePlan {
            requires_journal_repair: false,
            patches,
        },
        edited: fitted,
        password_hash: password_hash.to_string(),
    })
}

fn plan_xfs(
    partition: &LinuxPartition,
    partition_index: u32,
    edited: String,
    password_hash: &'static str,
) -> Result<LinuxBypassPlan, EmulationBypassError> {
    let LinuxFilesystem::Xfs(fs) = &partition.fs else {
        unreachable!("plan_xfs is only invoked for an XFS partition");
    };
    match assess_xfs(fs, edited.as_bytes()) {
        XfsVerdict::HostInPlace => {
            let plan = fs
                .plan_in_place_file_rewrite(SHADOW_PATH, edited.as_bytes())
                .map_err(map_xfs_rewrite_error)?;
            let patches = plan
                .patches
                .into_iter()
                .map(|patch| VolumePatch {
                    volume_offset: patch.volume_offset,
                    bytes: patch.bytes,
                })
                .collect();
            Ok(LinuxBypassPlan::HostInPlace {
                partition: partition_index,
                plan: HostInPlacePlan {
                    requires_journal_repair: false,
                    patches,
                },
                edited,
                password_hash: password_hash.to_string(),
            })
        }
        XfsVerdict::Unavailable(blocker) => Ok(unavailable(blocker)),
    }
}

#[derive(Debug)]
pub(crate) enum XfsVerdict {
    HostInPlace,
    Unavailable(EmulationLinuxBypassBlockerDto),
}

/// Read-only XFS assessment shared by the planner and unit tests.
pub(crate) fn assess_xfs(fs: &fs_xfs::XfsReader, edited: &[u8]) -> XfsVerdict {
    let assessment = fs.assess_in_place_file_rewrite(SHADOW_PATH, edited);
    if assessment.supported {
        return XfsVerdict::HostInPlace;
    }
    let blocker = assessment
        .blocker
        .map(xfs_blocker_to_dto)
        .unwrap_or(EmulationLinuxBypassBlockerDto::UnsupportedLayout);
    XfsVerdict::Unavailable(blocker)
}

/// Whether the ext4 jbd2 journal still advertises transactions for replay.
pub(crate) fn ext4_journal_requires_repair(
    fs: &fs_ext4::Ext4Reader,
) -> Result<bool, EmulationBypassError> {
    fs.journal_replay_required().map_err(map_journal_error)
}

fn map_journal_error(error: fs_ext4::journal::JournalError) -> EmulationBypassError {
    match error {
        fs_ext4::journal::JournalError::Io(io) => {
            EmulationBypassError::EvidenceRead(io.to_string())
        }
        other => EmulationBypassError::Unsupported(format!(
            "cannot prove the ext4 journal is clean: {other}"
        )),
    }
}

/// Client method corresponding to the read-only assessment.
pub(crate) fn recommended_method(supported: bool) -> EmulationLinuxBypassMethodDto {
    if supported {
        return EmulationLinuxBypassMethodDto::HostInPlace;
    }
    EmulationLinuxBypassMethodDto::None
}
