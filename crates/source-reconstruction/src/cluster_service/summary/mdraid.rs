use std::collections::{BTreeMap, BTreeSet};

use domain::{DataSource, DataSourceKind};
use rusqlite::Connection;
use transport::dto::{LinuxMdRaidArrayDto, LinuxMdRaidMemberDto};

use super::Result;

pub(super) fn load_persisted_arrays(
    conn: &Connection,
    import_set_id: &str,
    sources: &BTreeMap<String, DataSource>,
) -> Result<Vec<LinuxMdRaidArrayDto>> {
    let member_paths = conn
        .prepare("SELECT source_path FROM linux_import_set_members WHERE import_set_id = ?1")?
        .query_map([import_set_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<BTreeSet<_>, _>>()?;
    sources
        .values()
        .filter(|source| source.kind == DataSourceKind::MdRaid)
        .map(|source| {
            let payload = std::fs::read(&source.source_path)?;
            let plan: evidence_access::mdraid::MdRaidArrayPlan = serde_json::from_slice(&payload)?;
            if !super::mdraid_plan_belongs_to_set(&plan, &member_paths) {
                return Ok(None);
            }
            Ok(Some(LinuxMdRaidArrayDto {
                array_uuid: hex::encode(plan.array_uuid),
                name: plan.name,
                level: plan.level,
                layout: plan.layout,
                chunk_sectors: plan.chunk_sectors,
                raid_disks: plan.raid_disks,
                min_data_size_sectors: plan.min_data_size_sectors,
                capacity_mismatch: plan.capacity_mismatch,
                filesystem: "candidate".to_string(),
                members: plan
                    .members
                    .into_iter()
                    .map(|member| LinuxMdRaidMemberDto {
                        dev_number: member.superblock.dev_number,
                        vg_name: member.origin.vg_name(),
                        lv_name: member.origin.lv_name(),
                        data_offset_sectors: member.superblock.data_offset_sectors,
                        data_size_sectors: member.superblock.data_size_sectors,
                        super_offset_sectors: member.superblock.super_offset_sectors,
                    })
                    .collect(),
            }))
        })
        .filter_map(|result| result.transpose())
        .collect()
}
