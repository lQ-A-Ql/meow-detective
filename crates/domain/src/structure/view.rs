use serde::{Deserialize, Deserializer, Serialize};

use super::{
    canonical_identity, AcquisitionId, AddressSpace, ByteRange, ByteRangeError,
    CanonicalIdentityError, NativeObjectId, SourceRevisionRef,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceCaptureRef {
    pub source: SourceRevisionRef,
    pub acquisition_id: AcquisitionId,
}

impl SourceCaptureRef {
    pub fn new(
        source: SourceRevisionRef,
        acquisition_id: AcquisitionId,
    ) -> Result<Self, ViewModelError> {
        let capture = Self {
            source,
            acquisition_id,
        };
        capture.validate()?;
        Ok(capture)
    }

    pub fn validate(&self) -> Result<(), ViewModelError> {
        require_identity(&self.source.case_id, "case_id")?;
        require_identity(&self.source.source_id, "source_id")?;
        require_identity(self.acquisition_id.as_str(), "acquisition_id")?;
        if self.source.revision == 0 {
            return Err(ViewModelError::UnregisteredRevision);
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for SourceCaptureRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            source: SourceRevisionRef,
            acquisition_id: AcquisitionId,
        }
        let record = Record::deserialize(deserializer)?;
        Self::new(record.source, record.acquisition_id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackingVersionRef {
    pub capture: SourceCaptureRef,
    pub instance_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_manifest_sha256: Option<String>,
    pub change_token: String,
}

impl BackingVersionRef {
    pub fn new(
        capture: SourceCaptureRef,
        instance_id: String,
        content_manifest_sha256: Option<String>,
        change_token: String,
    ) -> Result<Self, ViewModelError> {
        let backing = Self {
            capture,
            instance_id,
            content_manifest_sha256,
            change_token,
        };
        backing.validate()?;
        Ok(backing)
    }

    pub fn validate(&self) -> Result<(), ViewModelError> {
        self.capture.validate()?;
        require_identity(&self.instance_id, "instance_id")?;
        require_identity(&self.change_token, "change_token")?;
        if let Some(digest) = &self.content_manifest_sha256 {
            require_digest(digest, "content_manifest_sha256")?;
        }
        Ok(())
    }

    pub fn identity(&self) -> Result<String, ViewModelError> {
        self.validate()?;
        Ok(canonical_identity("backing-version-v1", self)?)
    }
}

impl<'de> Deserialize<'de> for BackingVersionRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            capture: SourceCaptureRef,
            instance_id: String,
            content_manifest_sha256: Option<String>,
            change_token: String,
        }
        let record = Record::deserialize(deserializer)?;
        Self::new(
            record.capture,
            record.instance_id,
            record.content_manifest_sha256,
            record.change_token,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeViewVersionRef {
    pub id: String,
    pub adapter_id: String,
    pub adapter_version: String,
    pub native_object_id: NativeObjectId,
    pub parents: Vec<SourceCaptureRef>,
    pub parent_view_ids: Vec<String>,
    pub selection_sha256: String,
    pub mapping_sha256: String,
    pub policy_version: String,
}

impl NativeViewVersionRef {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        adapter_id: String,
        adapter_version: String,
        native_object_id: NativeObjectId,
        mut parents: Vec<SourceCaptureRef>,
        mut parent_view_ids: Vec<String>,
        selection_sha256: String,
        mapping_sha256: String,
        policy_version: String,
    ) -> Result<Self, ViewModelError> {
        parents.sort_by(|left, right| capture_key(left).cmp(&capture_key(right)));
        parents.dedup();
        parent_view_ids.sort_unstable();
        parent_view_ids.dedup();
        let mut view = Self {
            id: String::new(),
            adapter_id,
            adapter_version,
            native_object_id,
            parents,
            parent_view_ids,
            selection_sha256,
            mapping_sha256,
            policy_version,
        };
        view.validate_inputs()?;
        view.id = view.canonical_id()?;
        Ok(view)
    }

    pub fn validate(&self) -> Result<(), ViewModelError> {
        self.validate_inputs()?;
        if self.parent_view_ids.contains(&self.id) {
            return Err(ViewModelError::SelfParent);
        }
        if self.id != self.canonical_id()? {
            return Err(ViewModelError::IdentityMismatch);
        }
        Ok(())
    }

    fn validate_inputs(&self) -> Result<(), ViewModelError> {
        for (field, value) in [
            ("adapter_id", self.adapter_id.as_str()),
            ("adapter_version", self.adapter_version.as_str()),
            ("policy_version", self.policy_version.as_str()),
            ("native_adapter", self.native_object_id.adapter.as_str()),
            (
                "identity_space",
                self.native_object_id.identity_space.as_str(),
            ),
            ("native_id", self.native_object_id.native_id.as_str()),
            ("scope", self.native_object_id.scope.as_str()),
        ] {
            require_identity(value, field)?;
        }
        if self.native_object_id.adapter != self.adapter_id {
            return Err(ViewModelError::AdapterMismatch);
        }
        require_digest(&self.selection_sha256, "selection_sha256")?;
        require_digest(&self.mapping_sha256, "mapping_sha256")?;
        if self.parents.is_empty() && self.parent_view_ids.is_empty() {
            return Err(ViewModelError::MissingParents);
        }
        for parent in &self.parents {
            parent.validate()?;
            if self.parents[0].source.case_id != parent.source.case_id {
                return Err(ViewModelError::CrossCaseParents);
            }
        }
        for parent in &self.parent_view_ids {
            require_identity(parent, "parent_view_id")?;
        }
        if self
            .parents
            .windows(2)
            .any(|pair| capture_key(&pair[0]) >= capture_key(&pair[1]))
            || self
                .parent_view_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            return Err(ViewModelError::NonCanonicalParents);
        }
        Ok(())
    }

    fn canonical_id(&self) -> Result<String, ViewModelError> {
        let identity = ViewIdentity {
            adapter_id: &self.adapter_id,
            adapter_version: &self.adapter_version,
            native_object_id: &self.native_object_id,
            parents: &self.parents,
            parent_view_ids: &self.parent_view_ids,
            selection_sha256: &self.selection_sha256,
            mapping_sha256: &self.mapping_sha256,
            policy_version: &self.policy_version,
        };
        Ok(canonical_identity("native-view-v1", &identity)?)
    }
}

impl<'de> Deserialize<'de> for NativeViewVersionRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            id: String,
            adapter_id: String,
            adapter_version: String,
            native_object_id: NativeObjectId,
            parents: Vec<SourceCaptureRef>,
            parent_view_ids: Vec<String>,
            selection_sha256: String,
            mapping_sha256: String,
            policy_version: String,
        }
        let record = Record::deserialize(deserializer)?;
        let view = Self {
            id: record.id,
            adapter_id: record.adapter_id,
            adapter_version: record.adapter_version,
            native_object_id: record.native_object_id,
            parents: record.parents,
            parent_view_ids: record.parent_view_ids,
            selection_sha256: record.selection_sha256,
            mapping_sha256: record.mapping_sha256,
            policy_version: record.policy_version,
        };
        view.validate().map_err(serde::de::Error::custom)?;
        Ok(view)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewIdentity<'a> {
    adapter_id: &'a str,
    adapter_version: &'a str,
    native_object_id: &'a NativeObjectId,
    parents: &'a [SourceCaptureRef],
    parent_view_ids: &'a [String],
    selection_sha256: &'a str,
    mapping_sha256: &'a str,
    policy_version: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerLocator {
    pub view_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_view_id: Option<String>,
    pub address_space: AddressSpace,
    pub native_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<ByteRange>,
    pub parser_version: String,
}

impl LayerLocator {
    pub fn validate(&self) -> Result<(), ViewModelError> {
        require_identity(&self.view_id, "view_id")?;
        require_identity(&self.parser_version, "parser_version")?;
        if self.native_id.is_empty() {
            return Err(ViewModelError::EmptyField { field: "native_id" });
        }
        if let Some(parent) = &self.parent_view_id {
            require_identity(parent, "parent_view_id")?;
            if parent == &self.view_id {
                return Err(ViewModelError::SelfParent);
            }
        }
        if let Some(range) = self.range {
            ByteRange::new(range.start(), range.length())?;
            if !matches!(
                self.address_space,
                AddressSpace::PhysicalByte
                    | AddressSpace::LogicalBlock
                    | AddressSpace::File
                    | AddressSpace::Object
            ) {
                return Err(ViewModelError::NonByteRange);
            }
        }
        Ok(())
    }

    pub fn identity(&self) -> Result<String, ViewModelError> {
        self.validate()?;
        Ok(canonical_identity("layer-locator-v1", self)?)
    }
}

impl<'de> Deserialize<'de> for LayerLocator {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            view_id: String,
            parent_view_id: Option<String>,
            address_space: AddressSpace,
            native_id: String,
            range: Option<ByteRange>,
            parser_version: String,
        }
        let record = Record::deserialize(deserializer)?;
        let locator = Self {
            view_id: record.view_id,
            parent_view_id: record.parent_view_id,
            address_space: record.address_space,
            native_id: record.native_id,
            range: record.range,
            parser_version: record.parser_version,
        };
        locator.validate().map_err(serde::de::Error::custom)?;
        Ok(locator)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ViewModelError {
    #[error("view identity field '{field}' is empty")]
    EmptyField { field: &'static str },
    #[error("source revision must refer to a registered revision")]
    UnregisteredRevision,
    #[error("view digest '{field}' must be lowercase SHA-256")]
    InvalidDigest { field: &'static str },
    #[error("native view adapter differs from its object adapter")]
    AdapterMismatch,
    #[error("native view has no parent inputs")]
    MissingParents,
    #[error("native view parents belong to different cases")]
    CrossCaseParents,
    #[error("native view parent sets are not sorted and deduplicated")]
    NonCanonicalParents,
    #[error("native view identity does not match its immutable inputs")]
    IdentityMismatch,
    #[error("a view cannot be its own parent")]
    SelfParent,
    #[error("a byte range cannot select this address space")]
    NonByteRange,
    #[error(transparent)]
    Range(#[from] ByteRangeError),
    #[error(transparent)]
    Identity(#[from] CanonicalIdentityError),
}

fn capture_key(capture: &SourceCaptureRef) -> (&str, &str, u64, &str) {
    (
        &capture.source.case_id,
        &capture.source.source_id,
        capture.source.revision,
        capture.acquisition_id.as_str(),
    )
}

fn require_identity(value: &str, field: &'static str) -> Result<(), ViewModelError> {
    if value.trim().is_empty() {
        return Err(ViewModelError::EmptyField { field });
    }
    Ok(())
}

fn require_digest(value: &str, field: &'static str) -> Result<(), ViewModelError> {
    if !valid_sha256(value) {
        return Err(ViewModelError::InvalidDigest { field });
    }
    Ok(())
}

pub(super) fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
