use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_IDENTITY_DEPTH: usize = 64;
const MAX_IDENTITY_VALUES: usize = 65_536;

#[derive(Debug, thiserror::Error)]
pub enum CanonicalIdentityError {
    #[error("identity domain is empty")]
    EmptyDomain,
    #[error("identity contains a floating-point number")]
    FloatingPoint,
    #[error("identity exceeds its structural limit")]
    StructuralLimit,
    #[error("identity serialization rejected a value: {0}")]
    InvalidValue(String),
    #[error("identity serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl serde::ser::Error for CanonicalIdentityError {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self::InvalidValue(message.to_string())
    }
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
enum CanonicalValue {
    Null,
    Bool(bool),
    Integer(String),
    String(String),
    Sequence(Vec<CanonicalValue>),
    Object(BTreeMap<String, CanonicalValue>),
}

pub fn canonical_identity<T: Serialize + ?Sized>(
    domain: &str,
    value: &T,
) -> Result<String, CanonicalIdentityError> {
    if domain.is_empty() {
        return Err(CanonicalIdentityError::EmptyDomain);
    }
    value.serialize(&mut super::identity_validation::IdentityValidation::default())?;
    let mut remaining = MAX_IDENTITY_VALUES;
    let value = canonical_value(serde_json::to_value(value)?, 0, &mut remaining)?;
    let payload = serde_json::to_vec(&value)?;
    let mut digest = Sha256::new();
    digest.update((domain.len() as u64).to_le_bytes());
    digest.update(domain.as_bytes());
    digest.update((payload.len() as u64).to_le_bytes());
    digest.update(payload);
    Ok(format!("{domain}-{}", hex::encode(digest.finalize())))
}

fn canonical_value(
    value: Value,
    depth: usize,
    remaining: &mut usize,
) -> Result<CanonicalValue, CanonicalIdentityError> {
    if depth > MAX_IDENTITY_DEPTH || *remaining == 0 {
        return Err(CanonicalIdentityError::StructuralLimit);
    }
    *remaining -= 1;
    Ok(match value {
        Value::Null => CanonicalValue::Null,
        Value::Bool(value) => CanonicalValue::Bool(value),
        Value::String(value) => CanonicalValue::String(value),
        Value::Number(value) => CanonicalValue::Integer(
            value
                .as_i64()
                .map(|value| value.to_string())
                .or_else(|| value.as_u64().map(|value| value.to_string()))
                .ok_or(CanonicalIdentityError::FloatingPoint)?,
        ),
        Value::Array(values) => CanonicalValue::Sequence(
            values
                .into_iter()
                .map(|value| canonical_value(value, depth + 1, remaining))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(values) => CanonicalValue::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    canonical_value(value, depth + 1, remaining).map(|value| (key, value))
                })
                .collect::<Result<_, _>>()?,
        ),
    })
}
