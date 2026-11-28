use serde::{Deserialize, Deserializer, Serialize};

use super::{canonical_identity, view::valid_sha256, CanonicalIdentityError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VersionSelection {
    Current,
    Exact {
        #[serde(rename = "viewId")]
        view_id: String,
    },
    Historical {
        boundary: VersionBoundary,
    },
}

impl VersionSelection {
    pub fn validate(&self) -> Result<(), VersionSelectionError> {
        match self {
            Self::Current => Ok(()),
            Self::Exact { view_id } => require_value(view_id, "view_id"),
            Self::Historical { boundary } => boundary.validate(),
        }
    }

    pub fn identity(&self) -> Result<String, VersionSelectionError> {
        self.validate()?;
        Ok(canonical_identity("version-selection-v1", self)?)
    }
}

impl<'de> Deserialize<'de> for VersionSelection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Record {
            Current {},
            Exact {
                #[serde(rename = "viewId")]
                view_id: String,
            },
            Historical {
                boundary: VersionBoundary,
            },
        }
        let selection = match Record::deserialize(deserializer)? {
            Record::Current {} => Self::Current,
            Record::Exact { view_id } => Self::Exact { view_id },
            Record::Historical { boundary } => Self::Historical { boundary },
        };
        selection.validate().map_err(serde::de::Error::custom)?;
        Ok(selection)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionBoundary {
    pub adapter_id: String,
    pub native_kind: String,
    pub native_value: String,
    pub witness_sha256: String,
}

impl VersionBoundary {
    pub fn new(
        adapter_id: String,
        native_kind: String,
        native_value: String,
        witness_sha256: String,
    ) -> Result<Self, VersionSelectionError> {
        let boundary = Self {
            adapter_id,
            native_kind,
            native_value,
            witness_sha256,
        };
        boundary.validate()?;
        Ok(boundary)
    }

    pub fn validate(&self) -> Result<(), VersionSelectionError> {
        require_value(&self.adapter_id, "adapter_id")?;
        require_value(&self.native_kind, "native_kind")?;
        require_value(&self.native_value, "native_value")?;
        if !valid_sha256(&self.witness_sha256) {
            return Err(VersionSelectionError::InvalidWitness);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for VersionBoundary {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            adapter_id: String,
            native_kind: String,
            native_value: String,
            witness_sha256: String,
        }
        let record = Record::deserialize(deserializer)?;
        Self::new(
            record.adapter_id,
            record.native_kind,
            record.native_value,
            record.witness_sha256,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VersionSelectionError {
    #[error("version selection field '{field}' is empty")]
    EmptyField { field: &'static str },
    #[error("version selection witness must be lowercase SHA-256")]
    InvalidWitness,
    #[error(transparent)]
    Identity(#[from] CanonicalIdentityError),
}

fn require_value(value: &str, field: &'static str) -> Result<(), VersionSelectionError> {
    if value.trim().is_empty() {
        return Err(VersionSelectionError::EmptyField { field });
    }
    Ok(())
}
