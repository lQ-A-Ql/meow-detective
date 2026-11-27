use super::*;

impl Graph {
    pub(super) fn walk(
        &mut self,
        reader: &mut dyn EvidenceReader,
        location: u64,
        kind: Kind,
        lower: u64,
        upper: Option<u64>,
    ) -> Result<()> {
        if location == 0 || self.path.len() >= 64 {
            return Err(metadata_error(
                "thin space-map tree has invalid root or depth".into(),
            ));
        }
        let first = self.reference(location, kind)?;
        let block = read_metadata_block(reader, location)?;
        let value_size = match kind {
            Kind::Details => 24,
            Kind::DataIndex => 16,
            Kind::DataOverflow | Kind::MetadataOverflow => 4,
            _ => 8,
        };
        let header = parse_node_header(&block, location, value_size, self.path.is_empty())?;
        let keys = parse_node_keys(&block, &header)?;
        if keys.first().is_some_and(|key| *key < lower)
            || keys
                .last()
                .is_some_and(|key| upper.is_some_and(|upper| *key >= upper))
            || (!header.is_leaf && keys.is_empty())
        {
            return Err(metadata_error(
                "thin space-map tree violates parent key bounds".into(),
            ));
        }
        if !first {
            return Ok(());
        }
        self.path.push(location);
        for (index, key) in keys.iter().copied().enumerate() {
            if !header.is_leaf {
                self.walk(
                    reader,
                    parse_internal_child(&block, &header, index)?,
                    kind,
                    key,
                    keys.get(index + 1).copied().or(upper),
                )?;
            } else {
                let offset =
                    values_offset(header.max_entries)? + index * header.value_size as usize;
                if kind == Kind::Roots {
                    self.walk(
                        reader,
                        read_u64(&block, offset, "thin mapping tree")?,
                        Kind::Mappings,
                        0,
                        None,
                    )?;
                } else if kind == Kind::Mappings {
                    self.push_mapping(read_u64(&block, offset, "thin mapping")? >> 24)?;
                }
            }
        }
        self.path.pop();
        Ok(())
    }

    pub(super) fn entries(
        &mut self,
        reader: &mut dyn EvidenceReader,
        root: u64,
        kind: Kind,
        size: u32,
    ) -> Result<Vec<(u64, Vec<u8>)>> {
        self.walk(reader, root, kind, 0, None)?;
        let mut output = Vec::new();
        self.collect(reader, root, size, &mut output)?;
        Ok(output)
    }

    fn collect(
        &mut self,
        reader: &mut dyn EvidenceReader,
        root: u64,
        size: u32,
        output: &mut Vec<(u64, Vec<u8>)>,
    ) -> Result<()> {
        if root == 0 || self.path.contains(&root) || self.path.len() >= 64 {
            return Err(metadata_error(
                "thin space-map enumeration has invalid depth or cycle".into(),
            ));
        }
        self.path.push(root);
        let block = read_metadata_block(reader, root)?;
        let header = parse_node_header(&block, root, size, true)?;
        let keys = parse_node_keys(&block, &header)?;
        for (index, key) in keys.iter().copied().enumerate() {
            if header.is_leaf {
                self.charge(128 + 2 * u64::from(size))?;
                let offset = values_offset(header.max_entries)? + index * size as usize;
                output.push((key, block[offset..offset + size as usize].to_vec()));
            } else {
                self.collect(
                    reader,
                    parse_internal_child(&block, &header, index)?,
                    size,
                    output,
                )?;
            }
        }
        self.path.pop();
        Ok(())
    }

    pub(super) fn overflow(
        &mut self,
        reader: &mut dyn EvidenceReader,
        root: u64,
        kind: Kind,
    ) -> Result<Vec<(u64, u32)>> {
        self.entries(reader, root, kind, 4)?
            .into_iter()
            .map(|(key, value)| {
                let count = read_u32(&value, 0, "thin overflow count")?;
                if count < 3 {
                    return Err(metadata_error("thin overflow count is below three".into()));
                }
                Ok((key, count))
            })
            .collect()
    }
}
