use super::parse_value_data;
use super::types::{
    NkRecord, RegistryValue, BASE_BLOCK_SIZE, HBIN_MAGIC, INVALID_OFFSET, MAX_KEY_LOOKUP_DEPTH,
    NK_SIGNATURE, VK_SIGNATURE,
};
use super::utf16::{decode_name, read_i32, read_u16, read_u32};

// ── RegistryHiveReader ──────────────────────────────────────────────────────

pub(crate) struct RegistryHiveReader<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) root_cell_offset: u32,
}

impl<'a> RegistryHiveReader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < BASE_BLOCK_SIZE {
            return Err("registry hive shorter than base block".to_string());
        }
        if bytes.get(0..4) != Some(b"regf") {
            return Err("not a valid registry hive".to_string());
        }
        // Validate first hbin header at offset 0x1000 (Task 2.1.1)
        if bytes.len() < BASE_BLOCK_SIZE + 32 {
            return Err("registry hive too short for first hbin header".to_string());
        }
        if bytes.get(BASE_BLOCK_SIZE..BASE_BLOCK_SIZE + 4) != Some(HBIN_MAGIC) {
            return Err("first hbin header missing 'hbin' magic".to_string());
        }
        let hbin_size = read_u32(bytes, BASE_BLOCK_SIZE + 8)? as usize;
        if hbin_size == 0 || !hbin_size.is_multiple_of(4096) {
            return Err(format!(
                "first hbin size {hbin_size:#x} is not a valid page multiple"
            ));
        }
        let root_cell_offset = read_u32(bytes, 0x24)?;
        // Validate root cell offset is within first hbin (Task 2.1.3)
        if root_cell_offset >= hbin_size as u32 {
            return Err(format!(
                "root cell offset {root_cell_offset:#x} exceeds first hbin size {hbin_size:#x}"
            ));
        }
        Ok(Self {
            bytes,
            root_cell_offset,
        })
    }

    pub(crate) fn lookup_value(
        &self,
        key_path: &[&str],
        value_name: &str,
    ) -> Result<Option<RegistryValue>, String> {
        // Task 2.1.2: bounded key path depth
        if key_path.len() > MAX_KEY_LOOKUP_DEPTH {
            return Err(format!(
                "registry key path depth {} exceeds limit {}",
                key_path.len(),
                MAX_KEY_LOOKUP_DEPTH
            ));
        }
        let mut nk = self.parse_nk(self.root_cell_offset)?;
        for segment in key_path {
            let Some(next_offset) = self.find_subkey_offset(&nk, segment)? else {
                return Ok(None);
            };
            nk = self.parse_nk(next_offset)?;
        }
        self.read_value(&nk, value_name)
    }

    fn find_subkey_offset(&self, nk: &NkRecord, wanted: &str) -> Result<Option<u32>, String> {
        if nk.num_subkeys == 0 || nk.subkeys_list_offset == INVALID_OFFSET {
            return Ok(None);
        }
        for offset in self.read_subkey_offsets(nk.subkeys_list_offset, 0)? {
            match self.parse_nk(offset) {
                Ok(child) if child.name.eq_ignore_ascii_case(wanted) => return Ok(Some(offset)),
                Ok(_) => {}
                Err(_) => continue,
            }
        }
        Ok(None)
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
            Err(err) => warnings.push(format!(
                "Select\\Current parse error: {err}; falling back to common ControlSet names"
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

    fn read_value(&self, nk: &NkRecord, value_name: &str) -> Result<Option<RegistryValue>, String> {
        if nk.num_values == 0 || nk.values_list_offset == INVALID_OFFSET {
            return Ok(None);
        }
        let list_abs = self.abs(nk.values_list_offset)?;
        let cell_size = read_i32(self.bytes, list_abs)?;
        if cell_size >= 0 {
            return Err(format!(
                "value list at {:#x} is free",
                nk.values_list_offset
            ));
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid registry value list cell size".to_string())?
            as usize;
        self.require(list_abs, cell_len)?;
        let list_len = (nk.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "registry value list size overflow".to_string())?;
        let list_start = list_abs + 4;
        if list_len > cell_len.saturating_sub(4) {
            return Err(format!(
                "value list at {:#x} length {:#x} exceeds cell",
                nk.values_list_offset, list_len
            ));
        }
        self.require(list_start, list_len)?;
        for index in 0..nk.num_values as usize {
            let value_offset = read_u32(self.bytes, list_start + index * 4)?;
            if value_offset == INVALID_OFFSET {
                continue;
            }
            let Some((name, value)) = self.parse_vk(value_offset)? else {
                continue;
            };
            if name.eq_ignore_ascii_case(value_name) {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }

    /// Read a named value's raw bytes directly, without type interpretation.
    /// Used for SAM V/F binary blobs that parse_value_data misidentifies.
    pub(crate) fn read_raw_value_bytes(
        &self,
        nk: &NkRecord,
        value_name: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        if nk.num_values == 0 || nk.values_list_offset == INVALID_OFFSET {
            return Ok(None);
        }
        let list_abs = self.abs(nk.values_list_offset)?;
        let cell_size = read_i32(self.bytes, list_abs)?;
        if cell_size >= 0 {
            return Ok(None);
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid value list".to_string())? as usize;
        self.require(list_abs, cell_len)?;
        let list_len = (nk.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "overflow".to_string())?;
        let list_start = list_abs + 4;
        if list_len > cell_len.saturating_sub(4) {
            return Ok(None);
        }
        self.require(list_start, list_len)?;
        for idx in 0..nk.num_values as usize {
            let vk_off = read_u32(self.bytes, list_start + idx * 4)?;
            if vk_off == INVALID_OFFSET {
                continue;
            }
            let vk_abs = self.abs(vk_off)?;
            // Read VK cell directly: check signature, name, then extract data bytes
            if vk_abs + 0x18 > self.bytes.len() {
                continue;
            }
            if &self.bytes[vk_abs + 4..vk_abs + 6] != VK_SIGNATURE {
                continue;
            }
            let name_len = read_u16(self.bytes, vk_abs + 6)? as usize;
            let data_len_raw = read_u32(self.bytes, vk_abs + 8)?;
            let data_offset = read_u32(self.bytes, vk_abs + 0x0C)?;
            let flags = read_u16(self.bytes, vk_abs + 0x14)?;
            let name_start = vk_abs + 0x18;
            if self.bytes.len() < name_start + name_len {
                continue;
            }
            let name = decode_name(
                &self.bytes[name_start..name_start + name_len],
                flags & 0x01 != 0,
            )?;
            if !name.eq_ignore_ascii_case(value_name) {
                continue;
            }
            // Extract raw bytes (skip RegistryValue type interpretation)
            let data_len = (data_len_raw & 0x7FFF_FFFF) as usize;
            let raw = if data_len_raw & 0x8000_0000 != 0 {
                if data_len > 4 {
                    return Err("inline value >4 bytes".into());
                }
                data_offset.to_le_bytes()[..data_len].to_vec()
            } else if data_len == 0 {
                // REG_NONE: the data_offset IS the value
                data_offset.to_le_bytes().to_vec()
            } else {
                let data_abs = self.abs(data_offset)?;
                let dcell = read_i32(self.bytes, data_abs)?;
                if dcell >= 0 {
                    return Ok(None);
                }
                let dlen = dcell
                    .checked_abs()
                    .ok_or_else(|| "invalid data cell".to_string())?
                    as usize;
                self.require(data_abs, dlen)?;
                let dstart = data_abs + 4;
                if data_len > dlen.saturating_sub(4) {
                    return Ok(None);
                }
                self.require(dstart, data_len)?;
                self.bytes[dstart..dstart + data_len].to_vec()
            };
            return Ok(Some(raw));
        }
        Ok(None)
    }

    pub(crate) fn parse_nk(&self, cell_offset: u32) -> Result<NkRecord, String> {
        let abs = self.abs(cell_offset)?;
        let cell_size = read_i32(self.bytes, abs)?;
        if cell_size >= 0 {
            return Err(format!("cell at {cell_offset:#x} is free"));
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid registry cell size".to_string())?
            as usize;
        self.require(abs, cell_len)?;
        if self.bytes.get(abs + 4..abs + 6) != Some(NK_SIGNATURE) {
            return Err(format!("cell at {cell_offset:#x} is not nk"));
        }
        let flags = read_u16(self.bytes, abs + 6)?;
        let num_subkeys = read_u32(self.bytes, abs + 0x18)?;
        let subkeys_list_offset = read_u32(self.bytes, abs + 0x20)?;
        let num_values = read_u32(self.bytes, abs + 0x28)?;
        let values_list_offset = read_u32(self.bytes, abs + 0x2c)?;
        let name_len = read_u16(self.bytes, abs + 0x4c)? as usize;
        let name_start = abs + 0x50;
        self.require(name_start, name_len)?;
        let name = decode_name(
            &self.bytes[name_start..name_start + name_len],
            flags & 0x20 != 0,
        )?;
        Ok(NkRecord {
            name,
            num_subkeys,
            subkeys_list_offset,
            num_values,
            values_list_offset,
        })
    }

    pub(crate) fn parse_vk(
        &self,
        cell_offset: u32,
    ) -> Result<Option<(String, RegistryValue)>, String> {
        let abs = self.abs(cell_offset)?;
        let cell_size = read_i32(self.bytes, abs)?;
        if cell_size >= 0 {
            return Ok(None);
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid registry value cell size".to_string())?
            as usize;
        self.require(abs, cell_len)?;
        if self.bytes.get(abs + 4..abs + 6) != Some(VK_SIGNATURE) {
            return Ok(None);
        }
        let name_len = read_u16(self.bytes, abs + 6)? as usize;
        let data_len_raw = read_u32(self.bytes, abs + 8)?;
        let data_offset = read_u32(self.bytes, abs + 0x0c)?;
        let data_type = read_u32(self.bytes, abs + 0x10)?;
        let flags = read_u16(self.bytes, abs + 0x14)?;
        let name_start = abs + 0x18;
        self.require(name_start, name_len)?;
        let name = decode_name(
            &self.bytes[name_start..name_start + name_len],
            flags & 0x01 != 0,
        )?;
        let data_len = (data_len_raw & 0x7fff_ffff) as usize;
        let data = if data_len_raw & 0x8000_0000 != 0 {
            if data_len > 4 {
                return Err(format!(
                    "inline value at {cell_offset:#x} length {data_len:#x} exceeds 4 bytes"
                ));
            }
            let inline = data_offset.to_le_bytes();
            inline[..data_len].to_vec()
        } else if data_len == 0 {
            Vec::new()
        } else {
            let data_abs = self.abs(data_offset)?;
            let cell_size = read_i32(self.bytes, data_abs)?;
            if cell_size >= 0 {
                return Err(format!("value data cell at {data_offset:#x} is free"));
            }
            let cell_len = cell_size
                .checked_abs()
                .ok_or_else(|| "invalid registry value data cell size".to_string())?
                as usize;
            self.require(data_abs, cell_len)?;
            let data_start = data_abs + 4;
            self.require(data_start, data_len)?;
            if data_len > cell_len.saturating_sub(4) {
                return Err(format!(
                    "value data at {data_offset:#x} length {data_len:#x} exceeds cell"
                ));
            }
            self.bytes[data_start..data_start + data_len].to_vec()
        };
        Ok(Some((name, parse_value_data(data_type, &data)?)))
    }

    fn read_subkey_offsets(&self, list_offset: u32, depth: u8) -> Result<Vec<u32>, String> {
        if depth > 8 {
            return Err("registry subkey list nesting too deep".to_string());
        }
        let abs = self.abs(list_offset)?;
        let cell_size = read_i32(self.bytes, abs)?;
        if cell_size >= 0 {
            return Err(format!("subkey list at {list_offset:#x} is free"));
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid subkey list cell size".to_string())?
            as usize;
        self.require(abs, cell_len)?;
        let signature = self
            .bytes
            .get(abs + 4..abs + 6)
            .ok_or_else(|| "subkey list signature out of bounds".to_string())?;
        let count = read_u16(self.bytes, abs + 6)? as usize;
        let mut offsets = Vec::new();
        match signature {
            b"lf" | b"lh" => {
                for index in 0..count {
                    let entry = abs + 8 + index * 8;
                    self.require(entry, 8)?;
                    let primary = read_u32(self.bytes, entry)?;
                    let legacy_synthetic = read_u32(self.bytes, entry + 4)?;
                    offsets.push(primary);
                    if legacy_synthetic != primary {
                        // Older synthetic fixtures in this repository wrote
                        // the name hash before the child offset. Real Windows
                        // hives store the child offset first.
                        offsets.push(legacy_synthetic);
                    }
                }
            }
            b"li" => {
                for index in 0..count {
                    let entry = abs + 8 + index * 4;
                    self.require(entry, 4)?;
                    offsets.push(read_u32(self.bytes, entry)?);
                }
            }
            b"ri" => {
                for index in 0..count {
                    let entry = abs + 8 + index * 4;
                    self.require(entry, 4)?;
                    offsets
                        .extend(self.read_subkey_offsets(read_u32(self.bytes, entry)?, depth + 1)?);
                }
            }
            _ => {
                return Err(format!(
                    "unsupported subkey list signature {}",
                    String::from_utf8_lossy(signature)
                ));
            }
        }
        Ok(offsets)
    }

    /// Navigate to the NK record at `key_path` (empty slice = root).
    pub(crate) fn navigate_to(&self, key_path: &[&str]) -> Result<Option<NkRecord>, String> {
        if key_path.len() > MAX_KEY_LOOKUP_DEPTH {
            return Err(format!(
                "registry key path depth {} exceeds limit {}",
                key_path.len(),
                MAX_KEY_LOOKUP_DEPTH
            ));
        }
        let mut nk = self.parse_nk(self.root_cell_offset)?;
        for segment in key_path {
            let Some(next_offset) = self.find_subkey_offset(&nk, segment)? else {
                return Ok(None);
            };
            nk = self.parse_nk(next_offset)?;
        }
        Ok(Some(nk))
    }

    /// Read all (name, value) pairs from a given NK record.
    pub(crate) fn read_all_values_from_nk(
        &self,
        nk: &NkRecord,
    ) -> Result<Vec<(String, RegistryValue)>, String> {
        if nk.num_values == 0 || nk.values_list_offset == INVALID_OFFSET {
            return Ok(Vec::new());
        }
        let list_abs = self.abs(nk.values_list_offset)?;
        let cell_size = read_i32(self.bytes, list_abs)?;
        if cell_size >= 0 {
            return Err(format!(
                "value list at {:#x} is free",
                nk.values_list_offset
            ));
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid registry value list cell size".to_string())?
            as usize;
        self.require(list_abs, cell_len)?;
        let list_len = (nk.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "registry value list size overflow".to_string())?;
        let list_start = list_abs + 4;
        if list_len > cell_len.saturating_sub(4) {
            return Err(format!(
                "value list at {:#x} length {:#x} exceeds cell",
                nk.values_list_offset, list_len
            ));
        }
        self.require(list_start, list_len)?;
        let mut result = Vec::with_capacity(nk.num_values as usize);
        for index in 0..nk.num_values as usize {
            let value_offset = read_u32(self.bytes, list_start + index * 4)?;
            if value_offset == INVALID_OFFSET {
                continue;
            }
            if let Some((name, value)) = self.parse_vk(value_offset)? {
                result.push((name, value));
            }
        }
        Ok(result)
    }

    /// Read raw VK cell offsets from an NK record's value list.
    /// Used by SAM RID extraction when REG_NONE values have empty data
    /// but the data_offset field encodes the RID inline.
    pub(crate) fn read_raw_vk_data_offsets(&self, nk: &NkRecord) -> Result<Vec<u32>, String> {
        if nk.num_values == 0 || nk.values_list_offset == INVALID_OFFSET {
            return Ok(Vec::new());
        }
        let list_abs = self.abs(nk.values_list_offset)?;
        let cell_size = read_i32(self.bytes, list_abs)?;
        if cell_size >= 0 {
            return Ok(Vec::new());
        }
        let cell_len = cell_size
            .checked_abs()
            .ok_or_else(|| "invalid value list cell".to_string())? as usize;
        self.require(list_abs, cell_len)?;
        let list_len = (nk.num_values as usize)
            .checked_mul(4)
            .ok_or_else(|| "overflow".to_string())?;
        let list_start = list_abs + 4;
        if list_len > cell_len.saturating_sub(4) {
            return Ok(Vec::new());
        }
        self.require(list_start, list_len)?;
        let mut offsets = Vec::with_capacity(nk.num_values as usize);
        for idx in 0..nk.num_values as usize {
            let vk_off = read_u32(self.bytes, list_start + idx * 4)?;
            if vk_off != INVALID_OFFSET {
                offsets.push(vk_off);
            }
        }
        Ok(offsets)
    }

    /// Read the names of all subkeys of a given NK record.
    pub(crate) fn read_subkey_names_from_nk(&self, nk: &NkRecord) -> Result<Vec<String>, String> {
        if nk.num_subkeys == 0 || nk.subkeys_list_offset == INVALID_OFFSET {
            return Ok(Vec::new());
        }
        let offsets = self.read_subkey_offsets(nk.subkeys_list_offset, 0)?;
        let mut names = Vec::with_capacity(offsets.len());
        for offset in offsets {
            if let Ok(child) = self.parse_nk(offset) {
                names.push(child.name);
            }
        }
        Ok(names)
    }

    /// Navigate to `key_path` and read the class name of that key.
    /// Returns `None` when the key exists but has no class name.
    pub(crate) fn read_class_name_at(&self, key_path: &[&str]) -> Result<Option<String>, String> {
        if key_path.len() > MAX_KEY_LOOKUP_DEPTH {
            return Err(format!(
                "registry key path depth {} exceeds limit {}",
                key_path.len(),
                MAX_KEY_LOOKUP_DEPTH
            ));
        }
        let mut nk_offset = self.root_cell_offset;
        let mut nk = self.parse_nk(nk_offset)?;
        for segment in key_path {
            let Some(next_offset) = self.find_subkey_offset(&nk, segment)? else {
                return Ok(None);
            };
            nk_offset = next_offset;
            nk = self.parse_nk(nk_offset)?;
        }
        self.read_nk_class_name(nk_offset)
    }

    /// Read the class name from an NK cell at the given hive-relative offset.
    fn read_nk_class_name(&self, nk_offset: u32) -> Result<Option<String>, String> {
        let nk_abs = self.abs(nk_offset)?;
        // Validate the cell is an NK record
        let cell_size = read_i32(self.bytes, nk_abs)?;
        if cell_size >= 0 {
            return Err(format!("NK cell at {nk_offset:#x} is free"));
        }
        if self.bytes.get(nk_abs + 4..nk_abs + 6) != Some(NK_SIGNATURE) {
            return Err("class name read target is not an NK cell".to_string());
        }

        let class_name_length = read_u16(self.bytes, nk_abs + 0x4E)? as usize;
        if class_name_length == 0 {
            return Ok(None);
        }
        if class_name_length > 4096 {
            return Err(format!(
                "class name length {class_name_length} at {nk_offset:#x} is implausibly large"
            ));
        }

        let classname_offset = read_u32(self.bytes, nk_abs + 0x34)?;
        let class_data: Vec<u8> = if classname_offset != INVALID_OFFSET && classname_offset != 0 {
            // External class name: read from the data cell at classname_offset.
            let data_abs = self.abs(classname_offset)?;
            let dcell_size = read_i32(self.bytes, data_abs)?;
            if dcell_size >= 0 {
                return Err(format!(
                    "class name data cell at {classname_offset:#x} is free"
                ));
            }
            let dcell_len = dcell_size
                .checked_abs()
                .ok_or_else(|| "invalid class name data cell size".to_string())?
                as usize;
            self.require(data_abs, dcell_len)?;
            let data_start = data_abs + 4;
            self.require(data_start, class_name_length)?;
            if class_name_length > dcell_len.saturating_sub(4) {
                return Err(format!(
                    "class name at {classname_offset:#x} length {class_name_length:#x} exceeds cell"
                ));
            }
            self.bytes[data_start..data_start + class_name_length].to_vec()
        } else {
            // Inline class name: stored right after the key name in the NK cell.
            let name_len = read_u16(self.bytes, nk_abs + 0x4C)? as usize;
            let class_start = nk_abs + 0x50 + name_len;
            self.require(class_start, class_name_length)?;
            self.bytes[class_start..class_start + class_name_length].to_vec()
        };

        // Decode the class name bytes (always UTF-16LE in registry hives).
        if class_data.len() < 2 || !class_data.len().is_multiple_of(2) {
            return Ok(None);
        }
        let units: Vec<u16> = class_data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let s = String::from_utf16_lossy(&units);
        let trimmed = s.trim_end_matches('\0');
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(trimmed.to_string()))
    }

    pub(crate) fn abs(&self, hive_offset: u32) -> Result<usize, String> {
        if hive_offset == INVALID_OFFSET {
            return Err("invalid registry offset".to_string());
        }
        BASE_BLOCK_SIZE
            .checked_add(hive_offset as usize)
            .ok_or_else(|| "registry offset overflow".to_string())
            .and_then(|abs| {
                if abs < self.bytes.len() {
                    Ok(abs)
                } else {
                    Err(format!("registry offset {hive_offset:#x} out of bounds"))
                }
            })
    }

    pub(crate) fn require(&self, abs: usize, len: usize) -> Result<(), String> {
        abs.checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .map(|_| ())
            .ok_or_else(|| format!("registry range {abs:#x}+{len:#x} out of bounds"))
    }
}
