use super::kubernetes_parser_error::Result;
use super::kubernetes_yaml::{map_get, map_seq, map_string, parse_yaml_documents, YamlNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubeconfigSummary {
    pub api_version: Option<String>,
    pub current_context: Option<String>,
    pub clusters: Vec<KubeconfigCluster>,
    pub contexts: Vec<KubeconfigContext>,
    pub users: Vec<KubeconfigUser>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubeconfigCluster {
    pub name: Option<String>,
    pub server: Option<String>,
    pub certificate_authority_path: Option<String>,
    pub certificate_authority_data_present: bool,
    pub insecure_skip_tls_verify: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubeconfigContext {
    pub name: Option<String>,
    pub cluster: Option<String>,
    pub user: Option<String>,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KubeconfigUser {
    pub name: Option<String>,
    pub client_certificate_path: Option<String>,
    pub client_key_path: Option<String>,
    pub client_certificate_data_present: bool,
    pub client_key_data_present: bool,
    pub token_present: bool,
    pub auth_provider: Option<String>,
}

pub fn parse_kubeconfig(input: &str) -> Result<KubeconfigSummary> {
    let document = parse_yaml_documents(input)?
        .into_iter()
        .next()
        .unwrap_or(YamlNode::Null);
    Ok(KubeconfigSummary {
        api_version: map_string(&document, "apiVersion"),
        current_context: map_string(&document, "current-context"),
        clusters: map_seq(&document, "clusters")
            .iter()
            .map(parse_cluster)
            .collect(),
        contexts: map_seq(&document, "contexts")
            .iter()
            .map(parse_context)
            .collect(),
        users: map_seq(&document, "users").iter().map(parse_user).collect(),
    })
}

fn parse_cluster(node: &YamlNode) -> KubeconfigCluster {
    let value = map_get(node, "cluster").unwrap_or(&YamlNode::Null);
    KubeconfigCluster {
        name: map_string(node, "name"),
        server: map_string(value, "server"),
        certificate_authority_path: map_string(value, "certificate-authority"),
        certificate_authority_data_present: map_string(value, "certificate-authority-data")
            .is_some_and(|data| !data.is_empty()),
        insecure_skip_tls_verify: map_string(value, "insecure-skip-tls-verify")
            .is_some_and(|value| value.eq_ignore_ascii_case("true")),
    }
}

fn parse_context(node: &YamlNode) -> KubeconfigContext {
    let value = map_get(node, "context").unwrap_or(&YamlNode::Null);
    KubeconfigContext {
        name: map_string(node, "name"),
        cluster: map_string(value, "cluster"),
        user: map_string(value, "user"),
        namespace: map_string(value, "namespace"),
    }
}

fn parse_user(node: &YamlNode) -> KubeconfigUser {
    let value = map_get(node, "user").unwrap_or(&YamlNode::Null);
    let auth_provider =
        map_get(value, "auth-provider").and_then(|provider| map_string(provider, "name"));
    KubeconfigUser {
        name: map_string(node, "name"),
        client_certificate_path: map_string(value, "client-certificate"),
        client_key_path: map_string(value, "client-key"),
        client_certificate_data_present: map_string(value, "client-certificate-data")
            .is_some_and(|data| !data.is_empty()),
        client_key_data_present: map_string(value, "client-key-data")
            .is_some_and(|data| !data.is_empty()),
        token_present: map_string(value, "token").is_some_and(|token| !token.is_empty()),
        auth_provider,
    }
}
