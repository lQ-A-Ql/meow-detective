use super::*;

pub(super) fn validate_checksum(
    block: &[u8; METADATA_BLOCK_SIZE],
    location: u64,
    salt: u32,
) -> Result<()> {
    let expected = read_u32(block, 0, "thin checksum")?;
    let actual = !crc32c::crc32c(&block[4..]) ^ salt;
    if expected != actual {
        return Err(LvmError::ThinChecksum {
            block: location,
            expected,
            actual,
        });
    }
    Ok(())
}

pub(super) fn validate_superblock(block: &[u8; METADATA_BLOCK_SIZE]) -> Result<()> {
    validate_checksum(block, 0, 160_774)?;
    let version = read_u32(block, 40, "thin version")?;
    if version != 2 {
        return Err(LvmError::ThinVersion { version });
    }
    let flags = read_u32(block, 4, "thin flags")?;
    if flags != 0 {
        return Err(LvmError::ThinFlags { flags });
    }
    let flags = read_u32(block, 360, "thin incompatible features")?;
    if flags != 0 {
        return Err(LvmError::ThinFeatures { flags });
    }
    Ok(())
}

pub(super) fn validate_trees(
    reader: &mut dyn EvidenceReader,
    superblock: &ThinSuperblock,
) -> Result<()> {
    let mut mapping_devices = 0u64;
    walk(
        reader,
        superblock,
        superblock.mapping_root,
        U64_VALUE_SIZE,
        &mut |reader, id, value| {
            mapping_devices = checked_add(mapping_devices, 1, "thin device count")?;
            let detail = btree_lookup_device_detail(reader, superblock.details_root, id)?
                .ok_or_else(|| metadata_error(format!("thin device {id} has no details")))?;
            validate_device_detail(superblock, id, detail)?;
            let root = parse_u64_value(value)?;
            let mut count = 0u64;
            walk(
                reader,
                superblock,
                root,
                BLOCK_TIME_VALUE_SIZE,
                &mut |_, _, value| {
                    let mapping = parse_block_time_value(value)?;
                    if mapping.block >= superblock.nr_data_blocks || mapping.time > superblock.time
                    {
                        return Err(metadata_error(format!(
                            "thin device {id} mapping exceeds committed data/time boundary"
                        )));
                    }
                    count = checked_add(count, 1, "thin mapped block count")?;
                    Ok(())
                },
            )?;
            if count != detail.mapped_blocks {
                return Err(metadata_error(format!(
                    "thin device {id} mapping count {count} differs from detail {}",
                    detail.mapped_blocks
                )));
            }
            Ok(())
        },
    )?;
    let mut detail_devices = 0u64;
    walk(
        reader,
        superblock,
        superblock.details_root,
        DEVICE_DETAIL_VALUE_SIZE,
        &mut |_, id, value| {
            validate_device_detail(superblock, id, parse_device_detail_value(value)?)?;
            detail_devices = checked_add(detail_devices, 1, "thin detail device count")?;
            Ok(())
        },
    )?;
    if detail_devices != mapping_devices {
        return Err(metadata_error(
            "thin mapping/details device sets differ".into(),
        ));
    }
    Ok(())
}

fn validate_device_detail(
    superblock: &ThinSuperblock,
    id: u64,
    detail: DeviceDetail,
) -> Result<()> {
    if id >= 1 << 24
        || detail.creation_time > superblock.time
        || detail.snapshotted_time > superblock.time
    {
        return Err(metadata_error(format!(
            "thin device {id} exceeds committed device/time boundary"
        )));
    }
    if detail.transaction_id > superblock.transaction_id {
        return Err(LvmError::ThinDeviceTransaction {
            device_id: id,
            actual: detail.transaction_id,
            maximum: superblock.transaction_id,
        });
    }
    Ok(())
}

type Visitor<'a> = dyn FnMut(&mut dyn EvidenceReader, u64, &[u8]) -> Result<()> + 'a;

pub(super) fn validate_device_capacity(
    reader: &mut dyn EvidenceReader,
    superblock: &ThinSuperblock,
    root: u64,
    logical_blocks: u64,
) -> Result<()> {
    walk(
        reader,
        superblock,
        root,
        BLOCK_TIME_VALUE_SIZE,
        &mut |_, key, _| {
            if key >= logical_blocks {
                return Err(metadata_error(format!(
                    "thin mapped block {key} exceeds logical capacity {logical_blocks}"
                )));
            }
            Ok(())
        },
    )
}

fn walk(
    reader: &mut dyn EvidenceReader,
    superblock: &ThinSuperblock,
    root: u64,
    value_size: u32,
    visitor: &mut Visitor<'_>,
) -> Result<()> {
    let mut tree = TreeWalk {
        superblock,
        value_size,
        path: Vec::new(),
        visitor,
    };
    tree.visit(reader, root, 0, None)
}

struct TreeWalk<'a, 'b> {
    superblock: &'a ThinSuperblock,
    value_size: u32,
    path: Vec<u64>,
    visitor: &'a mut Visitor<'b>,
}

impl TreeWalk<'_, '_> {
    fn visit(
        &mut self,
        reader: &mut dyn EvidenceReader,
        loc: u64,
        lower: u64,
        upper: Option<u64>,
    ) -> Result<()> {
        if loc == 0
            || loc >= self.superblock.nr_metadata_blocks
            || self.path.contains(&loc)
            || self.path.len() >= 64
        {
            return Err(metadata_error(format!(
                "thin tree block {loc} exceeds bounds or contains a cycle"
            )));
        }
        let block = read_metadata_block(reader, loc)?;
        let header = parse_node_header(&block, loc, self.value_size, self.path.is_empty())?;
        let keys = parse_node_keys(&block, &header)?;
        if keys.first().is_some_and(|key| *key < lower)
            || keys
                .last()
                .is_some_and(|key| upper.is_some_and(|upper| *key >= upper))
            || (!header.is_leaf && keys.is_empty())
        {
            return Err(metadata_error(format!(
                "thin tree block {loc} violates parent key bounds"
            )));
        }
        self.path.push(loc);
        for (index, key) in keys.iter().copied().enumerate() {
            if header.is_leaf {
                let start = values_offset(header.max_entries)? + index * header.value_size as usize;
                (self.visitor)(
                    reader,
                    key,
                    &block[start..start + header.value_size as usize],
                )?;
            } else {
                let child = parse_internal_child(&block, &header, index)?;
                self.visit(reader, child, key, keys.get(index + 1).copied().or(upper))?;
            }
        }
        self.path.pop();
        Ok(())
    }
}
