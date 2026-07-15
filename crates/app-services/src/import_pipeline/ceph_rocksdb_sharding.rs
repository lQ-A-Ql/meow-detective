use std::collections::{BTreeMap, BTreeSet};

use ceph_wire::BlueStoreOmapKeyFamily;
use persistence_sqlite::repositories::ceph_rocksdb_repo::CephRocksdbAggregate;
use transport::CommandError;

use super::ceph_bluefs_file_reader::BluefsExtentReader;
use super::ceph_bluefs_replay::{BluefsReplayFile, BluefsReplaySnapshot};

const SHARDING_DEFINITION_PATH: &str = "sharding/def";
const MAX_SHARDING_DEFINITION_BYTES: u64 = 16 * 1024;
const MAX_COLUMN_FAMILIES: usize = 256;
const MAX_SHARDS_PER_PREFIX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RocksdbShardingDefinition {
    routes: BTreeMap<String, RocksdbColumnFamilyRoute>,
    digest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RocksdbColumnFamilyRoute {
    pub(super) physical_name: String,
    pub(super) logical_prefix: Option<String>,
    pub(super) shard_index: Option<u32>,
    pub(super) hash_low: u32,
    pub(super) hash_high: u32,
    pub(super) strips_logical_prefix: bool,
}

impl RocksdbShardingDefinition {
    pub(super) fn route(&self, physical_name: &str) -> Option<&RocksdbColumnFamilyRoute> {
        self.routes.get(physical_name)
    }

    pub(super) fn route_omap_key<'a>(
        &self,
        physical_name: &str,
        user_key: &'a [u8],
    ) -> Result<Option<(BlueStoreOmapKeyFamily, &'a [u8])>, CommandError> {
        let route = self.route(physical_name).ok_or_else(|| {
            sharding_error(format!(
                "physical column family {physical_name} has no validated sharding route"
            ))
        })?;
        if !route.strips_logical_prefix {
            if route.physical_name != "default" || route.logical_prefix.is_some() {
                return Err(sharding_error(
                    "non-stripping OMAP route is not the canonical default column family",
                ));
            }
            let Some(prefix) = user_key.first().copied() else {
                return Ok(None);
            };
            let Some(family) = omap_family(prefix) else {
                return Ok(None);
            };
            if user_key.len() < 2 || user_key[1] != 0 {
                return Err(sharding_error("default OMAP key is not prefix-NUL encoded"));
            }
            return Ok(Some((family, &user_key[2..])));
        }

        let prefix = route
            .logical_prefix
            .as_deref()
            .ok_or_else(|| sharding_error("OMAP route has no logical prefix"))?;
        let bytes = prefix.as_bytes();
        if bytes.len() != 1 {
            return Ok(None);
        }
        Ok(omap_family(bytes[0]).map(|family| (family, user_key)))
    }

    pub(super) fn digest_sha256(&self) -> &str {
        &self.digest_sha256
    }

    pub(super) fn census_context(
        &self,
        physical_name: &str,
    ) -> Result<rocksdb_wire::KeySpaceCensusContext, CommandError> {
        let route = self.route(physical_name).ok_or_else(|| {
            sharding_error(format!(
                "column family {physical_name} has no validated sharding route"
            ))
        })?;
        if physical_name == "default" {
            return default_census_context(physical_name);
        }
        let logical_prefix = route
            .logical_prefix
            .as_deref()
            .ok_or_else(|| sharding_error("non-default sharding route has no logical prefix"))?;
        rocksdb_wire::KeySpaceCensusContext::single_bucket(
            physical_name,
            logical_bucket_name(logical_prefix),
            "bluestore.unknown",
        )
        .map_err(|error| sharding_error(format!("invalid SST census context: {error}")))
    }
}

fn default_census_context(
    physical_name: &str,
) -> Result<rocksdb_wire::KeySpaceCensusContext, CommandError> {
    let rules = [
        ("bluestore.super", b"S\0".as_slice()),
        ("bluestore.stat", b"T\0".as_slice()),
        ("bluestore.collection", b"C\0".as_slice()),
        ("bluestore.omap", b"M\0".as_slice()),
        ("bluestore.allocator", b"B\0".as_slice()),
        ("bluestore.allocator_bitmap", b"b\0".as_slice()),
        ("bluestore.shared_blob", b"X\0".as_slice()),
        ("bluestore.object", b"O\0".as_slice()),
        ("bluestore.omap.per_pool", b"m\0".as_slice()),
        ("bluestore.omap.per_pg", b"p\0".as_slice()),
        ("bluestore.deferred", b"L\0".as_slice()),
        ("bluestore.omap.pgmeta", b"P\0".as_slice()),
    ]
    .into_iter()
    .map(|(name, prefix)| rocksdb_wire::KeySpacePrefixRule::new(name, prefix.to_vec()))
    .collect::<Result<Vec<_>, _>>()
    .map_err(|error| sharding_error(format!("invalid default SST census rule: {error}")))?;
    rocksdb_wire::KeySpaceCensusContext::prefix_buckets(physical_name, "bluestore.unknown", rules)
        .map_err(|error| sharding_error(format!("invalid SST census context: {error}")))
}

