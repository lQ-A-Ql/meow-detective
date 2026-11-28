use std::collections::{BTreeMap, BTreeSet};

use super::{
    require_capacity, require_limit, require_text, BindingPrerequisite, CapabilityEntry, Frontier,
    FrontierError, FrontierResource,
};

#[derive(Debug)]
pub struct CapabilityIndex {
    entry_limit: usize,
    role_limit: usize,
    role_count: usize,
    entries: BTreeMap<String, CapabilityEntry>,
    by_prerequisite: BTreeMap<BindingPrerequisite, BTreeSet<String>>,
}

impl CapabilityIndex {
    /// Applies the same bound to capability entries and role associations.
    pub fn new(limit: usize) -> Result<Self, FrontierError> {
        Self::with_limits(limit, limit)
    }

    pub fn with_limits(entry_limit: usize, role_limit: usize) -> Result<Self, FrontierError> {
        require_limit(entry_limit, FrontierResource::CapabilityEntries)?;
        require_limit(role_limit, FrontierResource::CapabilityRoles)?;
        Ok(Self {
            entry_limit,
            role_limit,
            role_count: 0,
            entries: BTreeMap::new(),
            by_prerequisite: BTreeMap::new(),
        })
    }

    pub fn publish(&mut self, entry: CapabilityEntry) -> Result<bool, FrontierError> {
        if !self.prepare_publication(&entry)? {
            return Ok(false);
        }
        self.insert_entry(entry);
        Ok(true)
    }

    pub fn publish_to_frontier(
        &mut self,
        entry: CapabilityEntry,
        frontier: &mut Frontier,
    ) -> Result<bool, FrontierError> {
        Ok(!self
            .publish_batch_to_frontier(vec![entry], frontier)?
            .is_empty())
    }

    /// Checks the complete batch before mutating either index or frontier.
    pub fn publish_batch_to_frontier(
        &mut self,
        entries: Vec<CapabilityEntry>,
        frontier: &mut Frontier,
    ) -> Result<Vec<String>, FrontierError> {
        let mut staged = BTreeMap::<String, (CapabilityEntry, bool)>::new();
        let mut added_ids = Vec::new();
        let mut entry_count = self.entries.len();
        let mut role_count = self.role_count;
        let mut queued_count = frontier.queue.len();
        for entry in entries {
            if let Some((existing, _)) = staged.get(&entry.capability.id) {
                if existing != &entry {
                    return Err(FrontierError::CapabilityConflict);
                }
                continue;
            }
            if !self.prepare_publication(&entry)? {
                continue;
            }
            require_capacity(
                entry_count,
                1,
                self.entry_limit,
                FrontierResource::CapabilityEntries,
            )?;
            require_capacity(
                role_count,
                entry.native_roles.len(),
                self.role_limit,
                FrontierResource::CapabilityRoles,
            )?;
            let enqueue = !frontier.queued.contains(&entry.capability.id);
            if enqueue {
                require_capacity(
                    queued_count,
                    1,
                    frontier.limit,
                    FrontierResource::QueuedCapabilities,
                )?;
                queued_count += 1;
            }
            entry_count += 1;
            role_count += entry.native_roles.len();
            let id = entry.capability.id.clone();
            added_ids.push(id.clone());
            staged.insert(id, (entry, enqueue));
        }
        for id in &added_ids {
            let (entry, enqueue) = staged
                .remove(id)
                .expect("validated publication batch entry is absent");
            self.insert_entry(entry);
            if enqueue {
                frontier.enqueue(id.clone());
            }
        }
        Ok(added_ids)
    }

    pub fn get(&self, id: &str) -> Result<Option<&CapabilityEntry>, FrontierError> {
        require_text(id, "capability_id")?;
        Ok(self.entries.get(id))
    }

    pub fn matching(
        &self,
        prerequisite: &BindingPrerequisite,
    ) -> Result<Vec<&CapabilityEntry>, FrontierError> {
        prerequisite.validate()?;
        let Some(ids) = self.by_prerequisite.get(prerequisite) else {
            return Ok(Vec::new());
        };
        Ok(ids
            .iter()
            .map(|id| {
                self.entries
                    .get(id)
                    .expect("capability role index references an absent entry")
            })
            .collect())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn prepare_publication(&self, entry: &CapabilityEntry) -> Result<bool, FrontierError> {
        if let Some(existing) = self.entries.get(&entry.capability.id) {
            return if existing == entry {
                Ok(false)
            } else {
                Err(FrontierError::CapabilityConflict)
            };
        }
        entry.validate()?;
        require_capacity(
            self.entries.len(),
            1,
            self.entry_limit,
            FrontierResource::CapabilityEntries,
        )?;
        require_capacity(
            self.role_count,
            entry.native_roles.len(),
            self.role_limit,
            FrontierResource::CapabilityRoles,
        )?;
        Ok(true)
    }

    fn insert_entry(&mut self, entry: CapabilityEntry) {
        let id = entry.capability.id.clone();
        for prerequisite in entry.prerequisites() {
            assert!(
                self.by_prerequisite
                    .entry(prerequisite)
                    .or_default()
                    .insert(id.clone()),
                "duplicate capability role association"
            );
        }
        self.role_count += entry.native_roles.len();
        assert!(
            self.entries.insert(id, entry).is_none(),
            "duplicate immutable capability"
        );
    }
}
