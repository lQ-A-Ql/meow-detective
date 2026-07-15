use ceph_wire::BlueStoreOmapKeyFamily;
use persistence_sqlite::repositories::{
    ceph_bluestore_omap_repo::{
        canonical_scope_identity, omap_aggregate_sha256, validate_replacement,
        CephBluestoreOmapAggregate, CephBluestoreOmapScanRecord, CephBluestoreOmapScopeRecord,
        CephBluestoreRbdDirectoryRecord, CephBluestoreRbdHeaderRecord,
        BLUESTORE_OMAP_DECODE_PROFILE, BLUESTORE_OMAP_SCHEMA_VERSION,
    },
    ceph_bluestore_semantic_repo::CephBluestoreSemanticAggregate,
};
use transport::CommandError;

use super::ceph_bluestore_omap::{
    BlueStoreOmapOwnerKind, BlueStoreOmapPoolScope, BlueStoreOmapScope, BlueStoreOmapSnapshot,
};

pub(super) fn build_omap_aggregate(
    data_source_id: &str,
    semantic: &CephBluestoreSemanticAggregate,
    snapshot: &BlueStoreOmapSnapshot,
) -> Result<CephBluestoreOmapAggregate, CommandError> {
    let inventory_id = semantic.scan.inventory_id.as_str();
    let mut aggregate = CephBluestoreOmapAggregate {
        scan: CephBluestoreOmapScanRecord {
            inventory_id: inventory_id.to_string(),
            data_source_id: data_source_id.to_string(),
            schema_version: BLUESTORE_OMAP_SCHEMA_VERSION,
            decode_profile: BLUESTORE_OMAP_DECODE_PROFILE.to_string(),
            sharding_sha256: semantic.scan.sharding_sha256.clone(),
            latest_state_sha256: semantic.scan.latest_state_sha256.clone(),
            semantic_sha256: semantic.scan.semantic_sha256.clone(),
            omap_sha256: String::new(),
            scope_count: snapshot.scopes.len() as u64,
            directory_mapping_count: snapshot.directory_mappings.len() as u64,
            rbd_header_count: snapshot.rbd_headers.len() as u64,
            profile_complete: true,
        },
        scopes: snapshot
            .scopes
            .iter()
            .map(|record| map_scope(inventory_id, record))
            .collect::<Result<Vec<_>, _>>()?,
        directory_mappings: snapshot
            .directory_mappings
            .iter()
            .map(|record| {
                Ok(CephBluestoreRbdDirectoryRecord {
                    inventory_id: inventory_id.to_string(),
                    scope_identity: scope_identity(&record.scope)?,
                    owner_nid_hex: hex_u64(record.owner_nid),
                    image_name: record.image_name.clone(),
                    image_id: record.image_id.clone(),
                    bidirectional: record.bidirectional,
                })
            })
            .collect::<Result<Vec<_>, CommandError>>()?,
        rbd_headers: snapshot
            .rbd_headers
            .iter()
            .map(|record| {
                Ok(CephBluestoreRbdHeaderRecord {
                    inventory_id: inventory_id.to_string(),
                    scope_identity: scope_identity(&record.scope)?,
                    owner_nid_hex: hex_u64(record.owner_nid),
                    image_id: record.image_id.clone(),
                    size_hex: record.size.map(hex_u64),
                    object_order: record.order,
                    features_hex: record.features.map(hex_u64),
                    operation_features_hex: record.operation_features.map(hex_u64),
                    parent_key_present: record.parent_key_present,
                    object_prefix: record.object_prefix.clone(),
                    stripe_unit_hex: record.stripe_unit.map(hex_u64),
                    stripe_count_hex: record.stripe_count.map(hex_u64),
                    data_pool_id: record.data_pool_id,
                })
            })
            .collect::<Result<Vec<_>, CommandError>>()?,
    };
    aggregate
        .scopes
        .sort_unstable_by(|left, right| left.scope_identity.cmp(&right.scope_identity));
    aggregate
        .directory_mappings
        .sort_unstable_by(|left, right| {
            (
                left.scope_identity.as_str(),
                left.image_name.as_str(),
                left.image_id.as_str(),
            )
                .cmp(&(
                    right.scope_identity.as_str(),
                    right.image_name.as_str(),
                    right.image_id.as_str(),
                ))
        });
    aggregate
        .rbd_headers
        .sort_unstable_by(|left, right| left.image_id.cmp(&right.image_id));
    aggregate.scan.omap_sha256 = omap_aggregate_sha256(&aggregate);
    validate_replacement(&aggregate).map_err(CommandError::from_service_error)?;
    Ok(aggregate)
}

