use domain::{CaseId, DataSourceId, FileEntry};
use persistence_sqlite::repositories::{
    file_repo::FileRepo,
    infrastructure_network_fact_repo::{
        InfrastructureNetworkConfigRow as ConfigRow, InfrastructureNetworkFactRecord as Fact,
    },
};
use rusqlite::Connection;

use crate::{
    cluster_service::{parse_kubernetes_network_resources, KubernetesNetworkResource},
    file_service::{read_file_bytes_for_case, SourceReadContext},
};

const MAX_RESOURCE_FILES: usize = 256;
const MAX_RESOURCE_BYTES: u64 = 1024 * 1024;
const PARSER_ID: &str = "kubernetes.network.resources.v1";

pub(super) fn extract(
    case_conn: &Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    source: &DataSourceId,
    source_conn: &Connection,
    host_id: &str,
) -> (Vec<Fact>, Vec<String>) {
    let Ok(entries) = FileRepo::new(source_conn).find_by_data_source(source) else {
        return (Vec::new(), Vec::new());
    };
    let candidates = entries
        .into_iter()
        .filter(|entry| is_resource_candidate(&entry.path))
        .take(MAX_RESOURCE_FILES);
    let mut facts = Vec::new();
    let mut diagnostics = Vec::new();
    for entry in candidates {
        extract_file(
            case_conn,
            case_root,
            case_id,
            source,
            source_conn,
            host_id,
            &entry,
            &mut facts,
            &mut diagnostics,
        );
    }
    (facts, diagnostics)
}

fn is_resource_candidate(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let in_kubernetes_root = [
        "/etc/kubernetes/",
        "/var/lib/kubelet/",
        "/var/lib/rancher/",
        "/etc/rancher/",
    ]
    .iter()
    .any(|prefix| normalized.contains(prefix));
    in_kubernetes_root && (normalized.ends_with(".yaml") || normalized.ends_with(".yml"))
}

#[allow(clippy::too_many_arguments)]
fn extract_file(
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
    let size = entry.size.unwrap_or_default().min(MAX_RESOURCE_BYTES) as u32;
    let Ok(bytes) = read_file_bytes_for_case(&mut context, &entry.id, 0, size) else {
        diagnostics.push(format!(
            "Kubernetes network evidence could not be read for {}",
            entry.path
        ));
        return;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return;
    };
    match parse_kubernetes_network_resources(text) {
        Ok(resources) => append_resources(facts, case_id, source, host_id, entry, resources),
        Err(error) => diagnostics.push(format!(
            "Kubernetes network evidence could not be parsed for {}: {error}",
            entry.path
        )),
    }
}

fn append_resources(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    resources: Vec<KubernetesNetworkResource>,
) {
    for resource in resources {
        match resource {
            KubernetesNetworkResource::Node(node) => {
                append_node(facts, case_id, source, host_id, entry, node);
            }
            KubernetesNetworkResource::Service(service) => {
                append_service(facts, case_id, source, host_id, entry, service);
            }
            KubernetesNetworkResource::EndpointSlice(slice) => {
                append_endpoint_slice(facts, case_id, source, host_id, entry, slice);
            }
            KubernetesNetworkResource::Ingress(ingress) => {
                append_ingress(facts, case_id, source, host_id, entry, ingress);
            }
            KubernetesNetworkResource::NetworkPolicy(policy) => {
                append_network_policy(facts, case_id, source, host_id, entry, policy);
            }
        }
    }
}

fn append_node(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    node: crate::cluster_service::KubernetesNodeNetworkSummary,
) {
    let subject = node.name.as_deref().unwrap_or("unknown");
    for (address_kind, address) in node.addresses {
        push_resource_fact(
            facts,
            case_id,
            source,
            host_id,
            entry,
            "kubernetes_node",
            subject,
            &format!("address:{address_kind}={address}"),
        );
    }
    if let Some(pod_cidr) = node.pod_cidr {
        push_resource_fact(
            facts,
            case_id,
            source,
            host_id,
            entry,
            "kubernetes_node",
            subject,
            &format!("pod_cidr={pod_cidr}"),
        );
    }
}

fn append_service(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    service: crate::cluster_service::KubernetesServiceNetworkSummary,
) {
    let subject = namespaced_subject(service.namespace.as_deref(), service.name.as_deref());
    append_optional_fact(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_service",
        &subject,
        "type",
        service.service_type,
    );
    append_optional_fact(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_service",
        &subject,
        "cluster_ip",
        service.cluster_ip,
    );
    append_values(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_service",
        &subject,
        "port",
        service.ports,
    );
    append_values(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_service",
        &subject,
        "selector",
        service.selector_keys,
    );
}

fn append_endpoint_slice(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    slice: crate::cluster_service::KubernetesEndpointSliceSummary,
) {
    let subject = namespaced_subject(slice.namespace.as_deref(), slice.name.as_deref());
    append_optional_fact(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_endpoint_slice",
        &subject,
        "service",
        slice.service_name,
    );
    append_values(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_endpoint_slice",
        &subject,
        "address",
        slice.addresses,
    );
    append_values(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_endpoint_slice",
        &subject,
        "node",
        slice.node_names,
    );
    append_values(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_endpoint_slice",
        &subject,
        "port",
        slice.ports,
    );
}

fn append_ingress(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    ingress: crate::cluster_service::KubernetesIngressSummary,
) {
    let subject = namespaced_subject(ingress.namespace.as_deref(), ingress.name.as_deref());
    append_values(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_ingress",
        &subject,
        "host",
        ingress.hosts,
    );
    append_values(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_ingress",
        &subject,
        "backend",
        ingress.backend_services,
    );
}

fn append_network_policy(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    policy: crate::cluster_service::KubernetesNetworkPolicySummary,
) {
    let subject = namespaced_subject(policy.namespace.as_deref(), policy.name.as_deref());
    append_values(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_network_policy",
        &subject,
        "type",
        policy.policy_types,
    );
    append_values(
        facts,
        case_id,
        source,
        host_id,
        entry,
        "kubernetes_network_policy",
        &subject,
        "selector",
        policy.selector_keys,
    );
}

#[allow(clippy::too_many_arguments)]
fn append_optional_fact(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    kind: &str,
    subject: &str,
    label: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        push_resource_fact(
            facts,
            case_id,
            source,
            host_id,
            entry,
            kind,
            subject,
            &format!("{label}={value}"),
        );
    }
}

fn namespaced_subject(namespace: Option<&str>, name: Option<&str>) -> String {
    match (namespace, name) {
        (Some(namespace), Some(name)) => format!("{namespace}/{name}"),
        (None, Some(name)) => name.to_string(),
        _ => "unknown".to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn append_values(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    kind: &str,
    subject: &str,
    label: &str,
    values: Vec<String>,
) {
    for value in values {
        push_resource_fact(
            facts,
            case_id,
            source,
            host_id,
            entry,
            kind,
            subject,
            &format!("{label}={value}"),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push_resource_fact(
    facts: &mut Vec<Fact>,
    case_id: &CaseId,
    source: &DataSourceId,
    host_id: &str,
    entry: &FileEntry,
    kind: &str,
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
    super::network_facts::push_fact_with_parser(
        facts, &case_id.0, &source.0, host_id, &row, kind, subject, value, PARSER_ID,
    );
}

#[cfg(test)]
#[path = "../../tests/unit/infrastructure_graph_service/kubernetes_network_facts.rs"]
mod tests;
