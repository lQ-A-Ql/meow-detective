use std::collections::BTreeMap;

use ceph_wire::{BlueStoreOmapKeyFamily, BlueStoreOnodeHeader};

use super::{
    accumulator::ClosedScope,
    decode::{decode_entry, DecodedOmapEntry},
    error::BlueStoreOmapError,
    types::{BlueStoreOmapOwnerKind, BlueStoreOmapScope},
};

fn observe_decoded_entry(
    pending: &mut ClosedScope,
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
        DecodedOmapEntry::OperationFeatures(value) => set_header(
            &pending.scope,
            &mut pending.header.operation_features,
            value,
            "rbd header operation features",
        ),
        DecodedOmapEntry::ParentKeyPresent => set_header_presence(
            &pending.scope,
            &mut pending.header.parent_key_present,
            "rbd header parent",
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

pub(super) fn decode_candidate_entries(
    scope: &mut ClosedScope,
    max_entries_per_scope: usize,
) -> Result<(), BlueStoreOmapError> {
    for entry in std::mem::take(&mut scope.candidate_entries) {
        let Some(decoded) = decode_entry(&entry.user_key, &entry.value)? else {
            continue;
        };
        observe_decoded_entry(scope, decoded)?;
        increment_entry(&mut scope.recognized_entry_count, max_entries_per_scope)?;
    }
    Ok(())
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

fn set_header_presence(
    scope: &BlueStoreOmapScope,
    target: &mut bool,
    field: &'static str,
) -> Result<(), BlueStoreOmapError> {
    if std::mem::replace(target, true) {
        Err(BlueStoreOmapError::DuplicateField {
            scope: *scope,
            field,
        })
    } else {
        Ok(())
    }
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

pub(super) fn increment_entry(value: &mut u64, limit: usize) -> Result<(), BlueStoreOmapError> {
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

pub(super) fn classify_owner(
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

pub(super) fn effective_omap_family(
    onode: &BlueStoreOnodeHeader,
) -> Option<BlueStoreOmapKeyFamily> {
    if !onode.flags.omap {
        return None;
    }
    if onode.flags.pgmeta_omap {
        Some(BlueStoreOmapKeyFamily::PgMeta)
    } else if onode.flags.per_pg_omap {
        Some(BlueStoreOmapKeyFamily::PerPg)
    } else if onode.flags.per_pool_omap {
        Some(BlueStoreOmapKeyFamily::PerPool)
    } else {
        Some(BlueStoreOmapKeyFamily::Bulk)
    }
}

pub(super) fn family_rank(family: BlueStoreOmapKeyFamily) -> u8 {
    match family {
        BlueStoreOmapKeyFamily::Bulk => 0,
        BlueStoreOmapKeyFamily::PgMeta => 1,
        BlueStoreOmapKeyFamily::PerPool => 2,
        BlueStoreOmapKeyFamily::PerPg => 3,
    }
}

pub(super) fn allows_headerless_scope(_family: BlueStoreOmapKeyFamily) -> bool {
    true
}
