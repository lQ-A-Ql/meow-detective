use std::collections::BTreeMap;

use ceph_wire::{
    BlueStoreObjectId, BlueStoreOmapKeyFamily, BlueStoreOmapKeyKind, BlueStoreOnodeHeader,
};

use super::{
    decode::is_rbd_candidate_key,
    error::BlueStoreOmapError,
    output::{append_directory_mappings, append_header},
    projection::{
        allows_headerless_scope, classify_owner, decode_candidate_entries, effective_omap_family,
        family_rank, increment_entry,
    },
    types::{
        BlueStoreOmapLimits, BlueStoreOmapOwner, BlueStoreOmapOwnerKind, BlueStoreOmapScope,
        BlueStoreOmapScopeRecord, BlueStoreOmapSnapshot, DirectoryAccumulator, HeaderAccumulator,
    },
};

#[derive(Debug)]
pub struct BlueStoreOmapFragment {
    limits: BlueStoreOmapLimits,
    open: BTreeMap<BlueStoreOmapScope, PendingScope>,
    closed: BTreeMap<BlueStoreOmapScope, ClosedScope>,
    owners: BTreeMap<(u64, u8), BlueStoreOmapOwner>,
    retained_text_bytes: usize,
}

#[derive(Debug)]
struct PendingScope {
    entry_count: u64,
    recognized_entry_count: u64,
    candidate_entries: Vec<CandidateEntry>,
    directory: DirectoryAccumulator,
    header: HeaderAccumulator,
}

#[derive(Debug)]
pub(super) struct CandidateEntry {
    pub(super) user_key: Vec<u8>,
    pub(super) value: Vec<u8>,
}

#[derive(Debug)]
pub(super) struct ClosedScope {
    pub(super) scope: BlueStoreOmapScope,
    pub(super) entry_count: u64,
    pub(super) recognized_entry_count: u64,
    pub(super) candidate_entries: Vec<CandidateEntry>,
    pub(super) directory: DirectoryAccumulator,
    pub(super) header: HeaderAccumulator,
}

impl Default for BlueStoreOmapFragment {
    fn default() -> Self {
        Self::new(BlueStoreOmapLimits::default())
    }
}

impl BlueStoreOmapFragment {
    pub fn new(limits: BlueStoreOmapLimits) -> Self {
        Self {
            limits,
            open: BTreeMap::new(),
            closed: BTreeMap::new(),
            owners: BTreeMap::new(),
            retained_text_bytes: 0,
        }
    }

    pub fn observe_onode(
        &mut self,
        object: &BlueStoreObjectId,
        onode: &BlueStoreOnodeHeader,
    ) -> Result<(), BlueStoreOmapError> {
        let Some(kind) = classify_owner(&object.object_name)? else {
            return Ok(());
        };
        if let Some(family) = effective_omap_family(onode) {
            self.insert_owner(BlueStoreOmapOwner {
                nid: onode.nid,
                family,
                kind,
            })?;
        }
        Ok(())
    }

    pub fn observe_routed_latest_value(
        &mut self,
        family: BlueStoreOmapKeyFamily,
        logical_key: &[u8],
        value: &[u8],
    ) -> Result<(), BlueStoreOmapError> {
        let key = ceph_wire::decode_bluestore_omap_logical_key(family, logical_key)?;
        let scope = BlueStoreOmapScope::from_key(&key);
        match key.kind {
            BlueStoreOmapKeyKind::Header => self.start_scope(scope),
            BlueStoreOmapKeyKind::Entry { user_key } => self.observe_entry(scope, user_key, value),
            BlueStoreOmapKeyKind::Tail => self.finish_scope(scope),
        }
    }

