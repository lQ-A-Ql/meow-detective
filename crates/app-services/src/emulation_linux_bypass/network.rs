use std::sync::Arc;

use super::{rewrite, volume};
use crate::emulation_bypass::{BypassCaseContext, EmulationBypassError};
use evidence_emulation::CowDisk;

const MAX_NETWORK_PROFILE_BYTES: usize = 64 * 1024;

/// Convert a static ifcfg profile to DHCP inside the emulation COW overlay.
/// The evidence image remains read-only and the rewrite preserves file size.
pub(super) fn prepare(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
) -> Result<bool, EmulationBypassError> {
    for partition_index in 0..64 {
        match prepare_network_partition(disk, case_context, partition_index) {
            Ok(true) => return Ok(true),
            Ok(false) => continue,
            Err(EmulationBypassError::PartitionNotFound { .. }) => continue,
            Err(EmulationBypassError::Unsupported(_)) => continue,
            Err(EmulationBypassError::EvidenceRead(_)) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

fn prepare_network_partition(
    disk: &Arc<CowDisk>,
    case_context: &BypassCaseContext<'_>,
    partition_index: u32,
) -> Result<bool, EmulationBypassError> {
    let partition = volume::open_linux_partition(case_context, partition_index, Some(disk))?;
    let Some(profile) = find_ifcfg_profile(&partition) else {
        return Ok(false);
    };
    let path = format!("etc/sysconfig/network-scripts/{profile}");
    let bytes = partition
        .fs
        .read_file_range(&path, 0, MAX_NETWORK_PROFILE_BYTES + 1)
        .map_err(|error| EmulationBypassError::EvidenceRead(error.to_string()))?;
    if bytes.len() > MAX_NETWORK_PROFILE_BYTES {
        return Err(EmulationBypassError::Unsupported(
            "network profile exceeds the bounded rewrite size".to_string(),
        ));
    }
    let text =
        String::from_utf8(bytes).map_err(|error| EmulationBypassError::Edit(error.to_string()))?;
    let Some(rewritten) = rewrite_ifcfg_for_dhcp(&text)? else {
        return Ok(false);
    };
    let plan = rewrite::plan_file_rewrite(&partition, &path, rewritten.as_bytes())?;
    rewrite::validate_rewrite_plan(&partition.mapping, &plan)?;
    rewrite::apply_rewrite_plan(disk, &partition.mapping, &plan)?;
    rewrite::verify_patch_bytes(disk, &partition.mapping, &plan)?;
    Ok(true)
}

fn find_ifcfg_profile(partition: &volume::LinuxPartition) -> Option<String> {
    if let Ok(children) = partition.fs.list_children("etc/sysconfig/network-scripts") {
        if let Some(entry) = children
            .into_iter()
            .find(|entry| entry.name.starts_with("ifcfg-") && !entry.is_dir)
        {
            return Some(entry.name);
        }
    }
    let mut candidates = vec![
        "ifcfg-ens160".to_string(),
        "ifcfg-ens33".to_string(),
        "ifcfg-ens32".to_string(),
        "ifcfg-eth0".to_string(),
    ];
    candidates.extend((1..=255).map(|index| format!("ifcfg-ens{index}")));
    candidates.into_iter().find(|name| {
        let path = format!("etc/sysconfig/network-scripts/{name}");
        partition.fs.read_file_range(&path, 0, 1).is_ok()
    })
}

fn rewrite_ifcfg_for_dhcp(text: &str) -> Result<Option<String>, EmulationBypassError> {
    let keys = [
        "BOOTPROTO",
        "IPADDR",
        "IPADDR0",
        "NETMASK",
        "PREFIX",
        "GATEWAY",
        "GATEWAY0",
        "DNS",
        "DNS1",
        "DNS2",
        "DNS3",
        "DEFROUTE",
    ];
    let mut changed = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        let Some((key, _)) = line.split_once('=') else {
            lines.push(line.to_string());
            continue;
        };
        let key = key.trim();
        let value = if key == "BOOTPROTO" {
            Some("dhcp")
        } else if keys.contains(&key) {
            Some("")
        } else {
            None
        };
        let Some(value) = value else {
            lines.push(line.to_string());
            continue;
        };
        let replacement = format!("{key}={value}");
        if replacement.len() > line.len() {
            return Err(EmulationBypassError::Unsupported(format!(
                "ifcfg field {key} is too short for DHCP rewrite"
            )));
        }
        changed |= replacement != line.trim_end();
        lines.push(format!("{replacement:<width$}", width = line.len()));
    }
    if !changed {
        return Ok(None);
    }
    let mut output = lines.join("\n");
    if text.ends_with('\n') {
        output.push('\n');
    }
    Ok(Some(output))
}

#[cfg(test)]
#[path = "../../tests/unit/emulation_linux_bypass_network.rs"]
mod tests;
