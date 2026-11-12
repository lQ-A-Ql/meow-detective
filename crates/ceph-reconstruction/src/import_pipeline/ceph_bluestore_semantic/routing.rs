use ceph_wire::BlueStoreKeySpace;
use transport::CommandError;

use super::super::ceph_rocksdb_sharding::{RocksdbColumnFamilyRoute, RocksdbShardingDefinition};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::import_pipeline) struct RoutedBlueStoreKey<'a> {
    pub(super) key_space: BlueStoreKeySpace,
    pub(super) logical_key: &'a [u8],
}

pub(in crate::import_pipeline) fn route_bluestore_key<'a>(
    sharding: &RocksdbShardingDefinition,
    physical_column_family: &str,
    user_key: &'a [u8],
) -> Result<Option<RoutedBlueStoreKey<'a>>, CommandError> {
    let route = sharding
        .route(physical_column_family)
        .ok_or_else(|| routing_error("physical column family has no validated sharding route"))?;
    if route.strips_logical_prefix {
        route_dedicated_key(route, user_key)
    } else {
        route_default_key(route, user_key)
    }
}

fn route_default_key<'a>(
    route: &RocksdbColumnFamilyRoute,
    user_key: &'a [u8],
) -> Result<Option<RoutedBlueStoreKey<'a>>, CommandError> {
    if route.physical_name != "default"
        || route.logical_prefix.is_some()
        || route.shard_index.is_some()
    {
        return Err(routing_error(
            "non-stripping route is not the canonical default column family",
        ));
    }
    if user_key.len() < 2 || user_key[1] != 0 {
        return Err(routing_error(
            "default column-family key is not prefix-NUL encoded",
        ));
    }
    Ok(
        key_space_from_prefix_byte(user_key[0]).map(|key_space| RoutedBlueStoreKey {
            key_space,
            logical_key: &user_key[2..],
        }),
    )
}

fn route_dedicated_key<'a>(
    route: &RocksdbColumnFamilyRoute,
    user_key: &'a [u8],
) -> Result<Option<RoutedBlueStoreKey<'a>>, CommandError> {
    let prefix = route
        .logical_prefix
        .as_deref()
        .ok_or_else(|| routing_error("dedicated column family has no logical prefix"))?;
    let bytes = prefix.as_bytes();
    if bytes.len() != 1 {
        return Ok(None);
    }
    Ok(
        key_space_from_prefix_byte(bytes[0]).map(|key_space| RoutedBlueStoreKey {
            key_space,
            logical_key: user_key,
        }),
    )
}

fn key_space_from_prefix_byte(prefix: u8) -> Option<BlueStoreKeySpace> {
    match prefix {
        b'S' => Some(BlueStoreKeySpace::Super),
        b'C' => Some(BlueStoreKeySpace::Collection),
        b'O' => Some(BlueStoreKeySpace::Object),
        b'X' => Some(BlueStoreKeySpace::SharedBlob),
        _ => None,
    }
}

fn routing_error(message: impl Into<String>) -> CommandError {
    CommandError::parser(format!(
        "BlueStore semantic key routing failed: {}",
        message.into()
    ))
}

#[cfg(test)]
#[path = "../../../tests/unit/import_pipeline/ceph_bluestore_semantic_routing.rs"]
mod tests;
