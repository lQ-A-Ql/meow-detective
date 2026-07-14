use transport::CommandError;

use super::ceph_rocksdb_sharding::RocksdbShardingDefinition;

pub(super) fn full_merge(
    sharding: &RocksdbShardingDefinition,
    column_family_name: &str,
    user_key: &[u8],
    existing_value: Option<&[u8]>,
    operands: &[&[u8]],
) -> Result<Vec<u8>, CommandError> {
    let operator = resolve_operator(sharding, column_family_name, user_key)?;
    match operator {
        CephMergeOperator::Int64Array => merge_int64_array(existing_value, operands),
        CephMergeOperator::BitwiseXor => merge_bitwise_xor(existing_value, operands),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CephMergeOperator {
    Int64Array,
    BitwiseXor,
}

fn resolve_operator(
    sharding: &RocksdbShardingDefinition,
    column_family_name: &str,
    user_key: &[u8],
) -> Result<CephMergeOperator, CommandError> {
    let logical_prefix = if column_family_name == "default" {
        default_key_prefix(user_key)
    } else {
        sharding
            .route(column_family_name)
            .and_then(|route| route.logical_prefix.as_deref())
    };
    match logical_prefix {
        Some("T") => Ok(CephMergeOperator::Int64Array),
        Some("b") => Ok(CephMergeOperator::BitwiseXor),
        _ => Err(CommandError::unsupported(format!(
            "RocksDB merge operand for column family {column_family_name} has no approved Ceph merge operator"
        ))),
    }
}

fn default_key_prefix(user_key: &[u8]) -> Option<&str> {
    let separator = user_key.iter().position(|byte| *byte == 0)?;
    std::str::from_utf8(&user_key[..separator]).ok()
}

fn merge_int64_array(
    existing_value: Option<&[u8]>,
    operands: &[&[u8]],
) -> Result<Vec<u8>, CommandError> {
    merge_associative(existing_value, operands, |left, right| {
        if left.len() != right.len() || right.len() % 8 != 0 {
            return Err(merge_error(
                "Ceph int64_array operands must have equal 8-byte-aligned lengths",
            ));
        }
        let mut merged = vec![0; right.len()];
        for ((left, right), output) in left
            .chunks_exact(8)
            .zip(right.chunks_exact(8))
            .zip(merged.chunks_exact_mut(8))
        {
            let left = u64::from_le_bytes(
                left.try_into()
                    .map_err(|_| merge_error("invalid int64_array left operand width"))?,
            );
            let right = u64::from_le_bytes(
                right
                    .try_into()
                    .map_err(|_| merge_error("invalid int64_array right operand width"))?,
            );
            output.copy_from_slice(&left.wrapping_add(right).to_le_bytes());
        }
        Ok(merged)
    })
}

fn merge_bitwise_xor(
    existing_value: Option<&[u8]>,
    operands: &[&[u8]],
) -> Result<Vec<u8>, CommandError> {
    merge_associative(existing_value, operands, |left, right| {
        if left.len() != right.len() {
            return Err(merge_error(
                "Ceph bitwise_xor operands must have equal lengths",
            ));
        }
        Ok(left
            .iter()
            .zip(right)
            .map(|(left, right)| left ^ right)
            .collect())
    })
}

fn merge_associative(
    existing_value: Option<&[u8]>,
    operands: &[&[u8]],
    mut merge: impl FnMut(&[u8], &[u8]) -> Result<Vec<u8>, CommandError>,
) -> Result<Vec<u8>, CommandError> {
    let mut current = existing_value.map(ToOwned::to_owned);
    for operand in operands {
        current = Some(match current {
            Some(existing) => merge(&existing, operand)?,
            None => operand.to_vec(),
        });
    }
    current.ok_or_else(|| merge_error("Ceph merge operator received no value or operands"))
}

fn merge_error(message: impl Into<String>) -> CommandError {
    CommandError::parser(format!("Ceph RocksDB merge failed: {}", message.into()))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_merge.rs"]
mod tests;
