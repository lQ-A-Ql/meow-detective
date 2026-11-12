pub fn is_bitlocker_partition(partition: &crate::partition_query_service::PartitionRecord) -> bool {
    partition.kind_label.eq_ignore_ascii_case("bitlocker")
        || partition
            .filesystem
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("bitlocker"))
        || matches!(
            partition.status.to_ascii_lowercase().as_str(),
            "locked" | "encrypted_bitlocker"
        )
}

#[cfg(test)]
#[path = "../tests/unit/partition_capabilities.rs"]
mod tests;
