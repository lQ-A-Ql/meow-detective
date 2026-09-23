use serde_json::Value;

use super::kubernetes_parser_error::{KubernetesParserError, Result};
use super::kubernetes_yaml::{map_get, map_seq, map_string, parse_yaml_documents, YamlNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CniNetworkSummary {
    pub name: Option<String>,
    pub cni_version: Option<String>,
    pub plugin_types: Vec<String>,
    pub ipam_type: Option<String>,
    pub subnets: Vec<String>,
    pub routes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KubernetesNetworkResource {
    Node(KubernetesNodeNetworkSummary),
    Service(KubernetesServiceNetworkSummary),
    EndpointSlice(KubernetesEndpointSliceSummary),
    Ingress(KubernetesIngressSummary),
    NetworkPolicy(KubernetesNetworkPolicySummary),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesNodeNetworkSummary {
    pub name: Option<String>,
    pub addresses: Vec<(String, String)>,
    pub pod_cidr: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesServiceNetworkSummary {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub service_type: Option<String>,
    pub cluster_ip: Option<String>,
    pub ports: Vec<String>,
    pub selector_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesEndpointSliceSummary {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub service_name: Option<String>,
    pub addresses: Vec<String>,
    pub node_names: Vec<String>,
    pub ports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesIngressSummary {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub hosts: Vec<String>,
    pub backend_services: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesNetworkPolicySummary {
    pub name: Option<String>,
    pub namespace: Option<String>,
    pub policy_types: Vec<String>,
    pub selector_keys: Vec<String>,
}

pub fn parse_kubernetes_network_resources(input: &str) -> Result<Vec<KubernetesNetworkResource>> {
    parse_yaml_documents(input)?
        .into_iter()
        .filter_map(parse_network_resource)
        .collect::<Result<Vec<_>>>()
}

fn parse_network_resource(node: YamlNode) -> Option<Result<KubernetesNetworkResource>> {
    let kind = map_string(&node, "kind")?;
    let metadata = map_get(&node, "metadata").unwrap_or(&YamlNode::Null);
    let name = map_string(metadata, "name");
    let namespace = map_string(metadata, "namespace");
    let result = match kind.as_str() {
        "Node" => KubernetesNetworkResource::Node(parse_node(&node, name)),
        "Service" => KubernetesNetworkResource::Service(parse_service(&node, name, namespace)),
        "EndpointSlice" => KubernetesNetworkResource::EndpointSlice(parse_endpoint_slice(
            &node, name, namespace, metadata,
        )),
        "Ingress" => KubernetesNetworkResource::Ingress(parse_ingress(&node, name, namespace)),
        "NetworkPolicy" => {
            KubernetesNetworkResource::NetworkPolicy(parse_network_policy(&node, name, namespace))
        }
        _ => return None,
    };
    Some(Ok(result))
}

fn parse_node(node: &YamlNode, name: Option<String>) -> KubernetesNodeNetworkSummary {
    let status = map_get(node, "status").unwrap_or(&YamlNode::Null);
    let addresses = map_seq(status, "addresses")
        .iter()
        .filter_map(|value| Some((map_string(value, "type")?, map_string(value, "address")?)))
        .collect();
    KubernetesNodeNetworkSummary {
        name,
        addresses,
        pod_cidr: map_string(status, "podCIDR"),
    }
}

fn parse_service(
    node: &YamlNode,
    name: Option<String>,
    namespace: Option<String>,
) -> KubernetesServiceNetworkSummary {
    let spec = map_get(node, "spec").unwrap_or(&YamlNode::Null);
    let ports = map_seq(spec, "ports")
        .iter()
        .filter_map(|port| {
            let port_number = map_string(port, "port")?;
            let protocol = map_string(port, "protocol").unwrap_or_else(|| "TCP".to_string());
            Some(format!("{protocol}/{port_number}"))
        })
        .collect();
    let selector_keys = map_get(spec, "selector")
        .and_then(map_keys)
        .unwrap_or_default();
    KubernetesServiceNetworkSummary {
        name,
        namespace,
        service_type: map_string(spec, "type"),
        cluster_ip: map_string(spec, "clusterIP"),
        ports,
        selector_keys,
    }
}

fn parse_endpoint_slice(
    node: &YamlNode,
    name: Option<String>,
    namespace: Option<String>,
    metadata: &YamlNode,
) -> KubernetesEndpointSliceSummary {
    let service_name = map_get(metadata, "labels")
        .and_then(|labels| map_string(labels, "kubernetes.io/service-name"));
    let addresses = map_seq(node, "endpoints")
        .iter()
        .flat_map(|endpoint| map_seq(endpoint, "addresses").iter().filter_map(scalar))
        .collect();
    let node_names = map_seq(node, "endpoints")
        .iter()
        .filter_map(|endpoint| map_string(endpoint, "nodeName"))
        .collect();
    let ports = map_seq(node, "ports")
        .iter()
        .filter_map(|port| {
            let value = map_string(port, "port")?;
            Some(format!(
                "{}/{}",
                map_string(port, "protocol").unwrap_or_else(|| "TCP".to_string()),
                value
            ))
        })
        .collect();
    KubernetesEndpointSliceSummary {
        name,
        namespace,
        service_name,
        addresses,
        node_names,
        ports,
    }
}

fn parse_ingress(
    node: &YamlNode,
    name: Option<String>,
    namespace: Option<String>,
) -> KubernetesIngressSummary {
    let spec = map_get(node, "spec").unwrap_or(&YamlNode::Null);
    let mut hosts = Vec::new();
    let mut backend_services = Vec::new();
    for rule in map_seq(spec, "rules") {
        if let Some(host) = map_string(rule, "host") {
            hosts.push(host);
        }
        let http = map_get(rule, "http").unwrap_or(&YamlNode::Null);
        for path in map_seq(http, "paths") {
            let backend = map_get(path, "backend").unwrap_or(&YamlNode::Null);
            if let Some(service) =
                map_get(backend, "service").and_then(|service| map_string(service, "name"))
            {
                backend_services.push(service);
            }
        }
    }
    KubernetesIngressSummary {
        name,
        namespace,
        hosts,
        backend_services,
    }
}

fn parse_network_policy(
    node: &YamlNode,
    name: Option<String>,
    namespace: Option<String>,
) -> KubernetesNetworkPolicySummary {
    let spec = map_get(node, "spec").unwrap_or(&YamlNode::Null);
    let policy_types = map_seq(spec, "policyTypes")
        .iter()
        .filter_map(scalar)
        .collect();
    let selector_keys = map_get(spec, "podSelector")
        .and_then(map_keys)
        .unwrap_or_default();
    KubernetesNetworkPolicySummary {
        name,
        namespace,
        policy_types,
        selector_keys,
    }
}

fn map_keys(node: &YamlNode) -> Option<Vec<String>> {
    match node {
        YamlNode::Map(values) => Some(values.keys().cloned().collect()),
        _ => None,
    }
}

fn scalar(node: &YamlNode) -> Option<String> {
    match node {
        YamlNode::Scalar(value) => Some(value.clone()),
        _ => None,
    }
}

pub fn parse_cni_config(input: &[u8]) -> Result<CniNetworkSummary> {
    let value: Value =
        serde_json::from_slice(input).map_err(|error| KubernetesParserError::InvalidBinary {
            offset: 0,
            reason: format!("invalid CNI JSON: {error}"),
        })?;
    let object = value
        .as_object()
        .ok_or_else(|| KubernetesParserError::InvalidBinary {
            offset: 0,
            reason: "CNI configuration must be a JSON object".to_string(),
        })?;
    let name = string(object.get("name"));
    let cni_version = string(object.get("cniVersion"));
    let mut plugin_types = Vec::new();
    let mut ipam_type = None;
    let mut subnets = Vec::new();
    let mut routes = Vec::new();
    if let Some(plugins) = object.get("plugins").and_then(Value::as_array) {
        for plugin in plugins {
            collect_plugin(
                plugin,
                &mut plugin_types,
                &mut ipam_type,
                &mut subnets,
                &mut routes,
            );
        }
    } else {
        collect_plugin(
            &value,
            &mut plugin_types,
            &mut ipam_type,
            &mut subnets,
            &mut routes,
        );
    }
    plugin_types.sort();
    plugin_types.dedup();
    subnets.sort();
    subnets.dedup();
    routes.sort();
    routes.dedup();
    Ok(CniNetworkSummary {
        name,
        cni_version,
        plugin_types,
        ipam_type,
        subnets,
        routes,
    })
}

fn collect_plugin(
    value: &Value,
    plugin_types: &mut Vec<String>,
    ipam_type: &mut Option<String>,
    subnets: &mut Vec<String>,
    routes: &mut Vec<String>,
) {
    let Some(object) = value.as_object() else {
        return;
    };
    if let Some(plugin_type) = string(object.get("type")) {
        plugin_types.push(plugin_type);
    }
    if let Some(ipam) = object.get("ipam").and_then(Value::as_object) {
        if ipam_type.is_none() {
            *ipam_type = string(ipam.get("type"));
        }
        collect_ipam(ipam, subnets, routes);
    }
    collect_ipam(object, subnets, routes);
}

fn collect_ipam(
    object: &serde_json::Map<String, Value>,
    subnets: &mut Vec<String>,
    routes: &mut Vec<String>,
) {
    for key in ["subnet", "range", "rangeStart", "rangeEnd"] {
        if let Some(value) = string(object.get(key)) {
            subnets.push(value);
        }
    }
    if let Some(route_values) = object.get("routes").and_then(Value::as_array) {
        for route in route_values {
            if let Some(dst) = route.get("dst").and_then(string_value) {
                routes.push(dst);
            }
        }
    }
}

fn string(value: Option<&Value>) -> Option<String> {
    value.and_then(string_value)
}

fn string_value(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

#[cfg(test)]
#[path = "../../tests/unit/cluster_service/network_parser.rs"]
mod tests;
