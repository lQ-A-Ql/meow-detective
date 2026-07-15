use std::collections::{HashMap, HashSet};

use crate::connection::{DbError, DbResult};

use super::{
    digest::omap_aggregate_sha256, CephBluestoreOmapAggregate, CephBluestoreOmapScopeRecord,
    CephBluestoreRbdDirectoryRecord, CephBluestoreRbdHeaderRecord, BLUESTORE_OMAP_DECODE_PROFILE,
    BLUESTORE_OMAP_SCHEMA_VERSION,
};

pub fn validate_replacement(aggregate: &CephBluestoreOmapAggregate) -> DbResult<()> {
    validate_scan(aggregate)?;
    let scopes = validate_scopes(aggregate)?;
    validate_directory_mappings(aggregate, &scopes)?;
    validate_headers(aggregate, &scopes)?;
    if aggregate.scan.omap_sha256 != omap_aggregate_sha256(aggregate) {
        return omap_error("BlueStore OMAP aggregate digest does not match its normalized rows");
    }
    Ok(())
}

pub fn canonical_scope_identity(
    key_family: &str,
    pool_kind: &str,
    pool_value_i64: Option<i64>,
    pool_value_hex: Option<&str>,
    hash: Option<u32>,
    nid_hex: &str,
) -> Option<String> {
    if !valid_family(key_family) || !valid_hex_u64(nid_hex) {
        return None;
    }
    let pool = match (key_family, pool_kind, pool_value_i64, pool_value_hex) {
        ("bulk" | "pgMeta", "none", None, None) => "-".to_string(),
        ("perPool", "perPool", Some(value), None) => format!("i{value}"),
        ("perPg", "perPg", None, Some(value)) if valid_hex_u64(value) => {
            format!("u{value}")
        }
        _ => return None,
    };
    let hash = match (key_family, hash) {
        ("perPg", Some(value)) => format!("{value:08x}"),
        ("bulk" | "pgMeta" | "perPool", None) => "-".to_string(),
        _ => return None,
    };
    Some(format!("{key_family}:{pool}:{hash}:{nid_hex}"))
}

fn validate_scan(aggregate: &CephBluestoreOmapAggregate) -> DbResult<()> {
    let scan = &aggregate.scan;
    if !valid_text(&scan.inventory_id)
        || !valid_text(&scan.data_source_id)
        || scan.schema_version != BLUESTORE_OMAP_SCHEMA_VERSION
        || scan.decode_profile != BLUESTORE_OMAP_DECODE_PROFILE
        || !valid_sha256(&scan.sharding_sha256)
        || !valid_sha256(&scan.latest_state_sha256)
        || !valid_sha256(&scan.semantic_sha256)
        || !valid_sha256(&scan.omap_sha256)
        || !scan.profile_complete
        || scan.scope_count != checked_len(aggregate.scopes.len())?
        || scan.directory_mapping_count != checked_len(aggregate.directory_mappings.len())?
        || scan.rbd_header_count != checked_len(aggregate.rbd_headers.len())?
    {
        return omap_error("BlueStore OMAP scan metadata is incomplete or inconsistent");
    }
    Ok(())
}

fn validate_scopes(
    aggregate: &CephBluestoreOmapAggregate,
) -> DbResult<HashMap<&str, &CephBluestoreOmapScopeRecord>> {
    let mut scopes = HashMap::with_capacity(aggregate.scopes.len());
    for record in &aggregate.scopes {
        if record.inventory_id != aggregate.scan.inventory_id
            || record.entry_count > i64::MAX as u64
            || record.recognized_entry_count > record.entry_count
            || canonical_scope_identity(
                &record.key_family,
                &record.pool_kind,
                record.pool_value_i64,
                record.pool_value_hex.as_deref(),
                record.hash,
                &record.nid_hex,
            )
            .as_deref()
                != Some(record.scope_identity.as_str())
            || !valid_owner(record)
            || scopes
                .insert(record.scope_identity.as_str(), record)
                .is_some()
        {
            return omap_error("BlueStore OMAP scope metadata is invalid or duplicated");
        }
    }
    Ok(scopes)
}

fn valid_owner(record: &CephBluestoreOmapScopeRecord) -> bool {
    match (
        record.owner_nid_hex.as_deref(),
        record.owner_family.as_deref(),
        record.owner_kind.as_deref(),
        record.owner_image_id.as_deref(),
    ) {
        (None, None, None, None) => true,
        (Some(nid), Some(family), Some("rbdDirectory"), None) => {
            nid == record.nid_hex && family == record.key_family
        }
        (Some(nid), Some(family), Some("rbdHeader"), Some(image_id)) => {
            nid == record.nid_hex && family == record.key_family && valid_bounded_text(image_id)
        }
        _ => false,
    }
}

