//! Apply stage for the Linux shadow bypass.
//!
//! Consumes the read-only [`LinuxBypassPlan`]: host in-place plans are written
//! through the COW overlay and verified; appliance/unavailable verdicts return
//! a structured result DTO without touching the disk. Only the host in-place
//! path may write or `invalidate`, and only after a failed write.

use std::sync::Arc;

use evidence_emulation::CowDisk;
use transport::dto::{
    EmulationLinuxBypassMethodDto, EmulationLinuxBypassResultDto, EmulationLinuxBypassStatusDto,
};

use super::common::{current_unix_day, read_shadow, SHADOW_PATH};
use super::plan::{appliance_alternatives, plan_linux_bypass, HostInPlacePlan, LinuxBypassPlan};
use super::rewrite::{apply_rewrite_plan, validate_rewrite_plan, verify_patch_bytes, VolumePatch};
use super::volume::open_linux_partition;
use crate::emulation_bypass::{BypassCaseContext, EmulationBypassError};

/// Plan → apply → verify for one account. The public signature is unchanged;
/// the old single-function flow is now driven by the plan verdict.
pub fn apply_linux_bypass(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    username: &str,
) -> Result<EmulationLinuxBypassResultDto, EmulationBypassError> {
    let plan = plan_linux_bypass(disk, case_context, partition_index, username)?;
    match plan {
        LinuxBypassPlan::HostInPlace {
            plan,
            edited,
            password_hash,
            ..
        } => apply_host_in_place(
            disk,
            case_context,
            partition_index,
            username,
            plan,
            edited,
            &password_hash,
        ),
        LinuxBypassPlan::Appliance { blocker, .. } => Ok(EmulationLinuxBypassResultDto {
            session_id: String::new(),
            data_source_id: case_context.data_source_id.0.clone(),
            partition_index,
            username: username.to_string(),
            password_set: false,
            already_configured: false,
            status: EmulationLinuxBypassStatusDto::Unavailable,
            method: EmulationLinuxBypassMethodDto::LinuxAppliance,
            reason: Some(blocker),
            alternatives: appliance_alternatives(),
            payload_staged: false,
        }),
        LinuxBypassPlan::Unavailable {
            blocker,
            alternatives,
        } => Ok(EmulationLinuxBypassResultDto {
            session_id: String::new(),
            data_source_id: case_context.data_source_id.0.clone(),
            partition_index,
            username: username.to_string(),
            password_set: false,
            already_configured: false,
            status: EmulationLinuxBypassStatusDto::Unavailable,
            method: EmulationLinuxBypassMethodDto::None,
            reason: Some(blocker),
            alternatives,
            payload_staged: false,
        }),
    }
}

fn apply_host_in_place(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    username: &str,
    plan: HostInPlacePlan,
    edited: String,
    password_hash: &str,
) -> Result<EmulationLinuxBypassResultDto, EmulationBypassError> {
    let (plan, edited) = if plan.requires_journal_repair {
        if let Err(error) = crate::emulation_ext4_repair::repair_ext4_journals(disk, case_context) {
            disk.invalidate();
            return Err(error);
        }
        match plan_linux_bypass(disk, case_context, partition_index, username)? {
            LinuxBypassPlan::HostInPlace { plan, edited, .. } if !plan.requires_journal_repair => {
                (plan, edited)
            }
            _ => {
                return Err(EmulationBypassError::Unsupported(
                    "journal repair did not make the ext4 volume writable".to_string(),
                ));
            }
        }
    } else {
        (plan, edited)
    };

    // Already configured: the plan returned the unchanged shadow; report success
    // without writing.
    let probe = open_linux_partition(case_context, partition_index, Some(disk))?;
    let current_shadow = read_shadow(&probe)?;
    if already_configured(&edited, &current_shadow) {
        return Ok(result_dto(
            case_context,
            partition_index,
            username,
            true,
            false,
            EmulationLinuxBypassMethodDto::HostInPlace,
        ));
    }

    let partition = open_linux_partition(case_context, partition_index, Some(disk))?;
    validate_rewrite_plan(&partition.mapping, &plan.patches)?;
    if let Err(error) = apply_rewrite_plan(disk, &partition.mapping, &plan.patches) {
        disk.invalidate();
        return Err(error);
    }
    let current_day = partition
        .fs
        .supports_file_resize()
        .then(current_unix_day)
        .transpose()?;
    if let Err(error) = verify_shadow_write(
        disk,
        case_context,
        partition_index,
        ShadowVerification {
            expected: &edited,
            username,
            password_hash,
            current_day,
            plan: &plan.patches,
        },
    ) {
        disk.invalidate();
        return Err(error);
    }
    Ok(result_dto(
        case_context,
        partition_index,
        username,
        false,
        true,
        EmulationLinuxBypassMethodDto::HostInPlace,
    ))
}

fn result_dto(
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    username: &str,
    already_configured: bool,
    password_set: bool,
    method: EmulationLinuxBypassMethodDto,
) -> EmulationLinuxBypassResultDto {
    EmulationLinuxBypassResultDto {
        session_id: String::new(),
        data_source_id: case_context.data_source_id.0.clone(),
        partition_index,
        username: username.to_string(),
        password_set,
        already_configured,
        status: EmulationLinuxBypassStatusDto::Applied,
        method,
        reason: None,
        alternatives: Vec::new(),
        payload_staged: false,
    }
}

struct ShadowVerification<'a> {
    expected: &'a str,
    username: &'a str,
    password_hash: &'a str,
    current_day: Option<u64>,
    plan: &'a [VolumePatch],
}

/// A-path verification kept from the original flow: byte-for-byte patch
/// read-back, semantic shadow re-read, and the XFS inode-state check.
fn verify_shadow_write(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    verification: ShadowVerification<'_>,
) -> Result<(), EmulationBypassError> {
    let partition = open_linux_partition(case_context, partition_index, Some(disk))?;
    verify_patch_bytes(disk, &partition.mapping, verification.plan)?;
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

/// B-path read-only verification: reopen the partition over the overlay and
/// compare the shadow byte-for-byte. Deliberately avoids
/// `plan_in_place_file_rewrite`, which can refuse a reflink target that the
/// caller only wants to confirm.
pub fn verify_shadow_content(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
    expected: &str,
) -> Result<(), EmulationBypassError> {
    let partition = open_linux_partition(case_context, partition_index, Some(disk))?;
    let reread = read_shadow(&partition)?;
    shadow_contents_match(expected, &reread)
}

/// Pure byte-for-byte shadow comparison shared by the B-path verifier and
/// unit tests; it never touches the disk or any rewrite planner.
pub(crate) fn shadow_contents_match(
    expected: &str,
    reread: &str,
) -> Result<(), EmulationBypassError> {
    if reread != expected {
        return Err(EmulationBypassError::OverlayWrite(
            "overlay shadow read-back does not match the expected content".to_string(),
        ));
    }
    Ok(())
}

/// True when the plan's edited shadow equals what the partition currently
/// holds: the account is already configured and no write is needed.
pub(crate) fn already_configured(edited: &str, current: &str) -> bool {
    edited == current
}
