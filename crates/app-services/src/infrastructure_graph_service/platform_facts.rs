use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::infrastructure_network_fact_repo::{
    InfrastructureNetworkConfigRow as ConfigRow, InfrastructureNetworkFactRecord as Fact,
};

const PARSER_ID: &str = "linux.platform.identity.v1";

pub(super) fn extract(
    case_id: &CaseId,
    source_id: &DataSourceId,
    host_id: &str,
    rows: &[ConfigRow],
) -> Vec<Fact> {
    let mut facts = Vec::new();
    for row in rows {
        let path = row.source_path.replace('\\', "/").to_ascii_lowercase();
        if path.ends_with("/etc/pve/.version") && !row.line.trim().is_empty() {
            push(
                &mut facts,
                case_id,
                source_id,
                host_id,
                row,
                "pve",
                row.line.trim(),
            );
        } else if path.ends_with("/etc/debian_version") && !row.line.trim().is_empty() {
            push(
                &mut facts,
                case_id,
                source_id,
                host_id,
                row,
                "debian",
                row.line.trim(),
            );
        }
    }
    facts
}

#[allow(clippy::too_many_arguments)]
fn push(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source_id: &DataSourceId,
    host_id: &str,
    row: &ConfigRow,
    subject: &str,
    value: &str,
) {
    super::network_facts::push_fact_with_parser(
        facts,
        &case_id.0,
        &source_id.0,
        host_id,
        row,
        "platform_version",
        subject,
        value,
        PARSER_ID,
    );
}

#[cfg(test)]
#[path = "../../tests/unit/infrastructure_graph_service/platform_facts.rs"]
mod tests;