fn logical_bucket_name(prefix: &str) -> &'static str {
    match prefix {
        "O" => "bluestore.object",
        "m" => "bluestore.omap.per_pool",
        "p" => "bluestore.omap.per_pg",
        "L" => "bluestore.deferred",
        "P" => "bluestore.omap.pgmeta",
        _ => "bluestore.unknown",
    }
}

fn omap_family(prefix: u8) -> Option<BlueStoreOmapKeyFamily> {
    match prefix {
        b'M' => Some(BlueStoreOmapKeyFamily::Bulk),
        b'P' => Some(BlueStoreOmapKeyFamily::PgMeta),
        b'm' => Some(BlueStoreOmapKeyFamily::PerPool),
        b'p' => Some(BlueStoreOmapKeyFamily::PerPg),
        _ => None,
    }
}

pub(super) fn read_rocksdb_sharding_definition(
    reader: &mut BluefsExtentReader<'_>,
    replay: &BluefsReplaySnapshot,
    rocksdb: &CephRocksdbAggregate,
) -> Result<RocksdbShardingDefinition, CommandError> {
    let file = required_file(replay)?;
    validate_file(file)?;
    let bytes = reader.read_plain_file(&file.fnode)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| sharding_error("sharding/def is not valid UTF-8"))?;
    let definition = parse_rocksdb_sharding_definition(text)?;
    validate_active_column_families(&definition, rocksdb)?;
    Ok(definition)
}

fn required_file(snapshot: &BluefsReplaySnapshot) -> Result<&BluefsReplayFile, CommandError> {
    let mut matches = snapshot
        .files
        .iter()
        .filter(|file| file.path == SHARDING_DEFINITION_PATH);
    let file = matches
        .next()
        .ok_or_else(|| sharding_error("BlueFS replay is missing the required sharding/def file"))?;
    if matches.next().is_some() {
        return Err(sharding_error(
            "BlueFS replay contains duplicate sharding/def files",
        ));
    }
    Ok(file)
}

fn validate_file(file: &BluefsReplayFile) -> Result<(), CommandError> {
    if file.fnode.encoding != 0
        || file.fnode.size == 0
        || file.fnode.size > MAX_SHARDING_DEFINITION_BYTES
    {
        return Err(sharding_error(format!(
            "sharding/def has unsupported encoding {} or size {}",
            file.fnode.encoding, file.fnode.size
        )));
    }
    Ok(())
}

pub(super) fn parse_rocksdb_sharding_definition(
    text: &str,
) -> Result<RocksdbShardingDefinition, CommandError> {
    if text.len() > MAX_SHARDING_DEFINITION_BYTES as usize
        || text.contains('\0')
        || text.trim() != text
    {
        return Err(sharding_error(
            "sharding/def is empty, oversized, contains NUL, or is not canonical",
        ));
    }
    let columns = split_column_definitions(text)?;
    if columns.len() > MAX_COLUMN_FAMILIES {
        return Err(sharding_error(
            "sharding/def column count is outside the supported range",
        ));
    }
    let mut routes = BTreeMap::from([(
        "default".to_string(),
        RocksdbColumnFamilyRoute {
            physical_name: "default".to_string(),
            logical_prefix: None,
            shard_index: None,
            hash_low: 0,
            hash_high: u32::MAX,
            strips_logical_prefix: false,
        },
    )]);
    for column in columns {
        let parsed = parse_column_definition(column)?;
        for route in parsed {
            if routes.insert(route.physical_name.clone(), route).is_some() {
                return Err(sharding_error(
                    "sharding/def expands to a duplicate physical column family",
                ));
            }
        }
    }
    Ok(RocksdbShardingDefinition {
        routes,
        digest_sha256: super::ceph_rocksdb_digest::sharding_sha256(text.as_bytes()),
    })
}

