use serde_json::Value;

use super::kubernetes_parser_error::{KubernetesParserError, Result};

const MAX_AUDIT_LINE_BYTES: usize = 16 * 1024 * 1024;
const MAX_AUDIT_EVENTS: usize = 131_072;
const MAX_AUDIT_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesAuditParseResult {
    pub events: Vec<KubernetesAuditEvent>,
    pub invalid_lines: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubernetesAuditEvent {
    pub line_number: u64,
    pub timestamp: Option<String>,
    pub stage: Option<String>,
    pub verb: Option<String>,
    pub username: Option<String>,
    pub resource: Option<String>,
    pub subresource: Option<String>,
    pub namespace: Option<String>,
    pub name: Option<String>,
    pub response_code: Option<u64>,
    pub source_ip: Option<String>,
    pub indicators: Vec<KubernetesAuditIndicator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum KubernetesAuditIndicator {
    InteractiveExec,
    SecretAccess,
    RbacChange,
    PrivilegedPod,
    AnonymousAccess,
}

pub fn parse_kubernetes_audit_log(input: &str) -> Result<KubernetesAuditParseResult> {
    if input.len() > MAX_AUDIT_BYTES {
        return Err(KubernetesParserError::Limit {
            kind: "audit bytes",
            actual: input.len(),
            max: MAX_AUDIT_BYTES,
        });
    }
    let mut events = Vec::new();
    let mut invalid_lines = 0;
    let mut truncated = false;
    for (index, line) in input.lines().enumerate() {
        let line_number = index as u64 + 1;
        if line.len() > MAX_AUDIT_LINE_BYTES {
            return Err(KubernetesParserError::Limit {
                kind: "audit line bytes",
                actual: line.len(),
                max: MAX_AUDIT_LINE_BYTES,
            });
        }
        if line.trim().is_empty() {
            continue;
        }
        let value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(source) => {
                invalid_lines += 1;
                if invalid_lines > 1024 {
                    return Err(KubernetesParserError::InvalidAuditJson {
                        line: line_number as usize,
                        source,
                    });
                }
                continue;
            }
        };
        if events.len() >= MAX_AUDIT_EVENTS {
            truncated = true;
            break;
        }
        events.push(parse_event(line_number, &value));
    }
    Ok(KubernetesAuditParseResult {
        events,
        invalid_lines,
        truncated,
    })
}

fn parse_event(line_number: u64, value: &Value) -> KubernetesAuditEvent {
    let object_ref = value.get("objectRef").unwrap_or(&Value::Null);
    let user = value.get("user").unwrap_or(&Value::Null);
    let response = value.get("responseStatus").unwrap_or(&Value::Null);
    let resource = string(object_ref, "resource");
    let subresource = string(object_ref, "subresource");
    let verb = string(value, "verb");
    let username = string(user, "username");
    let mut indicators = Vec::new();
    if matches!(subresource.as_deref(), Some("exec" | "attach"))
        || matches!(resource.as_deref(), Some("pods/exec" | "pods/attach"))
    {
        indicators.push(KubernetesAuditIndicator::InteractiveExec);
    }
    if resource.as_deref() == Some("secrets")
        && matches!(verb.as_deref(), Some("get" | "list" | "watch"))
    {
        indicators.push(KubernetesAuditIndicator::SecretAccess);
    }
    if matches!(
        resource.as_deref(),
        Some("clusterrolebindings" | "clusterroles" | "rolebindings" | "roles")
    ) && matches!(
        verb.as_deref(),
        Some("create" | "update" | "patch" | "delete")
    ) {
        indicators.push(KubernetesAuditIndicator::RbacChange);
    }
    if has_privileged_pod(value) {
        indicators.push(KubernetesAuditIndicator::PrivilegedPod);
    }
    if username.as_deref().is_some_and(|value| {
        value == "system:anonymous" || value.starts_with("system:unauthenticated")
    }) {
        indicators.push(KubernetesAuditIndicator::AnonymousAccess);
    }
    KubernetesAuditEvent {
        line_number,
        timestamp: string(value, "requestReceivedTimestamp"),
        stage: string(value, "stage"),
        verb,
        username,
        resource,
        subresource,
        namespace: string(object_ref, "namespace"),
        name: string(object_ref, "name"),
        response_code: response.get("code").and_then(Value::as_u64),
        source_ip: value
            .get("sourceIPs")
            .and_then(Value::as_array)
            .and_then(|ips| ips.first())
            .and_then(Value::as_str)
            .map(str::to_string),
        indicators,
    }
}

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn has_privileged_pod(value: &Value) -> bool {
    let Some(spec) = value
        .get("requestObject")
        .and_then(|object| object.get("spec"))
    else {
        return false;
    };
    let mut containers = spec
        .get("containers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();
    containers.any(|container| {
        container
            .get("securityContext")
            .and_then(|context| context.get("privileged"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    })
}