    pub fn merge(&mut self, other: Self) -> Result<(), BlueStoreOmapError> {
        if !self.open.is_empty() || !other.open.is_empty() {
            return Err(BlueStoreOmapError::MergeWithOpenScope);
        }
        self.merge_owners(other.owners)?;
        self.claim_scope_capacity(other.closed.len())?;
        self.claim_text_capacity(other.retained_text_bytes)?;
        for (scope, source) in other.closed {
            if self.closed.contains_key(&scope) {
                return Err(BlueStoreOmapError::DuplicateScope { scope });
            }
            self.closed.insert(scope, source);
        }
        Ok(())
    }

    pub fn finish(self) -> Result<BlueStoreOmapSnapshot, BlueStoreOmapError> {
        let scope = self.open.keys().next().copied();
        if let Some(scope) = scope {
            return Err(BlueStoreOmapError::UnclosedScope { scope });
        }
        let max_entries_per_scope = self.limits.max_entries_per_scope;
        let mut snapshot = BlueStoreOmapSnapshot::default();
        for mut closed in self.closed.into_values() {
            let owner = self
                .owners
                .get(&(closed.scope.nid, family_rank(closed.scope.family)))
                .cloned();
            if owner.is_some() {
                decode_candidate_entries(&mut closed, max_entries_per_scope)?;
            }
            snapshot.scopes.push(BlueStoreOmapScopeRecord {
                scope: closed.scope,
                owner: owner.clone(),
                entry_count: closed.entry_count,
                recognized_entry_count: closed.recognized_entry_count,
            });
            if let Some(owner) = owner.as_ref() {
                match &owner.kind {
                    BlueStoreOmapOwnerKind::RbdDirectory => {
                        append_directory_mappings(&mut snapshot, &closed, owner)?;
                    }
                    BlueStoreOmapOwnerKind::RbdHeader { image_id } => {
                        append_header(&mut snapshot, &closed, owner, image_id)?;
                    }
                }
            }
        }
        Ok(snapshot)
    }

    fn start_scope(&mut self, scope: BlueStoreOmapScope) -> Result<(), BlueStoreOmapError> {
        if self.open.contains_key(&scope) || self.closed.contains_key(&scope) {
            return Err(BlueStoreOmapError::DuplicateHeader { scope });
        }
        self.claim_scope_capacity(1)?;
        self.open.insert(
            scope,
            PendingScope {
                entry_count: 0,
                recognized_entry_count: 0,
                candidate_entries: Vec::new(),
                directory: DirectoryAccumulator::default(),
                header: HeaderAccumulator::default(),
            },
        );
        Ok(())
    }

    fn observe_entry(
        &mut self,
        scope: BlueStoreOmapScope,
        user_key: &[u8],
        value: &[u8],
    ) -> Result<(), BlueStoreOmapError> {
        self.ensure_entry_scope(scope)?;
        let pending = self
            .open
            .get_mut(&scope)
            .ok_or(BlueStoreOmapError::MissingHeader { scope })?;
        increment_entry(&mut pending.entry_count, self.limits.max_entries_per_scope)?;
        if !is_rbd_candidate_key(user_key) {
            return Ok(());
        }
        let retained_bytes =
            user_key
                .len()
                .checked_add(value.len())
                .ok_or(BlueStoreOmapError::LimitExceeded {
                    resource: "OMAP retained candidate bytes",
                    limit: self.limits.max_retained_text_bytes,
                })?;
        self.claim_text_capacity(retained_bytes)?;
        let pending = self
            .open
            .get_mut(&scope)
            .ok_or(BlueStoreOmapError::MissingHeader { scope })?;
        pending.candidate_entries.push(CandidateEntry {
            user_key: user_key.to_vec(),
            value: value.to_vec(),
        });
        Ok(())
    }