fn map_scope(
    inventory_id: &str,
    record: &super::ceph_bluestore_omap::BlueStoreOmapScopeRecord,
) -> Result<CephBluestoreOmapScopeRecord, CommandError> {
    let (key_family, pool_kind, pool_value_i64, pool_value_hex, hash, nid_hex) =
        scope_parts(&record.scope);
    let (owner_nid_hex, owner_family, owner_kind, owner_image_id) =
        record
            .owner
            .as_ref()
            .map_or((None, None, None, None), |owner| {
                let (kind, image_id) = match &owner.kind {
                    BlueStoreOmapOwnerKind::RbdDirectory => ("rbdDirectory", None),
                    BlueStoreOmapOwnerKind::RbdHeader { image_id } => {
                        ("rbdHeader", Some(image_id.clone()))
                    }
                };
                (
                    Some(hex_u64(owner.nid)),
                    Some(family_name(owner.family).to_string()),
                    Some(kind.to_string()),
                    image_id,
                )
            });
    let scope_identity = canonical_scope_identity(
        key_family,
        pool_kind,
        pool_value_i64,
        pool_value_hex.as_deref(),
        hash,
        &nid_hex,
    )
    .ok_or_else(|| CommandError::parser("BlueStore OMAP scope identity is not canonical"))?;
    Ok(CephBluestoreOmapScopeRecord {
        inventory_id: inventory_id.to_string(),
        scope_identity,
        key_family: key_family.to_string(),
        pool_kind: pool_kind.to_string(),
        pool_value_i64,
        pool_value_hex,
        hash,
        nid_hex,
        owner_nid_hex,
        owner_family,
        owner_kind,
        owner_image_id,
        entry_count: record.entry_count,
        recognized_entry_count: record.recognized_entry_count,
    })
}

fn scope_identity(scope: &BlueStoreOmapScope) -> Result<String, CommandError> {
    let (family, pool_kind, pool_value_i64, pool_value_hex, hash, nid_hex) = scope_parts(scope);
    canonical_scope_identity(
        family,
        pool_kind,
        pool_value_i64,
        pool_value_hex.as_deref(),
        hash,
        &nid_hex,
    )
    .ok_or_else(|| CommandError::parser("BlueStore OMAP scope identity is not canonical"))
}

fn scope_parts(
    scope: &BlueStoreOmapScope,
) -> (
    &'static str,
    &'static str,
    Option<i64>,
    Option<String>,
    Option<u32>,
    String,
) {
    let (pool_kind, pool_value_i64, pool_value_hex) = match scope.pool {
        None => ("none", None, None),
        Some(BlueStoreOmapPoolScope::PerPool(value)) => ("perPool", Some(value), None),
        Some(BlueStoreOmapPoolScope::PerPg(value)) => ("perPg", None, Some(hex_u64(value))),
    };
    (
        family_name(scope.family),
        pool_kind,
        pool_value_i64,
        pool_value_hex,
        scope.hash,
        hex_u64(scope.nid),
    )
}

fn family_name(family: BlueStoreOmapKeyFamily) -> &'static str {
    match family {
        BlueStoreOmapKeyFamily::Bulk => "bulk",
        BlueStoreOmapKeyFamily::PgMeta => "pgMeta",
        BlueStoreOmapKeyFamily::PerPool => "perPool",
        BlueStoreOmapKeyFamily::PerPg => "perPg",
    }
}

fn hex_u64(value: u64) -> String {
    format!("{value:016x}")
}
