use std::collections::BTreeSet;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    file_service::{read_file_bytes_for_case, SourceReadContext},
    source_db,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxEvidenceFacts {
    pub hostname: Option<String>,
    pub operating_system: Option<String>,
    pub os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub addresses: Vec<String>,
    pub roles: Vec<String>,
    pub services: Vec<String>,
    pub containers: Vec<String>,
    pub diagnostics: Vec<String>,
}

pub fn load_persisted_linux_evidence_facts(
    source_connection: &rusqlite::Connection,
    data_source_id: &DataSourceId,
) -> Option<LinuxEvidenceFacts> {
    let facts_json = source_connection
        .query_row(
            "SELECT facts_json FROM linux_evidence_facts WHERE data_source_id = ?1 AND schema_version = 1",
            [data_source_id.0.as_str()],
            |row| row.get::<_, String>(0),
        )
        .ok()?;
    serde_json::from_str(&facts_json).ok()
}

pub fn persist_linux_evidence_facts(
    source_connection: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    facts: &LinuxEvidenceFacts,
) -> Result<(), rusqlite::Error> {
    source_connection.execute(
        "INSERT INTO linux_evidence_facts (data_source_id, schema_version, facts_json) VALUES (?1, 1, ?2)
         ON CONFLICT(data_source_id) DO UPDATE SET facts_json = excluded.facts_json, updated_at = datetime('now')",
        rusqlite::params![data_source_id.0, serde_json::to_string(facts).unwrap_or_else(|_| "{}".to_string())],
    )?;
    Ok(())
}

pub fn collect_linux_evidence_facts(
    case_connection: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> LinuxEvidenceFacts {
    let Ok(source_connection) =
        source_db::open_registered_source_db_read_only(case_connection, case_root, data_source_id)
    else {
        return LinuxEvidenceFacts {
            diagnostics: vec!["source DB could not be opened read-only".to_string()],
            ..Default::default()
        };
    };
    if let Some(facts) = load_persisted_linux_evidence_facts(&source_connection, data_source_id) {
        return facts;
    }
    let mut facts = LinuxEvidenceFacts::default();
    let file_repo = FileRepo::new(&source_connection);
    let mut entries = Vec::new();
    for fragments in [
        vec!["etc/hostname"],
        vec!["os-release"],
        vec!["etc/hosts"],
        vec!["vmlinuz-"],
        vec!["etc/kubernetes/manifests"],
        vec!["etc/kubernetes/kubelet.conf"],
        vec!["var/lib/docker/containers", "config.v2.json"],
    ] {
        if let Ok(mut matches) = file_repo.find_by_path_fragments(data_source_id, &fragments) {
            entries.append(&mut matches);
        }
    }
    entries.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
    entries.dedup_by(|left, right| left.id == right.id);
    let mut context = SourceReadContext::new(
        &source_connection,
        case_connection,
        case_root,
        case_id,
        data_source_id,
    );
    let read =
        |entry: &domain::FileEntry, context: &mut SourceReadContext<'_>| -> Option<Vec<u8>> {
            read_file_bytes_for_case(
                context,
                &entry.id,
                0,
                entry.size.unwrap_or(0).min(16 * 1024 * 1024) as u32,
            )
            .ok()
        };

    if let Some(entry) = find_entry(&entries, &["etc/hostname"]) {
        facts.hostname = read(entry, &mut context).and_then(|bytes| clean_text(&bytes));
    }
    if let Some(entry) = find_entry(&entries, &["usr/lib/os-release", "etc/os-release"]) {
        if let Some(text) = read(entry, &mut context).and_then(|bytes| clean_text(&bytes)) {
            facts.operating_system =
                key_value(&text, "PRETTY_NAME").or_else(|| key_value(&text, "NAME"));
            facts.os_version = key_value(&text, "VERSION_ID");
        }
    }
    if let Some(entry) = find_entry(&entries, &["etc/hosts"]) {
        if let Some(text) = read(entry, &mut context).and_then(|bytes| clean_text(&bytes)) {
            facts.addresses = parse_hosts_addresses(&text, facts.hostname.as_deref());
        }
    }
    if let Some(entry) = entries.iter().find(|entry| {
        let path = normalized(&entry.path);
        (path.contains("/boot/vmlinuz-")
            || path.starts_with("boot/vmlinuz-")
            || entry.name.starts_with("vmlinuz-"))
            && !entry.name.starts_with("vmlinuz-0-rescue-")
    }) {
        let name = entry.name.trim_start_matches("vmlinuz-");
        if !name.is_empty() {
            facts.kernel_version = Some(name.to_string());
        }
    }

    let manifest_entries = entries
        .iter()
        .filter(|entry| {
            normalized(&entry.path).contains("etc/kubernetes/manifests/")
                && entry.name.ends_with(".yaml")
        })
        .take(32);
    for entry in manifest_entries {
        if let Some(text) = read(entry, &mut context).and_then(|bytes| clean_text(&bytes)) {
            facts.roles.push("control_plane".to_string());
            facts
                .services
                .push(entry.name.trim_end_matches(".yaml").to_string());
            for image in manifest_images(&text) {
                facts.services.push(image);
            }
        }
    }
    let has_kubelet = entries
        .iter()
        .any(|entry| path_matches(&entry.path, "etc/kubernetes/kubelet.conf"));
    if has_kubelet && !facts.roles.iter().any(|role| role == "control_plane") {
        facts.roles.push("worker".to_string());
    }

    let container_entries = entries
        .iter()
        .filter(|entry| {
            normalized(&entry.path).contains("var/lib/docker/containers/")
                && entry.name == "config.v2.json"
        })
        .take(256);
    for entry in container_entries {
        if let Some(bytes) = read(entry, &mut context) {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                let name = value["Name"]
                    .as_str()
                    .or_else(|| value["Config"]["Image"].as_str())
                    .map(|value| value.trim_start_matches('/').to_string());
                if let Some(name) = name {
                    facts.containers.push(name);
                }
            }
        }
    }
    collect_artifact_facts(&source_connection, &mut facts);
    if facts.hostname.is_none() && facts.operating_system.is_none() {
        let candidates = entries
            .iter()
            .filter(|entry| {
                let path = normalized(&entry.path);
                path.contains("hostname") || path.contains("os-release")
            })
            .take(8)
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            facts
                .diagnostics
                .push("identity paths were not present in source catalog".to_string());
        } else {
            facts.diagnostics.push(format!(
                "identity candidates present but unreadable: {candidates:?}"
            ));
        }
    }
    facts.roles = unique(facts.roles);
    facts.services = unique(facts.services);
    facts.containers = unique(facts.containers);
    facts.addresses = unique(facts.addresses);
    facts
}

