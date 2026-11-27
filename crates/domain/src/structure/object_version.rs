use serde::{Deserialize, Serialize};

use super::{
    identity::identity_digest, AcquisitionId, NativeObjectId, NativeVersion, RevisionConsistency,
    SourceRevisionRef, StructureIdentityError,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectVersionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageObjectVersion {
    pub id: ObjectVersionId,
    pub native_object_id: NativeObjectId,
    pub acquisition_ids: Vec<AcquisitionId>,
    pub storage_object_id: String,
    pub owner_source_id: String,
    pub derived_source_id: String,
    pub adapter_version: String,
    pub policy_version: String,
    pub mapping_sha256: String,
    pub parent_source_revisions: Vec<SourceRevisionRef>,
    pub native_versions: Vec<NativeVersion>,
    pub consistency: RevisionConsistency,
    pub version_kind: StorageObjectVersionKind,
}

impl StorageObjectVersion {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        native_object_id: NativeObjectId,
        acquisition_ids: Vec<AcquisitionId>,
        owner_source_id: String,
        derived_source_id: String,
        adapter_version: String,
        policy_version: String,
        mapping_sha256: String,
        parent_source_revisions: Vec<SourceRevisionRef>,
        native_versions: Vec<NativeVersion>,
        consistency: RevisionConsistency,
        version_kind: StorageObjectVersionKind,
    ) -> Result<Self, StructureIdentityError> {
        if owner_source_id.is_empty()
            || derived_source_id.is_empty()
            || adapter_version.is_empty()
            || policy_version.is_empty()
            || acquisition_ids.is_empty()
            || native_versions.is_empty()
            || !valid_sha256(&mapping_sha256)
        {
            return Err(StructureIdentityError::EmptyField {
                field: "storage_object_version",
            });
        }
        let id = ObjectVersionId(object_version_id(
            &native_object_id,
            &acquisition_ids,
            &owner_source_id,
            &derived_source_id,
            &adapter_version,
            &policy_version,
            &mapping_sha256,
            &parent_source_revisions,
            &native_versions,
            version_kind,
        )?);
        let storage_object_id = native_object_id.storage_object_id();
        Ok(Self {
            id,
            native_object_id,
            acquisition_ids,
            storage_object_id,
            owner_source_id,
            derived_source_id,
            adapter_version,
            policy_version,
            mapping_sha256,
            parent_source_revisions,
            native_versions,
            consistency,
            version_kind,
        })
    }

    pub fn validate(&self) -> Result<(), StructureIdentityError> {
        if self.id.0.is_empty()
            || self.storage_object_id != self.native_object_id.storage_object_id()
            || self.owner_source_id.is_empty()
            || self.derived_source_id.is_empty()
            || self.adapter_version.is_empty()
            || self.policy_version.is_empty()
            || self.acquisition_ids.is_empty()
            || self.native_versions.is_empty()
            || !valid_sha256(&self.mapping_sha256)
        {
            return Err(StructureIdentityError::EmptyField {
                field: "storage_object_version",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageObjectVersionKind {
    DerivedOutput,
    NativeObservation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructureViewAttempt {
    pub derived_source_id: String,
    pub input_fingerprint: String,
    pub attempt_id: String,
    pub owner_id: String,
    pub generation: String,
    pub receipt: String,
}

impl StorageObjectVersionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DerivedOutput => "derived_output",
            Self::NativeObservation => "native_observation",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "derived_output" => Some(Self::DerivedOutput),
            "native_observation" => Some(Self::NativeObservation),
            _ => None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn object_version_id(
    native_object_id: &NativeObjectId,
    acquisition_ids: &[AcquisitionId],
    owner_source_id: &str,
    derived_source_id: &str,
    adapter_version: &str,
    policy_version: &str,
    mapping_sha256: &str,
    parent_source_revisions: &[SourceRevisionRef],
    native_versions: &[NativeVersion],
    version_kind: StorageObjectVersionKind,
) -> Result<String, StructureIdentityError> {
    let object_id = native_object_id.storage_object_id();
    let mut acquisitions = acquisition_ids
        .iter()
        .map(AcquisitionId::as_str)
        .collect::<Vec<_>>();
    acquisitions.sort_unstable();
    let acquisitions = acquisitions.join("\n");
    let mut parent_source_revisions = parent_source_revisions.iter().collect::<Vec<_>>();
    parent_source_revisions.sort_by(|left, right| {
        (&left.case_id, &left.source_id, left.revision).cmp(&(
            &right.case_id,
            &right.source_id,
            right.revision,
        ))
    });
    let parents = parent_source_revisions
        .iter()
        .map(|parent| {
            format!(
                "{}\n{}\n{}",
                parent.case_id, parent.source_id, parent.revision
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let versions = serde_json::to_string(native_versions)
        .map_err(|_| StructureIdentityError::InvalidDigest)?;
    Ok(identity_digest(
        b"object-version-v1",
        [
            object_id.as_str(),
            acquisitions.as_str(),
            owner_source_id,
            derived_source_id,
            adapter_version,
            policy_version,
            mapping_sha256,
            parents.as_str(),
            versions.as_str(),
            version_kind.as_str(),
        ],
    ))
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
