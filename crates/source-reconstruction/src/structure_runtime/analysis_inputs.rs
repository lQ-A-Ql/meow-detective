use std::collections::{BTreeMap, BTreeSet};

use domain::structure::{CapabilityIntegrity, CapabilityKind};

use super::frontier::{BindingPrerequisite, CapabilityIndex, FrontierError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimumIntegrity {
    Observed,
    NativeVerified,
    ContentVerified,
}

impl MinimumIntegrity {
    fn accepts(self, integrity: &CapabilityIntegrity) -> bool {
        match self {
            Self::Observed => true,
            Self::NativeVerified => matches!(
                integrity,
                CapabilityIntegrity::NativeVerified { .. }
                    | CapabilityIntegrity::ContentVerified { .. }
            ),
            Self::ContentVerified => {
                matches!(integrity, CapabilityIntegrity::ContentVerified { .. })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisInputRequirement {
    pub kind: CapabilityKind,
    pub native_role: String,
    pub minimum_integrity: MinimumIntegrity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisInputMatch {
    pub capability_ids: BTreeSet<String>,
    pub missing: BTreeSet<BindingPrerequisite>,
}

pub fn match_inputs(
    scope_id: &str,
    requirements: &[AnalysisInputRequirement],
    capabilities: &CapabilityIndex,
) -> Result<AnalysisInputMatch, AnalysisInputError> {
    validate_requirements(scope_id, requirements)?;
    let mut matched = AnalysisInputMatch {
        capability_ids: BTreeSet::new(),
        missing: BTreeSet::new(),
    };
    for requirement in requirements {
        let prerequisite = BindingPrerequisite {
            scope_id: scope_id.into(),
            kind: requirement.kind,
            native_role: requirement.native_role.clone(),
        };
        let entries = capabilities.matching(&prerequisite)?;
        let mut found = false;
        for entry in entries {
            if requirement
                .minimum_integrity
                .accepts(&entry.capability.integrity)
            {
                matched.capability_ids.insert(entry.capability.id.clone());
                found = true;
            }
        }
        if !found {
            matched.missing.insert(prerequisite);
        }
    }
    Ok(matched)
}

fn validate_requirements(
    scope_id: &str,
    requirements: &[AnalysisInputRequirement],
) -> Result<(), AnalysisInputError> {
    if scope_id.trim().is_empty() {
        return Err(AnalysisInputError::EmptyScope);
    }
    if requirements.is_empty() {
        return Err(AnalysisInputError::EmptyRequirements);
    }
    let mut seen = BTreeMap::new();
    for requirement in requirements {
        if requirement.native_role.trim().is_empty() {
            return Err(AnalysisInputError::EmptyRole);
        }
        let key = (requirement.kind, requirement.native_role.as_str());
        if let Some(previous) = seen.insert(key, requirement.minimum_integrity) {
            return Err(if previous == requirement.minimum_integrity {
                AnalysisInputError::DuplicateRequirement {
                    kind: requirement.kind,
                    native_role: requirement.native_role.clone(),
                }
            } else {
                AnalysisInputError::ConflictingRequirement {
                    kind: requirement.kind,
                    native_role: requirement.native_role.clone(),
                }
            });
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum AnalysisInputError {
    #[error("analysis input scope is empty")]
    EmptyScope,
    #[error("analysis input requirements are empty")]
    EmptyRequirements,
    #[error("analysis input native role is empty")]
    EmptyRole,
    #[error("analysis input requirement for {kind:?} and role '{native_role}' is duplicated")]
    DuplicateRequirement {
        kind: CapabilityKind,
        native_role: String,
    },
    #[error("analysis input requirement for {kind:?} and role '{native_role}' has conflicting integrity levels")]
    ConflictingRequirement {
        kind: CapabilityKind,
        native_role: String,
    },
    #[error(transparent)]
    Index(#[from] FrontierError),
}
