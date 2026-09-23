use serde_json::Value;

use super::kubernetes_parser_error::{KubernetesParserError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CniNetworkSummary {
    pub name: Option<String>,
    pub cni_version: Option<String>,
    pub plugin_types: Vec<String>,
    pub ipam_type: Option<String>,
    pub subnets: Vec<String>,
    pub routes: Vec<String>,
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