    fn ensure_entry_scope(&mut self, scope: BlueStoreOmapScope) -> Result<(), BlueStoreOmapError> {
        if self.open.contains_key(&scope) {
            return Ok(());
        }
        if !allows_headerless_scope(scope.family) {
            return Err(BlueStoreOmapError::MissingHeader { scope });
        }
        if self.closed.contains_key(&scope) {
            return Err(BlueStoreOmapError::DuplicateScope { scope });
        }
        self.claim_scope_capacity(1)?;
        self.open.insert(
            scope,
            PendingScope {
                entry_count: 0,
                recognized_entry_count: 0,
                candidate_entries: Vec::new(),
                directory: DirectoryAccumulator::default(),
                header: HeaderAccumulator::default(),
            },
        );
        Ok(())
    }

    fn finish_scope(&mut self, scope: BlueStoreOmapScope) -> Result<(), BlueStoreOmapError> {
        if !self.open.contains_key(&scope) && allows_headerless_scope(scope.family) {
            if self.closed.contains_key(&scope) {
                return Err(BlueStoreOmapError::DuplicateScope { scope });
            }
            self.claim_scope_capacity(1)?;
            self.open.insert(
                scope,
                PendingScope {
                    entry_count: 0,
                    recognized_entry_count: 0,
                    candidate_entries: Vec::new(),
                    directory: DirectoryAccumulator::default(),
                    header: HeaderAccumulator::default(),
                },
            );
        }
        let pending = self
            .open
            .remove(&scope)
            .ok_or(BlueStoreOmapError::MissingHeader { scope })?;
        self.closed.insert(
            scope,
            ClosedScope {
                scope,
                entry_count: pending.entry_count,
                recognized_entry_count: pending.recognized_entry_count,
                candidate_entries: pending.candidate_entries,
                directory: pending.directory,
                header: pending.header,
            },
        );
        Ok(())
    }

    fn insert_owner(&mut self, owner: BlueStoreOmapOwner) -> Result<(), BlueStoreOmapError> {
        let key = (owner.nid, family_rank(owner.family));
        if let Some(existing) = self.owners.get(&key) {
            if existing != &owner {
                return Err(BlueStoreOmapError::OwnerConflict {
                    nid: owner.nid,
                    family: owner.family,
                });
            }
            return Ok(());
        }
        if self.owners.len() >= self.limits.max_owners {
            return Err(BlueStoreOmapError::LimitExceeded {
                resource: "OMAP owners",
                limit: self.limits.max_owners,
            });
        }
        self.owners.insert(key, owner);
        Ok(())
    }

    fn merge_owners(
        &mut self,
        owners: BTreeMap<(u64, u8), BlueStoreOmapOwner>,
    ) -> Result<(), BlueStoreOmapError> {
        for owner in owners.into_values() {
            self.insert_owner(owner)?;
        }
        Ok(())
    }

    fn claim_scope_capacity(&self, additional: usize) -> Result<(), BlueStoreOmapError> {
        let current = self.open.len().checked_add(self.closed.len()).ok_or(
            BlueStoreOmapError::LimitExceeded {
                resource: "OMAP scopes",
                limit: self.limits.max_scopes,
            },
        )?;
        let total = current
            .checked_add(additional)
            .ok_or(BlueStoreOmapError::LimitExceeded {
                resource: "OMAP scopes",
                limit: self.limits.max_scopes,
            })?;
        if total > self.limits.max_scopes {
            return Err(BlueStoreOmapError::LimitExceeded {
                resource: "OMAP scopes",
                limit: self.limits.max_scopes,
            });
        }
        Ok(())
    }

    fn claim_text_capacity(&mut self, additional: usize) -> Result<(), BlueStoreOmapError> {
        let total = self.retained_text_bytes.checked_add(additional).ok_or(
            BlueStoreOmapError::LimitExceeded {
                resource: "OMAP retained text",
                limit: self.limits.max_retained_text_bytes,
            },
        )?;
        if total > self.limits.max_retained_text_bytes {
            return Err(BlueStoreOmapError::LimitExceeded {
                resource: "OMAP retained text",
                limit: self.limits.max_retained_text_bytes,
            });
        }
        self.retained_text_bytes = total;
        Ok(())
    }
}
