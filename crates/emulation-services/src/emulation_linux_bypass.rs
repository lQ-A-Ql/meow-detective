//! Host-side Linux logon bypass for emulation sessions.
//!
//! The service replaces one `/etc/shadow` password hash and applies a bounded
//! in-place rewrite exclusively through the session COW. ext4 and XFS roots
//! are supported directly or through persisted LVM mappings. Planning is read
//! only: [`plan_linux_bypass`] classifies the partition into a host in-place
//! edit or an explicitly unavailable layout; [`apply_linux_bypass`] consumes
//! that verdict and only writes on the host in-place path.

mod apply;
mod common;
mod inspect;
mod network;
mod plan;
mod rewrite;
mod volume;

use std::sync::Arc;

use evidence_emulation::CowDisk;
use transport::dto::EmulationLinuxAccountDto;

use crate::emulation_bypass::{BypassCaseContext, EmulationBypassError};
use common::{account_sort_key, read_passwd, read_shadow};

pub use apply::{apply_linux_bypass, verify_shadow_content};
pub use inspect::inspect_linux_bypass;
pub use plan::{plan_linux_bypass, HostInPlacePlan, LinuxBypassPlan};

pub fn prepare_network_for_emulation(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
) -> Result<bool, EmulationBypassError> {
    crate::emulation_ext4_repair::repair_ext4_journals(disk, case_context)?;
    network::prepare(disk, case_context)
}

pub fn list_linux_accounts(
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<Vec<EmulationLinuxAccountDto>, EmulationBypassError> {
    let partition = volume::open_linux_partition(case_context, partition_index, None)?;
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

#[cfg(test)]
#[path = "../tests/unit/emulation_linux_bypass.rs"]
mod tests;
