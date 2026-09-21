use super::kubernetes_parser_error::Result;
use super::kubernetes_yaml::{
    map_bool, map_get, map_seq, map_string, parse_yaml_documents, YamlNode,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticPodManifestSummary {
    pub api_version: Option<String>,
    pub kind: Option<String>,
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub container_count: usize,
    pub containers: Vec<ManifestContainer>,
    pub host_network: bool,
    pub host_pid: bool,
    pub host_ipc: bool,
    pub host_path_count: usize,
    pub privileged_container_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestContainer {
    pub name: Option<String>,
    pub image: Option<String>,
    pub privileged: bool,
    pub allow_privilege_escalation: Option<bool>,
    pub command_count: usize,
    pub args_count: usize,
}

pub fn parse_static_pod_manifests(input: &str) -> Result<Vec<StaticPodManifestSummary>> {
    parse_yaml_documents(input)?
        .iter()
        .map(parse_manifest)
        .collect()
}

fn parse_manifest(node: &YamlNode) -> Result<StaticPodManifestSummary> {
    let metadata = map_get(node, "metadata").unwrap_or(&YamlNode::Null);
    let spec = map_get(node, "spec").unwrap_or(&YamlNode::Null);
    let containers = map_seq(spec, "containers")
        .iter()
        .map(parse_container)
        .collect::<Vec<_>>();
    let privileged_container_count = containers
        .iter()
        .filter(|container| container.privileged)
        .count();
    let host_path_count = map_seq(spec, "volumes")
        .iter()
        .filter(|volume| map_get(volume, "hostPath").is_some())
        .count();
    Ok(StaticPodManifestSummary {
        api_version: map_string(node, "apiVersion"),
        kind: map_string(node, "kind"),
        name: map_string(metadata, "name"),
        namespace: map_string(metadata, "namespace"),
        container_count: containers.len(),
        containers,
        host_network: map_bool(spec, "hostNetwork").unwrap_or(false),
        host_pid: map_bool(spec, "hostPID").unwrap_or(false),
        host_ipc: map_bool(spec, "hostIPC").unwrap_or(false),
        host_path_count,
        privileged_container_count,
    })
}

fn parse_container(node: &YamlNode) -> ManifestContainer {
    let security = map_get(node, "securityContext").unwrap_or(&YamlNode::Null);
    ManifestContainer {
        name: map_string(node, "name"),
        image: map_string(node, "image"),
        privileged: map_bool(security, "privileged").unwrap_or(false),
        allow_privilege_escalation: map_bool(security, "allowPrivilegeEscalation"),
        command_count: map_seq(node, "command").len(),
        args_count: map_seq(node, "args").len(),
    }
}
