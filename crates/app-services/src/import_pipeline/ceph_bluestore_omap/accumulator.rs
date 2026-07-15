use std::collections::BTreeMap;

use ceph_wire::{
    BlueStoreObjectId, BlueStoreOmapKeyFamily, BlueStoreOmapKeyKind, BlueStoreOnodeHeader,
};

use super::{
    decode::{decode_entry, DecodedOmapEntry},
    error::BlueStoreOmapError,
    output::{append_directory_mappings, append_header},
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
    scope: BlueStoreOmapScope,
    header_present: bool,
    entry_count: u64,
    recognized_entry_count: u64,
    directory: DirectoryAccumulator,
    header: HeaderAccumulator,
}

#[derive(Debug)]
pub(super) struct ClosedScope {
    pub(super) scope: BlueStoreOmapScope,
    pub(super) header_present: bool,
    pub(super) entry_count: u64,
    pub(super) recognized_entry_count: u64,
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
        for (enabled, family) in omap_families(onode) {
            if enabled {
                self.insert_owner(BlueStoreOmapOwner {
                    nid: onode.nid,
                    family,
                    kind: kind.clone(),
                })?;
            }
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
        let mut snapshot = BlueStoreOmapSnapshot::default();
        for closed in self.closed.into_values() {
            let owner = closed.header_present.then(|| {
                self.owners
                    .get(&(closed.scope.nid, family_rank(closed.scope.family)))
                    .cloned()
            });
            let owner = owner.flatten();
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
                scope,
                header_present: true,
                entry_count: 0,
                recognized_entry_count: 0,
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
        if !pending.header_present {
            return Ok(());
        }

        let decoded = decode_entry(user_key, value)?;
        let text_bytes = decoded.as_ref().map_or(0, DecodedOmapEntry::text_bytes);
        self.claim_text_capacity(text_bytes)?;
        let pending = self
            .open
            .get_mut(&scope)
            .ok_or(BlueStoreOmapError::MissingHeader { scope })?;
        if let Some(decoded) = decoded {
            observe_decoded_entry(pending, decoded)?;
            pending.recognized_entry_count = pending.recognized_entry_count.checked_add(1).ok_or(
                BlueStoreOmapError::LimitExceeded {
                    resource: "OMAP recognized entry count",
                    limit: self.limits.max_entries_per_scope,
                },
            )?;
        }
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
                scope,
                header_present: false,
                entry_count: 0,
                recognized_entry_count: 0,
                directory: DirectoryAccumulator::default(),
                header: HeaderAccumulator::default(),
            },
        );
        Ok(())
    }

    fn finish_scope(&mut self, scope: BlueStoreOmapScope) -> Result<(), BlueStoreOmapError> {
        let pending = self
            .open
            .remove(&scope)
            .ok_or(BlueStoreOmapError::MissingHeader { scope })?;
        self.closed.insert(
            scope,
            ClosedScope {
                scope,
                header_present: pending.header_present,
                entry_count: pending.entry_count,
                recognized_entry_count: pending.recognized_entry_count,
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

fn observe_decoded_entry(
    pending: &mut PendingScope,
    entry: DecodedOmapEntry,
) -> Result<(), BlueStoreOmapError> {
    match entry {
        DecodedOmapEntry::DirectoryName {
            image_name,
            image_id,
        } => insert_directory_mapping(
            &pending.scope,
            &mut pending.directory.name_to_id,
            image_name,
            image_id,
        ),
        DecodedOmapEntry::DirectoryId {
            image_id,
            image_name,
        } => insert_directory_mapping(
            &pending.scope,
            &mut pending.directory.id_to_name,
            image_id,
            image_name,
        ),
        DecodedOmapEntry::Size(value) => set_header(
            &pending.scope,
            &mut pending.header.size,
            value,
            "rbd header size",
        ),
        DecodedOmapEntry::Order(value) => set_header(
            &pending.scope,
            &mut pending.header.order,
            value,
            "rbd header order",
        ),
        DecodedOmapEntry::Features(value) => set_header(
            &pending.scope,
            &mut pending.header.features,
            value,
            "rbd header features",
        ),
        DecodedOmapEntry::ObjectPrefix(value) => set_header(
            &pending.scope,
            &mut pending.header.object_prefix,
            value,
            "rbd header object_prefix",
        ),
        DecodedOmapEntry::StripeUnit(value) => set_header(
            &pending.scope,
            &mut pending.header.stripe_unit,
            value,
            "rbd header stripe_unit",
        ),
        DecodedOmapEntry::StripeCount(value) => set_header(
            &pending.scope,
            &mut pending.header.stripe_count,
            value,
            "rbd header stripe_count",
        ),
        DecodedOmapEntry::DataPoolId(value) => set_header(
            &pending.scope,
            &mut pending.header.data_pool_id,
            value,
            "rbd header data_pool_id",
        ),
    }
}

fn insert_directory_mapping(
    scope: &BlueStoreOmapScope,
    target: &mut BTreeMap<String, String>,
    key: String,
    value: String,
) -> Result<(), BlueStoreOmapError> {
    if let Some(existing) = target.get(&key) {
        if existing == &value {
            return Err(BlueStoreOmapError::DuplicateDirectoryMapping { scope: *scope });
        }
        return Err(BlueStoreOmapError::ConflictingDirectoryMapping { scope: *scope });
    }
    target.insert(key, value);
    Ok(())
}

fn set_header<T>(
    scope: &BlueStoreOmapScope,
    target: &mut Option<T>,
    value: T,
    field: &'static str,
) -> Result<(), BlueStoreOmapError> {
    if target.replace(value).is_some() {
        return Err(BlueStoreOmapError::DuplicateField {
            scope: *scope,
            field,
        });
    }
    Ok(())
}

fn increment_entry(value: &mut u64, limit: usize) -> Result<(), BlueStoreOmapError> {
    let next = value
        .checked_add(1)
        .ok_or(BlueStoreOmapError::LimitExceeded {
            resource: "OMAP entries",
            limit,
        })?;
    if next > limit as u64 {
        return Err(BlueStoreOmapError::LimitExceeded {
            resource: "OMAP entries",
            limit,
        });
    }
    *value = next;
    Ok(())
}

fn classify_owner(
    object_name: &[u8],
) -> Result<Option<BlueStoreOmapOwnerKind>, BlueStoreOmapError> {
    if object_name == b"rbd_directory" {
        return Ok(Some(BlueStoreOmapOwnerKind::RbdDirectory));
    }
    let Some(image_id) = object_name.strip_prefix(b"rbd_header.") else {
        return Ok(None);
    };
    if image_id.is_empty() {
        return Err(BlueStoreOmapError::InvalidOwnerName {
            kind: "rbd_header",
            reason: "image id is empty",
        });
    }
    let image_id =
        std::str::from_utf8(image_id).map_err(|_| BlueStoreOmapError::InvalidOwnerName {
            kind: "rbd_header",
            reason: "image id is not valid UTF-8",
        })?;
    if image_id.contains('\0') {
        return Err(BlueStoreOmapError::InvalidOwnerName {
            kind: "rbd_header",
            reason: "image id contains NUL",
        });
    }
    Ok(Some(BlueStoreOmapOwnerKind::RbdHeader {
        image_id: image_id.to_string(),
    }))
}

fn omap_families(onode: &BlueStoreOnodeHeader) -> [(bool, BlueStoreOmapKeyFamily); 4] {
    [
        (onode.flags.omap, BlueStoreOmapKeyFamily::Bulk),
        (onode.flags.pgmeta_omap, BlueStoreOmapKeyFamily::PgMeta),
        (onode.flags.per_pool_omap, BlueStoreOmapKeyFamily::PerPool),
        (onode.flags.per_pg_omap, BlueStoreOmapKeyFamily::PerPg),
    ]
}

fn family_rank(family: BlueStoreOmapKeyFamily) -> u8 {
    match family {
        BlueStoreOmapKeyFamily::Bulk => 0,
        BlueStoreOmapKeyFamily::PgMeta => 1,
        BlueStoreOmapKeyFamily::PerPool => 2,
        BlueStoreOmapKeyFamily::PerPg => 3,
    }
}

fn allows_headerless_scope(family: BlueStoreOmapKeyFamily) -> bool {
    matches!(
        family,
        BlueStoreOmapKeyFamily::PgMeta | BlueStoreOmapKeyFamily::PerPg
    )
}
