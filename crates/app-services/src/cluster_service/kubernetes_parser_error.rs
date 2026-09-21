use thiserror::Error;

#[derive(Debug, Error)]
pub enum KubernetesParserError {
    #[error("Kubernetes parser limit exceeded for {kind}: {actual} > {max}")]
    Limit {
        kind: &'static str,
        actual: usize,
        max: usize,
    },
    #[error("invalid Kubernetes YAML at line {line}: {reason}")]
    InvalidYaml { line: usize, reason: String },
    #[error("invalid Kubernetes audit JSON at line {line}: {source}")]
    InvalidAuditJson {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("Kubernetes text artifact is not valid UTF-8")]
    InvalidText,
    #[error("invalid Kubernetes binary structure at offset {offset}: {reason}")]
    InvalidBinary { offset: u64, reason: String },
    #[error("truncated Kubernetes binary structure at offset {offset}: {context}")]
    Truncated { offset: u64, context: &'static str },
}

pub type Result<T> = std::result::Result<T, KubernetesParserError>;