fn split_column_definitions(text: &str) -> Result<Vec<&str>, CommandError> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut columns = Vec::new();
    let mut start = 0usize;
    let mut braces = 0u32;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            byte if byte.is_ascii_whitespace() && byte != b' ' => {
                return Err(sharding_error(
                    "sharding/def contains non-canonical whitespace",
                ));
            }
            b'{' => braces = braces.saturating_add(1),
            b'}' => {
                braces = braces
                    .checked_sub(1)
                    .ok_or_else(|| sharding_error("sharding/def has an unmatched closing brace"))?;
            }
            b' ' if braces == 0 => {
                if start == index {
                    return Err(sharding_error(
                        "sharding/def has repeated or non-canonical whitespace",
                    ));
                }
                columns.push(&text[start..index]);
                start = index + 1;
            }
            byte if byte.is_ascii_control() || !byte.is_ascii() => {
                return Err(sharding_error(
                    "sharding/def contains unsupported control or non-ASCII bytes",
                ));
            }
            _ => {}
        }
    }
    if braces != 0 {
        return Err(sharding_error("sharding/def has an unclosed option brace"));
    }
    columns.push(&text[start..]);
    Ok(columns)
}

fn parse_column_definition(column: &str) -> Result<Vec<RocksdbColumnFamilyRoute>, CommandError> {
    let structural = column.split('=').next().unwrap_or_default();
    let (prefix, shape) = match structural.find('(') {
        Some(open) => {
            if !structural.ends_with(')') {
                return Err(sharding_error(
                    "sharding/def column shape is missing its closing parenthesis",
                ));
            }
            (
                &structural[..open],
                Some(&structural[open + 1..structural.len() - 1]),
            )
        }
        None => (structural, None),
    };
    validate_prefix(prefix)?;
    let (shards, hash_low, hash_high) =
        shape
            .map(parse_shape)
            .transpose()?
            .unwrap_or((1, 0, u32::MAX));
    (0..shards)
        .map(|index| {
            let physical_name = if shards == 1 {
                prefix.to_string()
            } else {
                format!("{prefix}-{index}")
            };
            Ok(RocksdbColumnFamilyRoute {
                physical_name,
                logical_prefix: Some(prefix.to_string()),
                shard_index: (shards > 1).then_some(index as u32),
                hash_low,
                hash_high,
                strips_logical_prefix: true,
            })
        })
        .collect()
}

fn validate_prefix(prefix: &str) -> Result<(), CommandError> {
    if prefix.is_empty()
        || prefix.len() > 32
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(sharding_error(
            "sharding/def contains an invalid logical prefix name",
        ));
    }
    Ok(())
}

fn parse_shape(shape: &str) -> Result<(usize, u32, u32), CommandError> {
    let (count_text, hash_text) = shape
        .split_once(',')
        .map_or((shape, None), |(count, hash)| (count, Some(hash)));
    let shards = parse_decimal::<usize>(count_text, "shard count")?;
    if shards == 0 || shards > MAX_SHARDS_PER_PREFIX {
        return Err(sharding_error(
            "sharding/def shard count is outside the supported range",
        ));
    }
    let (low, high) = hash_text
        .map(parse_hash_range)
        .transpose()?
        .unwrap_or((0, u32::MAX));
    if low > high {
        return Err(sharding_error("sharding/def hash range is reversed"));
    }
    Ok((shards, low, high))
}

fn parse_hash_range(value: &str) -> Result<(u32, u32), CommandError> {
    let (low, high) = value
        .split_once('-')
        .ok_or_else(|| sharding_error("sharding/def hash range is missing '-'"))?;
    let low = if low.is_empty() {
        0
    } else {
        parse_decimal(low, "hash lower bound")?
    };
    let high = if high.is_empty() {
        u32::MAX
    } else {
        parse_decimal(high, "hash upper bound")?
    };
    Ok((low, high))
}

fn parse_decimal<T>(value: &str, field: &'static str) -> Result<T, CommandError>
where
    T: std::str::FromStr,
{
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(sharding_error(format!("{field} is not canonical decimal")));
    }
    value
        .parse()
        .map_err(|_| sharding_error(format!("{field} overflows")))
}

fn validate_active_column_families(
    definition: &RocksdbShardingDefinition,
    rocksdb: &CephRocksdbAggregate,
) -> Result<(), CommandError> {
    let expected = definition.routes.keys().cloned().collect::<BTreeSet<_>>();
    let actual = rocksdb
        .column_families
        .iter()
        .filter(|column| !column.dropped)
        .map(|column| column.name.clone())
        .collect::<BTreeSet<_>>();
    if expected != actual {
        return Err(sharding_error(
            "sharding/def does not match the active MANIFEST column-family set",
        ));
    }
    Ok(())
}

fn sharding_error(error: impl std::fmt::Display) -> CommandError {
    CommandError::parser(format!("RocksDB sharding definition failed: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_sharding.rs"]
mod tests;
