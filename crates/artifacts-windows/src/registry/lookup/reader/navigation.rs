use super::cells::allocated_cell_length;
use super::RegistryHiveReader;
use crate::registry::lookup::types::{
    NkRecord, RegistryValue, INVALID_OFFSET, MAX_KEY_LOOKUP_DEPTH,
};
use crate::registry::lookup::utf16::{read_u16, read_u32};

impl RegistryHiveReader<'_> {
    pub(crate) fn lookup_value(
        &self,
        key_path: &[&str],
        value_name: &str,
    ) -> Result<Option<RegistryValue>, String> {
        let key = self.navigate_to(key_path)?;
        match key {
            Some(key) => self.read_value(&key, value_name),
            None => Ok(None),
        }
    }

    pub(crate) fn navigate_to(&self, key_path: &[&str]) -> Result<Option<NkRecord>, String> {
        validate_key_path_depth(key_path)?;
        let mut key = self.parse_nk(self.root_cell_offset)?;
        for segment in key_path {
            let Some(next_offset) = self.find_subkey_offset(&key, segment)? else {
                return Ok(None);
            };
            key = self.parse_nk(next_offset)?;
        }
        Ok(Some(key))
    }

    pub(crate) fn control_set_candidates(&self, warnings: &mut Vec<String>) -> Vec<String> {
        let mut candidates = Vec::new();
        match self.lookup_value(&["Select"], "Current") {
            Ok(Some(RegistryValue::Dword(value))) if (1..=999).contains(&value) => {
                candidates.push(format!("ControlSet{value:03}"));
            }
            Ok(Some(value)) => warnings.push(format!(
                "Select\\Current has unsupported type: {:?}; falling back to common ControlSet names",
                value
            )),
            Ok(None) => warnings
                .push("Select\\Current not found; falling back to common ControlSet names".to_string()),
            Err(error) => warnings.push(format!(
                "Select\\Current parse error: {error}; falling back to common ControlSet names"
            )),
        }
        for fallback in ["ControlSet001", "ControlSet002", "CurrentControlSet"] {
            if !candidates
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(fallback))
            {
                candidates.push(fallback.to_string());
            }
        }
        candidates
    }

    pub(crate) fn read_subkeys_from_nk(
        &self,
        key: &NkRecord,
    ) -> Result<Vec<(String, NkRecord)>, String> {
        Ok(self
            .subkey_offsets(key)?
            .into_iter()
            .filter_map(|offset| self.parse_nk(offset).ok())
            .map(|child| (child.name.clone(), child))
            .collect())
    }

    pub(crate) fn read_subkey_names_from_nk(&self, key: &NkRecord) -> Result<Vec<String>, String> {
        Ok(self
            .subkey_offsets(key)?
            .into_iter()
            .filter_map(|offset| self.parse_nk(offset).ok())
            .map(|child| child.name)
            .collect())
    }

    pub(super) fn find_subkey_offset(
        &self,
        key: &NkRecord,
        wanted: &str,
    ) -> Result<Option<u32>, String> {
        for offset in self.subkey_offsets(key)? {
            match self.parse_nk(offset) {
                Ok(child) if child.name.eq_ignore_ascii_case(wanted) => return Ok(Some(offset)),
                _ => {}
            }
        }
        Ok(None)
    }

    fn subkey_offsets(&self, key: &NkRecord) -> Result<Vec<u32>, String> {
        if key.num_subkeys == 0 || key.subkeys_list_offset == INVALID_OFFSET {
            return Ok(Vec::new());
        }
        self.read_subkey_offsets(key.subkeys_list_offset, 0)
    }

    fn read_subkey_offsets(&self, list_offset: u32, depth: u8) -> Result<Vec<u32>, String> {
        if depth > 8 {
            return Err("registry subkey list nesting too deep".to_string());
        }
        let absolute = self.abs(list_offset)?;
        let cell_length = allocated_cell_length(self, absolute, "subkey list")?;
        self.require(absolute, cell_length)?;
        let signature = self
            .bytes
            .get(absolute + 4..absolute + 6)
            .ok_or_else(|| "subkey list signature out of bounds".to_string())?;
        let count = read_u16(self.bytes, absolute + 6)? as usize;
        match signature {
            b"lf" | b"lh" => self.read_hashed_offsets(absolute, count),
            b"li" => self.read_index_offsets(absolute, count),
            b"ri" => self.read_indirect_offsets(absolute, count, depth),
            _ => Err(format!(
                "unsupported subkey list signature {}",
                String::from_utf8_lossy(signature)
            )),
        }
    }

    fn read_hashed_offsets(&self, absolute: usize, count: usize) -> Result<Vec<u32>, String> {
        let mut offsets = Vec::new();
        for index in 0..count {
            let entry = absolute + 8 + index * 8;
            self.require(entry, 8)?;
            let primary = read_u32(self.bytes, entry)?;
            let legacy = read_u32(self.bytes, entry + 4)?;
            offsets.push(primary);
            if legacy != primary {
                offsets.push(legacy);
            }
        }
        Ok(offsets)
    }

    fn read_index_offsets(&self, absolute: usize, count: usize) -> Result<Vec<u32>, String> {
        (0..count)
            .map(|index| {
                read_u32(self.bytes, absolute + 8 + index * 4).map_err(|error| error.to_string())
            })
            .collect()
    }

    fn read_indirect_offsets(
        &self,
        absolute: usize,
        count: usize,
        depth: u8,
    ) -> Result<Vec<u32>, String> {
        let mut offsets = Vec::new();
        for index in 0..count {
            let child = read_u32(self.bytes, absolute + 8 + index * 4)?;
            offsets.extend(self.read_subkey_offsets(child, depth + 1)?);
        }
        Ok(offsets)
    }
}

fn validate_key_path_depth(path: &[&str]) -> Result<(), String> {
    (path.len() <= MAX_KEY_LOOKUP_DEPTH)
        .then_some(())
        .ok_or_else(|| {
            format!(
                "registry key path depth {} exceeds limit {}",
                path.len(),
                MAX_KEY_LOOKUP_DEPTH
            )
        })
}
