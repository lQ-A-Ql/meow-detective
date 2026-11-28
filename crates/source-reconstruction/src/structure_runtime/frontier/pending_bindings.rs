use std::collections::{BTreeMap, BTreeSet};

use super::{
    require_capacity, require_limit, require_text, BindingPrerequisite, CapabilityEntry,
    FrontierError, FrontierResource,
};

#[derive(Debug)]
pub struct PendingBindings {
    candidate_limit: usize,
    prerequisite_limit: usize,
    prerequisite_count: usize,
    candidates: BTreeMap<String, BTreeSet<BindingPrerequisite>>,
    by_prerequisite: BTreeMap<BindingPrerequisite, BTreeSet<String>>,
}

impl PendingBindings {
    /// Applies the same bound to candidates and prerequisite associations.
    pub fn new(limit: usize) -> Result<Self, FrontierError> {
        Self::with_limits(limit, limit)
    }

    pub fn with_limits(
        candidate_limit: usize,
        prerequisite_limit: usize,
    ) -> Result<Self, FrontierError> {
        require_limit(candidate_limit, FrontierResource::PendingCandidates)?;
        require_limit(prerequisite_limit, FrontierResource::PendingPrerequisites)?;
        Ok(Self {
            candidate_limit,
            prerequisite_limit,
            prerequisite_count: 0,
            candidates: BTreeMap::new(),
            by_prerequisite: BTreeMap::new(),
        })
    }

    pub fn register(
        &mut self,
        candidate_id: String,
        prerequisites: BTreeSet<BindingPrerequisite>,
    ) -> Result<bool, FrontierError> {
        validate_candidate(&candidate_id, &prerequisites)?;
        if let Some(existing) = self.candidates.get(&candidate_id) {
            return if existing == &prerequisites {
                Ok(false)
            } else {
                Err(FrontierError::CandidateConflict)
            };
        }
        require_capacity(
            self.candidates.len(),
            1,
            self.candidate_limit,
            FrontierResource::PendingCandidates,
        )?;
        self.validate_prerequisite_capacity(self.prerequisite_count, prerequisites.len())?;
        self.insert_candidate(candidate_id, prerequisites);
        Ok(true)
    }

    pub fn replace(
        &mut self,
        candidate_id: &str,
        prerequisites: BTreeSet<BindingPrerequisite>,
    ) -> Result<bool, FrontierError> {
        validate_candidate(candidate_id, &prerequisites)?;
        let existing = self
            .candidates
            .get(candidate_id)
            .ok_or(FrontierError::MissingCandidate)?;
        if existing == &prerequisites {
            return Ok(false);
        }
        self.validate_prerequisite_capacity(
            self.prerequisite_count - existing.len(),
            prerequisites.len(),
        )?;
        self.remove(candidate_id)?;
        self.insert_candidate(candidate_id.to_owned(), prerequisites);
        Ok(true)
    }

    /// Returns affected candidates without claiming that all prerequisites
    /// are satisfied. The native binder decides when to replace or remove them.
    pub fn on_publish(&self, entry: &CapabilityEntry) -> Result<BTreeSet<String>, FrontierError> {
        entry.validate()?;
        let mut affected = BTreeSet::new();
        for prerequisite in entry.prerequisites() {
            if let Some(candidates) = self.by_prerequisite.get(&prerequisite) {
                affected.extend(candidates.iter().cloned());
            }
        }
        Ok(affected)
    }

    pub fn get(
        &self,
        candidate_id: &str,
    ) -> Result<Option<&BTreeSet<BindingPrerequisite>>, FrontierError> {
        require_text(candidate_id, "candidate_id")?;
        Ok(self.candidates.get(candidate_id))
    }

    pub fn remove(
        &mut self,
        candidate_id: &str,
    ) -> Result<Option<BTreeSet<BindingPrerequisite>>, FrontierError> {
        require_text(candidate_id, "candidate_id")?;
        let Some(prerequisites) = self.candidates.remove(candidate_id) else {
            return Ok(None);
        };
        for prerequisite in &prerequisites {
            let candidates = self
                .by_prerequisite
                .get_mut(prerequisite)
                .expect("pending prerequisite index is absent");
            assert!(
                candidates.remove(candidate_id),
                "pending prerequisite index references an absent candidate"
            );
            if candidates.is_empty() {
                self.by_prerequisite.remove(prerequisite);
            }
        }
        self.prerequisite_count -= prerequisites.len();
        Ok(Some(prerequisites))
    }

    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    fn validate_prerequisite_capacity(
        &self,
        retained: usize,
        additional: usize,
    ) -> Result<(), FrontierError> {
        require_capacity(
            retained,
            additional,
            self.prerequisite_limit,
            FrontierResource::PendingPrerequisites,
        )
    }

    fn insert_candidate(
        &mut self,
        candidate_id: String,
        prerequisites: BTreeSet<BindingPrerequisite>,
    ) {
        for prerequisite in &prerequisites {
            assert!(
                self.by_prerequisite
                    .entry(prerequisite.clone())
                    .or_default()
                    .insert(candidate_id.clone()),
                "duplicate pending prerequisite association"
            );
        }
        self.prerequisite_count += prerequisites.len();
        assert!(
            self.candidates
                .insert(candidate_id, prerequisites)
                .is_none(),
            "duplicate pending candidate"
        );
    }
}

fn validate_candidate(
    candidate_id: &str,
    prerequisites: &BTreeSet<BindingPrerequisite>,
) -> Result<(), FrontierError> {
    require_text(candidate_id, "candidate_id")?;
    if prerequisites.is_empty() {
        return Err(FrontierError::EmptyPrerequisites);
    }
    for prerequisite in prerequisites {
        prerequisite.validate()?;
    }
    Ok(())
}