fn validate_directory_mappings(
    aggregate: &CephBluestoreOmapAggregate,
    scopes: &HashMap<&str, &CephBluestoreOmapScopeRecord>,
) -> DbResult<()> {
    let mut names = HashSet::with_capacity(aggregate.directory_mappings.len());
    let mut ids = HashSet::with_capacity(aggregate.directory_mappings.len());
    for record in &aggregate.directory_mappings {
        let scope = scopes.get(record.scope_identity.as_str());
        if record.inventory_id != aggregate.scan.inventory_id
            || !valid_bounded_text(&record.image_name)
            || !valid_bounded_text(&record.image_id)
            || !valid_hex_u64(&record.owner_nid_hex)
            || scope.is_none_or(|scope| !is_directory_owner(scope, record))
            || !names.insert((record.scope_identity.as_str(), record.image_name.as_str()))
            || !ids.insert((record.scope_identity.as_str(), record.image_id.as_str()))
        {
            return omap_error("BlueStore RBD directory mapping is invalid or duplicated");
        }
    }
    Ok(())
}

fn validate_headers(
    aggregate: &CephBluestoreOmapAggregate,
    scopes: &HashMap<&str, &CephBluestoreOmapScopeRecord>,
) -> DbResult<()> {
    let mut images = HashSet::with_capacity(aggregate.rbd_headers.len());
    let mut header_scopes = HashSet::with_capacity(aggregate.rbd_headers.len());
    for record in &aggregate.rbd_headers {
        let scope = scopes.get(record.scope_identity.as_str());
        if record.inventory_id != aggregate.scan.inventory_id
            || !valid_bounded_text(&record.image_id)
            || !valid_hex_u64(&record.owner_nid_hex)
            || !record.size_hex.as_deref().is_none_or(valid_hex_u64)
            || record.object_order.is_some_and(|value| value > 63)
            || !record.features_hex.as_deref().is_none_or(valid_hex_u64)
            || !record
                .operation_features_hex
                .as_deref()
                .is_none_or(valid_hex_u64)
            || !record
                .object_prefix
                .as_deref()
                .is_none_or(valid_bounded_text)
            || !record.stripe_unit_hex.as_deref().is_none_or(valid_hex_u64)
            || !record.stripe_count_hex.as_deref().is_none_or(valid_hex_u64)
            || scope.is_none_or(|scope| !is_header_owner(scope, record))
            || !images.insert(record.image_id.as_str())
            || !header_scopes.insert(record.scope_identity.as_str())
        {
            return omap_error("BlueStore RBD header metadata is invalid or duplicated");
        }
    }
    Ok(())
}

fn is_directory_owner(
    scope: &CephBluestoreOmapScopeRecord,
    record: &CephBluestoreRbdDirectoryRecord,
) -> bool {
    scope.owner_kind.as_deref() == Some("rbdDirectory")
        && scope.owner_nid_hex.as_deref() == Some(record.owner_nid_hex.as_str())
}

fn is_header_owner(
    scope: &CephBluestoreOmapScopeRecord,
    record: &CephBluestoreRbdHeaderRecord,
) -> bool {
    scope.owner_kind.as_deref() == Some("rbdHeader")
        && scope.owner_nid_hex.as_deref() == Some(record.owner_nid_hex.as_str())
        && scope.owner_image_id.as_deref() == Some(record.image_id.as_str())
}

pub(super) fn valid_family(value: &str) -> bool {
    matches!(value, "bulk" | "pgMeta" | "perPool" | "perPg")
}

pub(super) fn valid_hex_u64(value: &str) -> bool {
    value.len() == 16
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && u64::from_str_radix(value, 16).is_ok()
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && !value.contains('\0')
}

fn valid_bounded_text(value: &str) -> bool {
    valid_text(value) && value.len() <= 4096
}

fn checked_len(value: usize) -> DbResult<u64> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value <= i64::MAX as u64)
        .ok_or_else(|| DbError::System("BlueStore OMAP row count exceeds SQLite".to_string()))
}

fn omap_error<T>(message: &str) -> DbResult<T> {
    Err(DbError::System(message.to_string()))
}