fn collect_artifact_facts(
    source_connection: &rusqlite::Connection,
    facts: &mut LinuxEvidenceFacts,
) {
    let Ok(mut statement) = source_connection.prepare(
        "SELECT attrs FROM artifacts WHERE artifact_type = 'LinuxSystemConfig' AND json_valid(attrs)",
    ) else { return };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(0)) else {
        return;
    };
    for row in rows.flatten() {
        let Ok(attrs) = serde_json::from_str::<Value>(&row) else {
            continue;
        };
        let path = attrs["sourcePath"].as_str().unwrap_or_default();
        let kind = attrs["configKind"].as_str().unwrap_or_default();
        let line = attrs["line"].as_str().unwrap_or_default().trim();
        if kind == "osRelease" && is_root_identity_path(path) {
            facts.operating_system = attrs["prettyName"]
                .as_str()
                .map(str::to_string)
                .or_else(|| facts.operating_system.clone());
            facts.os_version = attrs["versionId"]
                .as_str()
                .map(str::to_string)
                .or_else(|| facts.os_version.clone());
        } else if kind == "textConfig" && path_matches(path, "etc/hostname") {
            if !line.is_empty() {
                facts.hostname = facts.hostname.clone().or_else(|| Some(line.to_string()));
            }
        } else if path_matches(path, "etc/hosts") {
            facts
                .addresses
                .extend(parse_hosts_addresses(line, facts.hostname.as_deref()));
        } else if normalized(path).contains("etc/systemd/system/") && path.ends_with(".service") {
            if let Some(name) = path.rsplit('/').next() {
                facts
                    .services
                    .push(name.trim_end_matches(".service").to_string());
            }
        }
    }
}

fn path_matches(path: &str, suffix: &str) -> bool {
    let path = normalized(path);
    path == suffix || path.ends_with(&format!("/{suffix}"))
}

fn is_root_identity_path(path: &str) -> bool {
    let path = normalized(path);
    path == "etc/os-release" || path == "usr/lib/os-release"
}

fn find_entry<'a>(
    entries: &'a [domain::FileEntry],
    suffixes: &[&str],
) -> Option<&'a domain::FileEntry> {
    suffixes.iter().find_map(|suffix| {
        entries.iter().find(|entry| {
            let path = normalized(&entry.path);
            (path == *suffix || path.ends_with(&format!("/{suffix}")))
                && !path.contains("/overlay2/")
        })
    })
}

fn normalized(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn clean_text(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn key_value(text: &str, key: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .map(|value| value.trim_matches('"').trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_hosts_addresses(text: &str, hostname: Option<&str>) -> Vec<String> {
    let Some(hostname) = hostname else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let address = fields.next()?;
            let ip = address.parse::<std::net::IpAddr>().ok()?;
            (!ip.is_loopback() && fields.any(|name| name == hostname))
                .then_some(address.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn manifest_images(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("image:"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
