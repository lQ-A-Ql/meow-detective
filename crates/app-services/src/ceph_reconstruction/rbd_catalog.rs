use ceph_wire::RbdImageMetadata;
use persistence_sqlite::repositories::ceph_bluestore_omap_repo::{
    CephBluestoreOmapAggregate, CephBluestoreRbdHeaderRecord,
};
use thiserror::Error;

use super::rbd_reader::RbdReadContext;

#[derive(Debug, Error)]
pub enum RbdCatalogError {
    #[error("RBD OMAP aggregate has no directory mapping for image {image_id}")]
    MissingDirectoryMapping { image_id: String },
    #[error("RBD OMAP header is missing {field} for image {image_id}")]
    MissingField {
        image_id: String,
        field: &'static str,
    },
    #[error("RBD OMAP header field {field} is invalid for image {image_id}")]
    InvalidField {
        image_id: String,
        field: &'static str,
    },
    #[error("RBD OMAP header has no data pool for image {image_id}")]
    MissingDataPool { image_id: String },
    #[error("RBD OMAP header scope is not present for image {image_id}")]
    MissingScope { image_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbdImageDescriptor {
    pub metadata: RbdImageMetadata,
    pub scope_identity: String,
    pub context: RbdReadContext,
}

pub fn discover_rbd_images(
    aggregate: &CephBluestoreOmapAggregate,
) -> Result<Vec<RbdImageDescriptor>, RbdCatalogError> {
    aggregate
        .rbd_headers
        .iter()
        .map(|header| discover_image(aggregate, header))
        .collect()
}

fn discover_image(
    aggregate: &CephBluestoreOmapAggregate,
    header: &CephBluestoreRbdHeaderRecord,
) -> Result<RbdImageDescriptor, RbdCatalogError> {
    let image_id = header.image_id.clone();
    let mapping = aggregate
        .directory_mappings
        .iter()
        .find(|mapping| mapping.image_id == image_id)
        .ok_or_else(|| RbdCatalogError::MissingDirectoryMapping {
            image_id: image_id.clone(),
        })?;
    let scope = aggregate
        .scopes
        .iter()
        .find(|scope| scope.scope_identity == header.scope_identity)
        .ok_or_else(|| RbdCatalogError::MissingScope {
            image_id: image_id.clone(),
        })?;
    let data_pool_id = header
        .data_pool_id
        .or_else(|| {
            scope
                .pool_value_i64
                .filter(|_| scope.pool_kind == "perPool")
        })
        .ok_or_else(|| RbdCatalogError::MissingDataPool {
            image_id: image_id.clone(),
        })?;
    let metadata = RbdImageMetadata {
        name: mapping.image_name.clone(),
        id: image_id.clone(),
        object_prefix: required_text(header, header.object_prefix.as_deref(), "object_prefix")?,
        image_size: required_u64(header, header.size_hex.as_deref(), "size")?,
        order: header
            .object_order
            .ok_or_else(|| missing_field(&image_id, "order"))?,
        features: required_u64(header, header.features_hex.as_deref(), "features")?,
        stripe_unit: optional_u64(&image_id, header.stripe_unit_hex.as_deref(), "stripe_unit")?,
        stripe_count: optional_u64(
            &image_id,
            header.stripe_count_hex.as_deref(),
            "stripe_count",
        )?,
        data_pool_id,
    };
    Ok(RbdImageDescriptor {
        metadata,
        scope_identity: header.scope_identity.clone(),
        context: RbdReadContext {
            operation_features: 0,
            has_parent: false,
            snapshot_id: None,
            encrypted: false,
        },
    })
}

fn required_u64(
    header: &CephBluestoreRbdHeaderRecord,
    value: Option<&str>,
    field: &'static str,
) -> Result<u64, RbdCatalogError> {
    let value = value.ok_or_else(|| missing_field(&header.image_id, field))?;
    parse_hex(&header.image_id, field, value)
}

fn optional_u64(
    image_id: &str,
    value: Option<&str>,
    field: &'static str,
) -> Result<u64, RbdCatalogError> {
    value
        .map(|value| parse_hex(image_id, field, value))
        .unwrap_or(Ok(0))
}

fn required_text(
    header: &CephBluestoreRbdHeaderRecord,
    value: Option<&str>,
    field: &'static str,
) -> Result<String, RbdCatalogError> {
    let value = value.ok_or_else(|| missing_field(&header.image_id, field))?;
    if value.is_empty() || value.contains('\0') {
        return Err(RbdCatalogError::InvalidField {
            image_id: header.image_id.clone(),
            field,
        });
    }
    Ok(value.to_string())
}

fn parse_hex(image_id: &str, field: &'static str, value: &str) -> Result<u64, RbdCatalogError> {
    if value.len() != 16
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(RbdCatalogError::InvalidField {
            image_id: image_id.to_string(),
            field,
        });
    }
    u64::from_str_radix(value, 16).map_err(|_| RbdCatalogError::InvalidField {
        image_id: image_id.to_string(),
        field,
    })
}

fn missing_field(image_id: &str, field: &'static str) -> RbdCatalogError {
    RbdCatalogError::MissingField {
        image_id: image_id.to_string(),
        field,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/rbd_catalog.rs"]
mod tests;
