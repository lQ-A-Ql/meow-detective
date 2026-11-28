mod capability_index;
mod pending_bindings;

use std::collections::{BTreeSet, VecDeque};

use domain::structure::{CapabilityError, CapabilityKind, CapabilityRef};

pub use capability_index::CapabilityIndex;
pub use pending_bindings::PendingBindings;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BindingPrerequisite {
    pub scope_id: String,
    pub kind: CapabilityKind,
    pub native_role: String,
}

impl BindingPrerequisite {
    pub fn validate(&self) -> Result<(), FrontierError> {
        require_text(&self.scope_id, "scope_id")?;
        require_text(&self.native_role, "native_role")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityEntry {
    pub capability: CapabilityRef,
    pub scope_id: String,
    pub native_roles: BTreeSet<String>,
}

impl CapabilityEntry {
    pub fn validate(&self) -> Result<(), FrontierError> {
        self.capability.validate()?;
        require_text(&self.scope_id, "scope_id")?;
        for role in &self.native_roles {
            require_text(role, "native_role")?;
        }
        Ok(())
    }

    fn prerequisites(&self) -> impl Iterator<Item = BindingPrerequisite> + '_ {
        self.native_roles.iter().map(|role| BindingPrerequisite {
            scope_id: self.scope_id.clone(),
            kind: self.capability.kind,
            native_role: role.clone(),
        })
    }
}

/// Queued IDs are deduplicated until consumed. Publication deduplication
/// across consumed entries belongs to `CapabilityIndex`.
#[derive(Debug)]
pub struct Frontier {
    limit: usize,
    queue: VecDeque<String>,
    queued: BTreeSet<String>,
}

impl Frontier {
    pub fn new(limit: usize) -> Result<Self, FrontierError> {
        require_limit(limit, FrontierResource::QueuedCapabilities)?;
        Ok(Self {
            limit,
            queue: VecDeque::new(),
            queued: BTreeSet::new(),
        })
    }

    pub fn push(&mut self, capability: &CapabilityRef) -> Result<bool, FrontierError> {
        if !self.prepare_push(capability)? {
            return Ok(false);
        }
        self.enqueue(capability.id.clone());
        Ok(true)
    }

    pub fn pop(&mut self) -> Option<String> {
        let id = self.queue.pop_front()?;
        assert!(self.queued.remove(&id), "frontier queue index diverged");
        Some(id)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    fn prepare_push(&self, capability: &CapabilityRef) -> Result<bool, FrontierError> {
        capability.validate()?;
        if self.queued.contains(&capability.id) {
            return Ok(false);
        }
        require_capacity(
            self.queue.len(),
            1,
            self.limit,
            FrontierResource::QueuedCapabilities,
        )?;
        Ok(true)
    }

    fn enqueue(&mut self, id: String) {
        assert!(self.queued.insert(id.clone()), "duplicate frontier entry");
        self.queue.push_back(id);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontierResource {
    CapabilityEntries,
    CapabilityRoles,
    QueuedCapabilities,
    PendingCandidates,
    PendingPrerequisites,
}

#[derive(Debug, thiserror::Error)]
pub enum FrontierError {
    #[error("frontier field '{field}' is empty")]
    EmptyField { field: &'static str },
    #[error("frontier capacity for {resource:?} must be positive")]
    InvalidLimit { resource: FrontierResource },
    #[error("frontier capacity for {resource:?} requires {required}, exceeding limit {limit}")]
    CapacityExceeded {
        resource: FrontierResource,
        required: usize,
        limit: usize,
    },
    #[error("frontier capacity for {resource:?} overflows the address space")]
    CapacityOverflow { resource: FrontierResource },
    #[error(transparent)]
    InvalidCapability(#[from] CapabilityError),
    #[error("an immutable capability ID was published with different contents")]
    CapabilityConflict,
    #[error("a pending candidate already has different prerequisites")]
    CandidateConflict,
    #[error("a pending candidate must declare its missing prerequisites")]
    EmptyPrerequisites,
    #[error("a pending candidate does not exist")]
    MissingCandidate,
}

fn require_text(value: &str, field: &'static str) -> Result<(), FrontierError> {
    if value.trim().is_empty() {
        return Err(FrontierError::EmptyField { field });
    }
    Ok(())
}

fn require_limit(limit: usize, resource: FrontierResource) -> Result<(), FrontierError> {
    if limit == 0 {
        return Err(FrontierError::InvalidLimit { resource });
    }
    Ok(())
}

fn require_capacity(
    used: usize,
    additional: usize,
    limit: usize,
    resource: FrontierResource,
) -> Result<(), FrontierError> {
    let required = used
        .checked_add(additional)
        .ok_or(FrontierError::CapacityOverflow { resource })?;
    if required > limit {
        return Err(FrontierError::CapacityExceeded {
            resource,
            required,
            limit,
        });
    }
    Ok(())
}
