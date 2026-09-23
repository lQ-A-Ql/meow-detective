use std::net::IpAddr;

use domain::{CaseId, DataSourceId, FileEntry};
use persistence_sqlite::{
    repositories::infrastructure_network_fact_repo::{
        InfrastructureNetworkConfigRow as ConfigRow, InfrastructureNetworkFactRecord as Fact,
        InfrastructureNetworkFactRepo,
    },
    DbResult,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::{
    cluster_service::parse_cni_config,
    file_service::{read_file_bytes_for_case, SourceReadContext},
    source_db,
};
use persistence_sqlite::repositories::file_repo::FileRepo;

const MAX_NETWORK_CONFIG_ROWS: usize = 8_192;
const PARSER_ID: &str = "linux.network.config.v1";
const CNI_PARSER_ID: &str = "kubernetes.network.cni.v1";
const MAX_CNI_BYTES: u32 = 512 * 1024;

pub(super) fn refresh_network_facts(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
) -> DbResult<Vec<String>> {
    let repo = InfrastructureNetworkFactRepo::new(case_conn);
    let mut diagnostics = Vec::new();
    for (host_id, source_id) in repo.list_host_sources(&case_id.0)? {
        let source = DataSourceId(source_id.clone());
        let source_conn =
            match source_db::open_registered_source_db_read_only(case_conn, case_root, &source) {
                Ok(connection) => connection,
                Err(_) => {
                    diagnostics.push(format!("network facts unavailable for source {source_id}"));
                    continue;
                }
            };
        let mut rows = InfrastructureNetworkFactRepo::list_source_config_rows(
            &source_conn,
            MAX_NETWORK_CONFIG_ROWS + 1,
        )?;
        if rows.len() > MAX_NETWORK_CONFIG_ROWS {
            diagnostics.push(format!(
                "network fact row limit reached for source {source_id}"
            ));
            continue;
        }
        rows.sort_by(|left, right| {
            left.source_path
                .cmp(&right.source_path)
                .then_with(|| left.line_number.cmp(&right.line_number))
                .then_with(|| left.artifact_id.cmp(&right.artifact_id))
        });
        let mut facts = extract_network_facts(&case_id.0, &source_id, &host_id, &rows);
        extract_cni_facts(
            case_conn,
            case_root,
            case_id,
            &source,
            &source_conn,
            &host_id,
            &mut facts,
            &mut diagnostics,
        );
        let (kubernetes_facts, kubernetes_diagnostics) = super::kubernetes_network_facts::extract(
            case_conn,
            case_root,
            case_id,
            &source,
            &source_conn,
            &host_id,
        );
        facts.extend(kubernetes_facts);
        diagnostics.extend(kubernetes_diagnostics);
        repo.replace_for_source(&case_id.0, &source_id, &facts)?;
    }
    Ok(diagnostics)
}

#[allow(clippy::too_many_arguments)]
fn extract_cni_facts(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    source: &DataSourceId,
    source_conn: &Connection,
    host_id: &str,
    facts: &mut Vec<Fact>,
    diagnostics: &mut Vec<String>,
) {
    let Ok(entries) = FileRepo::new(source_conn).find_by_data_source(source) else {
        return;
    };
    let candidates = entries.into_iter().filter(|entry| {
        let path = entry.path.replace('\\', "/").to_ascii_lowercase();
        path.contains("/etc/cni/net.d/")
            && (path.ends_with(".json") || path.ends_with(".conf") || path.ends_with(".conflist"))
    });
    for entry in candidates.take(128) {
        extract_cni_entry(
            case_conn,
            case_root,
            case_id,
            source,
            source_conn,
            host_id,
            &entry,
            facts,
            diagnostics,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn extract_cni_entry(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    source: &DataSourceId,
    source_conn: &Connection,
    host_id: &str,
    entry: &FileEntry,
    facts: &mut Vec<Fact>,
    diagnostics: &mut Vec<String>,
) {
    let mut context = SourceReadContext::new(source_conn, case_conn, case_root, case_id, source);
    let size = entry.size.unwrap_or(0).min(MAX_CNI_BYTES as u64) as u32;
    let Ok(bytes) = read_file_bytes_for_case(&mut context, &entry.id, 0, size) else {
        diagnostics.push(format!("CNI evidence could not be read for {}", entry.path));
        return;
    };
    let Ok(summary) = parse_cni_config(&bytes) else {
        diagnostics.push(format!(
            "CNI evidence could not be parsed for {}",
            entry.path
        ));
        return;
    };
    append_cni_facts(facts, case_id, source, host_id, entry, summary);
}

fn append_cni_facts(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    summary: crate::cluster_service::CniNetworkSummary,
) {
    if let Some(name) = summary.name.as_deref() {
        push_cni_fact(facts, case_id, source, host_id, entry, "name", name);
    }
    for plugin in summary.plugin_types {
        push_cni_fact(facts, case_id, source, host_id, entry, "plugin", &plugin);
    }
    if let Some(ipam) = summary.ipam_type {
        push_cni_fact(facts, case_id, source, host_id, entry, "ipam", &ipam);
    }
    for subnet in summary.subnets {
        push_cni_fact(facts, case_id, source, host_id, entry, "subnet", &subnet);
    }
    for route in summary.routes {
        push_cni_fact(facts, case_id, source, host_id, entry, "route", &route);
    }
}

fn push_cni_fact(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    subject: &str,
    value: &str,
) {
    let row = ConfigRow {
        artifact_id: entry.id.0.clone(),
        file_id: entry.id.0.clone(),
        source_path: entry.path.clone(),
        line_number: 1,
        line: value.to_string(),
    };
    push_fact_with_parser(
        facts,
        &case_id.0,
        &source.0,
        host_id,
        &row,
        "cni_network",
        subject,
        value,
        CNI_PARSER_ID,
    );
}

fn extract_network_facts(
    case_id: &str,
    source_id: &str,
    host_id: &str,
    rows: &[ConfigRow],
) -> Vec<Fact> {
    let mut facts = Vec::new();
    let mut offset = 0;
    while offset < rows.len() {
        let path = rows[offset].source_path.as_str();
        let end = rows[offset..]
            .iter()
            .position(|row| row.source_path != path)
            .map_or(rows.len(), |position| offset + position);
        let group = &rows[offset..end];
        let normalized = path.replace('\\', "/").to_ascii_lowercase();
        if normalized.ends_with("/etc/hosts") {
            parse_hosts(case_id, source_id, host_id, group, &mut facts);
        } else if normalized.ends_with("/etc/resolv.conf") {
            parse_resolver(case_id, source_id, host_id, group, &mut facts);
        } else if normalized.ends_with("/etc/network/interfaces") {
            parse_interfaces(case_id, source_id, host_id, group, &mut facts);
        } else if normalized.ends_with("/etc/pve/corosync.conf")
            || normalized.ends_with("/etc/corosync/corosync.conf")
        {
            parse_corosync(case_id, source_id, host_id, group, &mut facts);
        }
        offset = end;
    }
    facts
}

fn parse_hosts(
    case_id: &str,
    source_id: &str,
    host_id: &str,
    rows: &[ConfigRow],
    facts: &mut Vec<Fact>,
) {
    for row in rows {
        let parts = row.line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 2 || parts[0].parse::<IpAddr>().is_err() {
            continue;
        }
        for hostname in &parts[1..] {
            push_fact(
                facts,
                case_id,
                source_id,
                host_id,
                row,
                "hostname_mapping",
                hostname,
                parts[0],
            );
        }
    }
}

fn parse_resolver(
    case_id: &str,
    source_id: &str,
    host_id: &str,
    rows: &[ConfigRow],
    facts: &mut Vec<Fact>,
) {
    for row in rows {
        let parts = row.line.split_whitespace().collect::<Vec<_>>();
        if parts.len() == 2 && parts[0] == "nameserver" && parts[1].parse::<IpAddr>().is_ok() {
            push_fact(
                facts,
                case_id,
                source_id,
                host_id,
                row,
                "dns_server",
                "resolver",
                parts[1],
            );
        }
    }
}

fn parse_interfaces(
    case_id: &str,
    source_id: &str,
    host_id: &str,
    rows: &[ConfigRow],
    facts: &mut Vec<Fact>,
) {
    let mut interface = String::new();
    for row in rows {
        let parts = row.line.split_whitespace().collect::<Vec<_>>();
        if parts.len() >= 2 && parts[0] == "iface" {
            interface = parts[1].to_string();
            continue;
        }
        if interface.is_empty() || parts.len() < 2 {
            continue;
        }
        match parts[0] {
            "address" if valid_address(parts[1]) => push_fact(
                facts,
                case_id,
                source_id,
                host_id,
                row,
                "interface_address",
                &interface,
                parts[1],
            ),
            "gateway" if parts[1].parse::<IpAddr>().is_ok() => push_fact(
                facts, case_id, source_id, host_id, row, "gateway", &interface, parts[1],
            ),
            "bridge-ports" => {
                for port in &parts[1..] {
                    if *port != "none" {
                        push_fact(
                            facts,
                            case_id,
                            source_id,
                            host_id,
                            row,
                            "bridge_member",
                            &interface,
                            port,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn parse_corosync(
    case_id: &str,
    source_id: &str,
    host_id: &str,
    rows: &[ConfigRow],
    facts: &mut Vec<Fact>,
) {
    let mut node_name = String::new();
    let mut links: Vec<(&ConfigRow, String, String)> = Vec::new();
    let mut in_node = false;
    for row in rows {
        let line = row.line.trim();
        if line.starts_with("node {") {
            in_node = true;
            node_name.clear();
            links.clear();
        } else if in_node && line == "}" {
            for (link_row, link_kind, address) in links.drain(..) {
                push_fact(
                    facts,
                    case_id,
                    source_id,
                    host_id,
                    link_row,
                    "cluster_link",
                    &node_name,
                    &format!("{link_kind}:{address}"),
                );
            }
            in_node = false;
        } else if in_node {
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim();
                if key == "name" {
                    node_name = value.to_string();
                } else if key.starts_with("ring") && key.ends_with("_addr") && valid_address(value)
                {
                    links.push((row, key.to_string(), value.to_string()));
                }
            }
        }
    }
}

fn valid_address(value: &str) -> bool {
    if let Some((address, prefix)) = value.split_once('/') {
        return address.parse::<IpAddr>().ok().is_some_and(|ip| {
            prefix
                .parse::<u8>()
                .ok()
                .is_some_and(|bits| bits <= if ip.is_ipv4() { 32 } else { 128 })
        });
    }
    value.parse::<IpAddr>().is_ok()
}

#[allow(clippy::too_many_arguments)]
fn push_fact(
    facts: &mut Vec<Fact>,
    case_id: &str,
    source_id: &str,
    host_id: &str,
    row: &ConfigRow,
    kind: &str,
    subject: &str,
    value: &str,
) {
    push_fact_with_parser(
        facts, case_id, source_id, host_id, row, kind, subject, value, PARSER_ID,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_fact_with_parser(
    facts: &mut Vec<Fact>,
    case_id: &str,
    source_id: &str,
    host_id: &str,
    row: &ConfigRow,
    kind: &str,
    subject: &str,
    value: &str,
    parser: &str,
) {
    if subject.is_empty()
        || subject.len() > 256
        || value.is_empty()
        || value.len() > 1024
        || row.line_number == 0
    {
        return;
    }
    let identity = format!(
        "{case_id}\0{source_id}\0{}\0{}\0{kind}\0{subject}\0{value}",
        row.file_id, row.line_number
    );
    let id = format!(
        "network:{}",
        hex::encode(Sha256::digest(identity.as_bytes()))
    );
    facts.push(Fact {
        id,
        case_id: case_id.to_string(),
        data_source_id: source_id.to_string(),
        environment_object_id: host_id.to_string(),
        file_id: row.file_id.clone(),
        source_path: row.source_path.clone(),
        line_number: row.line_number,
        fact_kind: kind.to_string(),
        subject: subject.to_string(),
        value: value.to_string(),
        assertion_kind: "configured".to_string(),
        confidence: "candidate".to_string(),
        parser: parser.to_string(),
        source_artifact_id: row.artifact_id.clone(),
    });
}

#[cfg(test)]
#[path = "../../tests/unit/infrastructure_graph_service/network_facts.rs"]
mod tests;
