use chrono::{TimeZone, Utc};

const BASE_BLOCK_SIZE: usize = 0x1000;
const NK_SIGNATURE: &[u8; 2] = b"nk";
const VK_SIGNATURE: &[u8; 2] = b"vk";
const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
const REG_DWORD: u32 = 4;
const REG_MULTI_SZ: u32 = 7;
const REG_QWORD: u32 = 11;
const INVALID_OFFSET: u32 = 0xFFFF_FFFF;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRegistryField {
    pub value: String,
    pub hive_path: String,
    pub key_path: String,
    pub value_name: String,
    pub parser: String,
}

#[derive(Debug, Clone, Default)]
pub struct SystemHiveInfo {
    pub computer_name: Option<ParsedRegistryField>,
    pub timezone: Option<ParsedRegistryField>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SoftwareHiveInfo {
    pub product_name: Option<ParsedRegistryField>,
    pub current_build: Option<ParsedRegistryField>,
    pub current_version: Option<ParsedRegistryField>,
    pub display_version: Option<ParsedRegistryField>,
    pub install_date: Option<ParsedRegistryField>,
    pub registered_owner: Option<ParsedRegistryField>,
    pub registered_organization: Option<ParsedRegistryField>,
    pub product_id: Option<ParsedRegistryField>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RegistryValue {
    String(String),
    Dword(u32),
    Qword(u64),
    MultiString(Vec<String>),
    Binary(Vec<u8>),
}

#[derive(Debug, Clone)]
struct NkRecord {
    name: String,
    num_subkeys: u32,
    subkeys_list_offset: u32,
    num_values: u32,
    values_list_offset: u32,
}

pub fn extract_system_hive_fields(bytes: &[u8], hive_path: &str) -> Result<SystemHiveInfo, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = SystemHiveInfo::default();
    let control_sets = hive.control_set_candidates(&mut info.warnings);

    for control_set in control_sets {
        let computer_key = [
            control_set.as_str(),
            "Control",
            "ComputerName",
            "ComputerName",
        ];
        if info.computer_name.is_none() {
            info.computer_name = lookup_string_field(
                &hive,
                hive_path,
                "registry.system",
                &computer_key,
                "ComputerName",
                &mut info.warnings,
            );
        }

        let timezone_key = [control_set.as_str(), "Control", "TimeZoneInformation"];
        if info.timezone.is_none() {
            info.timezone = lookup_string_field(
                &hive,
                hive_path,
                "registry.system",
                &timezone_key,
                "TimeZoneKeyName",
                &mut info.warnings,
            )
            .or_else(|| {
                lookup_string_field(
                    &hive,
                    hive_path,
                    "registry.system",
                    &timezone_key,
                    "StandardName",
                    &mut info.warnings,
                )
            });
        }

        if info.computer_name.is_some() && info.timezone.is_some() {
            break;
        }
    }
    Ok(info)
}

pub fn extract_software_hive_fields(
    bytes: &[u8],
    hive_path: &str,
) -> Result<SoftwareHiveInfo, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let key = ["Microsoft", "Windows NT", "CurrentVersion"];
    let mut info = SoftwareHiveInfo::default();

    info.product_name = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "ProductName",
        &mut info.warnings,
    );
    info.current_build = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "CurrentBuild",
        &mut info.warnings,
    );
    info.current_version = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "CurrentVersion",
        &mut info.warnings,
    );
    info.display_version = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "DisplayVersion",
        &mut info.warnings,
    )
    .or_else(|| {
        lookup_string_field(
            &hive,
            hive_path,
            "registry.software",
            &key,
            "ReleaseId",
            &mut info.warnings,
        )
    });
    info.registered_owner = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "RegisteredOwner",
        &mut info.warnings,
    );
    info.registered_organization = lookup_optional_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "RegisteredOrganization",
        &mut info.warnings,
    );
    info.product_id = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "ProductId",
        &mut info.warnings,
    );
    info.install_date = lookup_install_date_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        &mut info.warnings,
    );

    Ok(info)
}

fn lookup_string_field(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    key_path: &[&str],
    value_name: &str,
    warnings: &mut Vec<String>,
) -> Option<ParsedRegistryField> {
    match hive.lookup_value(key_path, value_name) {
        Ok(Some(RegistryValue::String(value))) if !value.trim().is_empty() => {
            Some(ParsedRegistryField {
                value,
                hive_path: hive_path.to_string(),
                key_path: key_path.join("\\"),
                value_name: value_name.to_string(),
                parser: parser.to_string(),
            })
        }
        Ok(Some(other)) => {
            warnings.push(format!(
                "{}\\{} has unsupported type: {:?}",
                key_path.join("\\"),
                value_name,
                other
            ));
            None
        }
        Ok(None) => {
            warnings.push(format!("{}\\{} not found", key_path.join("\\"), value_name));
            None
        }
        Err(err) => {
            warnings.push(format!(
                "{}\\{} parse error: {}",
                key_path.join("\\"),
                value_name,
                err
            ));
            None
        }
    }
}

fn lookup_optional_string_field(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    key_path: &[&str],
    value_name: &str,
    warnings: &mut Vec<String>,
) -> Option<ParsedRegistryField> {
    match hive.lookup_value(key_path, value_name) {
        Ok(None) => None,
        _ => lookup_string_field(hive, hive_path, parser, key_path, value_name, warnings),
    }
}

fn lookup_install_date_field(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    key_path: &[&str],
    warnings: &mut Vec<String>,
) -> Option<ParsedRegistryField> {
    match hive.lookup_value(key_path, "InstallDate") {
        Ok(Some(RegistryValue::Dword(value))) => {
            let Some(dt) = Utc.timestamp_opt(value as i64, 0).single() else {
                warnings.push("InstallDate is outside supported timestamp range".to_string());
                return None;
            };
            if !(946_684_800..=4_102_444_800).contains(&value) {
                warnings.push(format!("InstallDate {value} is outside plausible range"));
                return None;
            }
            Some(ParsedRegistryField {
                value: dt.to_rfc3339(),
                hive_path: hive_path.to_string(),
                key_path: key_path.join("\\"),
                value_name: "InstallDate".to_string(),
                parser: parser.to_string(),
            })
        }
        Ok(Some(other)) => {
            warnings.push(format!("InstallDate has unsupported type: {:?}", other));
            None
        }
        Ok(None) => {
            warnings.push(format!("{}\\InstallDate not found", key_path.join("\\")));
            None
        }
        Err(err) => {
            warnings.push(format!(
                "{}\\InstallDate parse error: {}",
                key_path.join("\\"),
                err
            ));
            None
        }
    }
}

struct RegistryHiveReader<'a> {
    bytes: &'a [u8],
    root_cell_offset: u32,
}

impl<'a> RegistryHiveReader<'a> {
    fn new(bytes: &'a [u8]) -> Result<Self, String> {
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

    fn lookup_value(
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

    fn control_set_candidates(&self, warnings: &mut Vec<String>) -> Vec<String> {
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

    fn parse_nk(&self, cell_offset: u32) -> Result<NkRecord, String> {
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

    fn parse_vk(&self, cell_offset: u32) -> Result<Option<(String, RegistryValue)>, String> {
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

    fn abs(&self, hive_offset: u32) -> Result<usize, String> {
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

    fn require(&self, abs: usize, len: usize) -> Result<(), String> {
        abs.checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .map(|_| ())
            .ok_or_else(|| format!("registry range {abs:#x}+{len:#x} out of bounds"))
    }
}

fn parse_value_data(data_type: u32, data: &[u8]) -> Result<RegistryValue, String> {
    match data_type {
        REG_SZ | REG_EXPAND_SZ => Ok(RegistryValue::String(decode_utf16_until_nul(data)?)),
        REG_DWORD => Ok(RegistryValue::Dword(
            read_le_array::<4>(data)
                .map(u32::from_le_bytes)
                .ok_or_else(|| "REG_DWORD value shorter than 4 bytes".to_string())?,
        )),
        REG_QWORD => Ok(RegistryValue::Qword(
            read_le_array::<8>(data)
                .map(u64::from_le_bytes)
                .ok_or_else(|| "REG_QWORD value shorter than 8 bytes".to_string())?,
        )),
        REG_MULTI_SZ => Ok(RegistryValue::MultiString(
            decode_utf16_full(data)?
                .split('\0')
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect(),
        )),
        _ => Ok(RegistryValue::Binary(data.to_vec())),
    }
}

fn decode_name(bytes: &[u8], compressed: bool) -> Result<String, String> {
    if compressed {
        return String::from_utf8(bytes.to_vec()).map_err(|err| err.to_string());
    }
    decode_utf16_full(bytes)
}

fn decode_utf16_until_nul(bytes: &[u8]) -> Result<String, String> {
    let mut decoded = decode_utf16_full(bytes)?;
    if let Some(index) = decoded.find('\0') {
        decoded.truncate(index);
    }
    Ok(decoded)
}

fn decode_utf16_full(bytes: &[u8]) -> Result<String, String> {
    if !bytes.len().is_multiple_of(2) {
        return Err("UTF-16 data has odd byte length".to_string());
    }
    let mut units = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        units.push(unit);
    }
    Ok(String::from_utf16_lossy(&units))
}

fn read_le_array<const N: usize>(bytes: &[u8]) -> Option<[u8; N]> {
    bytes.get(..N)?.try_into().ok()
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| format!("u16 at {offset:#x} out of bounds"))?
            .try_into()
            .map_err(|_| "invalid u16".to_string())?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| format!("u32 at {offset:#x} out of bounds"))?
            .try_into()
            .map_err(|_| "invalid u32".to_string())?,
    ))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, String> {
    Ok(i32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| format!("i32 at {offset:#x} out of bounds"))?
            .try_into()
            .map_err(|_| "invalid i32".to_string())?,
    ))
}

const HBIN_MAGIC: &[u8; 4] = b"hbin";
const MAX_KEY_LOOKUP_DEPTH: usize = 64;

#[cfg(test)]
mod tests {
    use super::*;
    use testing::{builders::registry as registry_fixture, fixtures};

    fn empty_hive(root_name: &str) -> Vec<u8> {
        let mut data = vec![0u8; 0x8000];
        data[0..4].copy_from_slice(b"regf");
        data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
        data[0x1000..0x1004].copy_from_slice(b"hbin");
        data[0x1008..0x100c].copy_from_slice(&0x2000u32.to_le_bytes());
        write_nk(&mut data, 0x20, root_name, &[], &[]);
        data
    }

    fn write_nk(data: &mut [u8], offset: u32, name: &str, subkeys: &[(&str, u32)], values: &[u32]) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        let name_bytes = name.as_bytes();
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"nk");
        data[abs + 6..abs + 8].copy_from_slice(&0x20u16.to_le_bytes());
        data[abs + 0x18..abs + 0x1c].copy_from_slice(&(subkeys.len() as u32).to_le_bytes());
        let subkey_list_offset = 0x2000 + offset;
        let value_list_offset = 0x4000 + offset;
        data[abs + 0x20..abs + 0x24].copy_from_slice(
            &if subkeys.is_empty() {
                INVALID_OFFSET
            } else {
                subkey_list_offset
            }
            .to_le_bytes(),
        );
        data[abs + 0x28..abs + 0x2c].copy_from_slice(&(values.len() as u32).to_le_bytes());
        data[abs + 0x2c..abs + 0x30].copy_from_slice(
            &if values.is_empty() {
                INVALID_OFFSET
            } else {
                value_list_offset
            }
            .to_le_bytes(),
        );
        data[abs + 0x4c..abs + 0x4e].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        data[abs + 0x50..abs + 0x50 + name_bytes.len()].copy_from_slice(name_bytes);

        if !values.is_empty() {
            let list_abs = BASE_BLOCK_SIZE + value_list_offset as usize;
            data[list_abs..list_abs + 4]
                .copy_from_slice(&(-((values.len() as i32 * 4) + 4)).to_le_bytes());
            for (index, value_offset) in values.iter().enumerate() {
                let entry = list_abs + 4 + index * 4;
                data[entry..entry + 4].copy_from_slice(&value_offset.to_le_bytes());
            }
        }

        if !subkeys.is_empty() {
            write_hashed_subkey_list(data, subkey_list_offset, b"lf", subkeys);
        }
    }

    fn write_nk_utf16_name(data: &mut [u8], offset: u32, name: &str) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        let name_bytes: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"nk");
        data[abs + 6..abs + 8].copy_from_slice(&0u16.to_le_bytes());
        data[abs + 0x20..abs + 0x24].copy_from_slice(&INVALID_OFFSET.to_le_bytes());
        data[abs + 0x2c..abs + 0x30].copy_from_slice(&INVALID_OFFSET.to_le_bytes());
        data[abs + 0x4c..abs + 0x4e].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        data[abs + 0x50..abs + 0x50 + name_bytes.len()].copy_from_slice(&name_bytes);
    }

    fn write_hashed_subkey_list(
        data: &mut [u8],
        offset: u32,
        signature: &[u8; 2],
        subkeys: &[(&str, u32)],
    ) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(signature);
        data[abs + 6..abs + 8].copy_from_slice(&(subkeys.len() as u16).to_le_bytes());
        for (index, (name, child_offset)) in subkeys.iter().enumerate() {
            let entry = abs + 8 + index * 8;
            let mut hash = [0u8; 4];
            for (idx, byte) in name.as_bytes().iter().take(4).enumerate() {
                hash[idx] = *byte;
            }
            data[entry..entry + 4].copy_from_slice(&hash);
            data[entry + 4..entry + 8].copy_from_slice(&child_offset.to_le_bytes());
        }
    }

    fn write_flat_subkey_list(data: &mut [u8], offset: u32, signature: &[u8; 2], subkeys: &[u32]) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(signature);
        data[abs + 6..abs + 8].copy_from_slice(&(subkeys.len() as u16).to_le_bytes());
        for (index, child_offset) in subkeys.iter().enumerate() {
            let entry = abs + 8 + index * 4;
            data[entry..entry + 4].copy_from_slice(&child_offset.to_le_bytes());
        }
    }

    fn set_nk_subkey_list(data: &mut [u8], nk_offset: u32, list_offset: u32, count: u32) {
        let abs = BASE_BLOCK_SIZE + nk_offset as usize;
        data[abs + 0x18..abs + 0x1c].copy_from_slice(&count.to_le_bytes());
        data[abs + 0x20..abs + 0x24].copy_from_slice(&list_offset.to_le_bytes());
    }

    fn write_string_value(data: &mut [u8], offset: u32, name: &str, value: &str, data_offset: u32) {
        write_typed_string_value(data, offset, name, REG_SZ, value, data_offset);
    }

    fn write_typed_string_value(
        data: &mut [u8],
        offset: u32,
        name: &str,
        value_type: u32,
        value: &str,
        data_offset: u32,
    ) {
        let encoded: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let data_abs = BASE_BLOCK_SIZE + data_offset as usize;
        data[data_abs..data_abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[data_abs + 4..data_abs + 4 + encoded.len()].copy_from_slice(&encoded);
        write_vk(
            data,
            offset,
            name,
            value_type,
            encoded.len() as u32,
            data_offset,
        );
    }

    fn write_multi_string_value(
        data: &mut [u8],
        offset: u32,
        name: &str,
        values: &[&str],
        data_offset: u32,
    ) {
        let mut encoded = Vec::new();
        for value in values {
            encoded.extend(value.encode_utf16().flat_map(u16::to_le_bytes));
            encoded.extend(0u16.to_le_bytes());
        }
        encoded.extend(0u16.to_le_bytes());
        let data_abs = BASE_BLOCK_SIZE + data_offset as usize;
        data[data_abs..data_abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[data_abs + 4..data_abs + 4 + encoded.len()].copy_from_slice(&encoded);
        write_vk(
            data,
            offset,
            name,
            REG_MULTI_SZ,
            encoded.len() as u32,
            data_offset,
        );
    }

    fn write_dword_value(data: &mut [u8], offset: u32, name: &str, value: u32) {
        write_vk(data, offset, name, REG_DWORD, 0x8000_0004, value);
    }

    fn write_qword_value(data: &mut [u8], offset: u32, name: &str, value: u64, data_offset: u32) {
        let data_abs = BASE_BLOCK_SIZE + data_offset as usize;
        data[data_abs..data_abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[data_abs + 4..data_abs + 12].copy_from_slice(&value.to_le_bytes());
        write_vk(data, offset, name, REG_QWORD, 8, data_offset);
    }

    fn write_vk(
        data: &mut [u8],
        offset: u32,
        name: &str,
        value_type: u32,
        data_len: u32,
        data_offset: u32,
    ) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        let name_bytes = name.as_bytes();
        data[abs..abs + 4].copy_from_slice(&(-128i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"vk");
        data[abs + 6..abs + 8].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        data[abs + 8..abs + 12].copy_from_slice(&data_len.to_le_bytes());
        data[abs + 12..abs + 16].copy_from_slice(&data_offset.to_le_bytes());
        data[abs + 16..abs + 20].copy_from_slice(&value_type.to_le_bytes());
        data[abs + 20..abs + 22].copy_from_slice(&1u16.to_le_bytes());
        data[abs + 0x18..abs + 0x18 + name_bytes.len()].copy_from_slice(name_bytes);
    }

    #[test]
    fn reject_non_regf() {
        assert!(RegistryHiveReader::new(b"not-registry").is_err());
    }

    #[test]
    fn reject_missing_hbin_magic() {
        let mut data = empty_hive("ROOT");
        // Corrupt the hbin magic at 0x1000
        data[0x1000..0x1004].copy_from_slice(b"NOPE");
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn reject_zero_hbin_size() {
        let mut data = empty_hive("ROOT");
        // Set hbin size to 0
        data[0x1008..0x100c].copy_from_slice(&0u32.to_le_bytes());
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn reject_non_page_aligned_hbin_size() {
        let mut data = empty_hive("ROOT");
        // Set hbin size to a non-page-aligned value
        data[0x1008..0x100c].copy_from_slice(&0x1234u32.to_le_bytes());
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn reject_truncated_before_hbin() {
        // Hive with regf but truncated before hbin
        let mut data = vec![0u8; 0x1010];
        data[0..4].copy_from_slice(b"regf");
        data[0x24..0x28].copy_from_slice(&0x20u32.to_le_bytes());
        // No hbin at 0x1000 (all zeros)
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn reject_root_cell_offset_exceeds_hbin() {
        let mut data = empty_hive("ROOT");
        // Set root cell offset beyond hbin size (0x2000)
        data[0x24..0x28].copy_from_slice(&0x3000u32.to_le_bytes());
        assert!(RegistryHiveReader::new(&data).is_err());
    }

    #[test]
    fn key_path_depth_exceeds_limit() {
        let data = empty_hive("ROOT");
        let hive = RegistryHiveReader::new(&data).unwrap();
        // Build a key path with 65 segments (exceeds MAX_KEY_LOOKUP_DEPTH = 64)
        let deep_path: Vec<&str> = (0..65).map(|_| "x").collect();
        let err = hive.lookup_value(&deep_path, "val").unwrap_err();
        assert!(err.contains("depth"));
    }

    #[test]
    fn key_path_depth_at_limit_is_allowed() {
        let data = empty_hive("ROOT");
        let hive = RegistryHiveReader::new(&data).unwrap();
        // 64 segments should not be rejected by depth check (will fail on lookup)
        let path: Vec<&str> = (0..64).map(|_| "x").collect();
        // This returns Ok(None) because keys don't exist, but no depth error
        assert!(hive.lookup_value(&path, "val").is_ok());
    }

    #[test]
    fn parse_base_block_regf() {
        let data = empty_hive("SYSTEM");
        assert_eq!(
            RegistryHiveReader::new(&data).unwrap().root_cell_offset,
            0x20
        );
    }

    #[test]
    fn parse_nk_compressed_name() {
        let data = empty_hive("SYSTEM");
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert_eq!(hive.parse_nk(0x20).unwrap().name, "SYSTEM");
    }

    #[test]
    fn parse_nk_utf16_name() {
        let mut data = empty_hive("ROOT");
        write_nk_utf16_name(&mut data, 0x20, "SYST\u{00c8}M");
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(hive.parse_nk(0x20).unwrap().name, "SYST\u{00c8}M");
    }

    #[test]
    fn read_subkeys_lf_and_vk_string() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[("Child", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_subkeys_lh_list() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        set_nk_subkey_list(&mut data, 0x20, 0x2020, 1);
        write_hashed_subkey_list(&mut data, 0x2020, b"lh", &[("Child", 0x200)]);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_subkeys_lf_offset_first_real_layout() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        set_nk_subkey_list(&mut data, 0x20, 0x2020, 1);
        let abs = BASE_BLOCK_SIZE + 0x2020;
        data[abs..abs + 4].copy_from_slice(&(-256i32).to_le_bytes());
        data[abs + 4..abs + 6].copy_from_slice(b"lf");
        data[abs + 6..abs + 8].copy_from_slice(&1u16.to_le_bytes());
        data[abs + 8..abs + 12].copy_from_slice(&0x200u32.to_le_bytes());
        data[abs + 12..abs + 16].copy_from_slice(b"Chil");
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_subkeys_li_list() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        set_nk_subkey_list(&mut data, 0x20, 0x2020, 1);
        write_flat_subkey_list(&mut data, 0x2020, b"li", &[0x200]);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_subkeys_ri_list() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[]);
        write_nk(&mut data, 0x200, "Child", &[], &[0x400]);
        write_string_value(&mut data, 0x400, "Name", "Value", 0x700);
        set_nk_subkey_list(&mut data, 0x20, 0x2020, 1);
        write_flat_subkey_list(&mut data, 0x2020, b"ri", &[0x2080]);
        write_flat_subkey_list(&mut data, 0x2080, b"li", &[0x200]);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&["Child"], "Name").unwrap(),
            Some(RegistryValue::String("Value".to_string()))
        );
    }

    #[test]
    fn read_vk_reg_dword_inline() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_dword_value(&mut data, 0x400, "Current", 1);
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert_eq!(
            hive.lookup_value(&[], "Current").unwrap(),
            Some(RegistryValue::Dword(1))
        );
    }

    #[test]
    fn read_vk_reg_expand_sz() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_typed_string_value(
            &mut data,
            0x400,
            "Path",
            REG_EXPAND_SZ,
            "%SystemRoot%\\System32",
            0x700,
        );
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&[], "Path").unwrap(),
            Some(RegistryValue::String("%SystemRoot%\\System32".to_string()))
        );
    }

    #[test]
    fn read_vk_reg_multi_sz_preserves_all_items() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_multi_string_value(&mut data, 0x400, "Services", &["Tcpip", "Dnscache"], 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&[], "Services").unwrap(),
            Some(RegistryValue::MultiString(vec![
                "Tcpip".to_string(),
                "Dnscache".to_string()
            ]))
        );
    }

    #[test]
    fn read_vk_reg_qword_external() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_qword_value(&mut data, 0x400, "Counter", 0x1122_3344_5566_7788, 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&[], "Counter").unwrap(),
            Some(RegistryValue::Qword(0x1122_3344_5566_7788))
        );
    }

    #[test]
    fn odd_utf16_value_data_is_rejected() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        let data_abs = BASE_BLOCK_SIZE + 0x700;
        data[data_abs..data_abs + 4].copy_from_slice(&(-8i32).to_le_bytes());
        data[data_abs + 4..data_abs + 7].copy_from_slice(b"A\0B");
        write_vk(&mut data, 0x400, "Odd", REG_SZ, 3, 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();

        let err = hive.lookup_value(&[], "Odd").unwrap_err();
        assert!(err.contains("UTF-16 data has odd byte length"));
    }

    #[test]
    fn read_value_list_uses_registry_cell_header() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400, 0x500]);
        write_dword_value(&mut data, 0x400, "First", 1);
        write_dword_value(&mut data, 0x500, "Second", 2);
        let hive = RegistryHiveReader::new(&data).unwrap();

        assert_eq!(
            hive.lookup_value(&[], "Second").unwrap(),
            Some(RegistryValue::Dword(2))
        );
    }

    #[test]
    fn bounds_rejects_truncated_value_list_cell() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400, 0x500]);
        let list_abs = BASE_BLOCK_SIZE + 0x4020;
        data[list_abs..list_abs + 4].copy_from_slice(&(-4i32).to_le_bytes());
        let hive = RegistryHiveReader::new(&data).unwrap();

        let err = hive.lookup_value(&[], "Second").unwrap_err();
        assert!(err.contains("value list"));
        assert!(err.contains("exceeds cell"));
    }

    #[test]
    fn inline_value_longer_than_four_bytes_is_rejected() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        write_vk(&mut data, 0x400, "TooLong", REG_DWORD, 0x8000_0005, 1);
        let hive = RegistryHiveReader::new(&data).unwrap();

        let err = hive.lookup_value(&[], "TooLong").unwrap_err();
        assert!(err.contains("inline value"));
        assert!(err.contains("exceeds 4 bytes"));
    }

    #[test]
    fn short_external_dword_is_rejected_instead_of_zero_filled() {
        let mut data = empty_hive("ROOT");
        write_nk(&mut data, 0x20, "ROOT", &[], &[0x400]);
        let data_abs = BASE_BLOCK_SIZE + 0x700;
        data[data_abs..data_abs + 4].copy_from_slice(&(-8i32).to_le_bytes());
        data[data_abs + 4..data_abs + 6].copy_from_slice(&1u16.to_le_bytes());
        write_vk(&mut data, 0x400, "Short", REG_DWORD, 2, 0x700);
        let hive = RegistryHiveReader::new(&data).unwrap();

        let err = hive.lookup_value(&[], "Short").unwrap_err();
        assert!(err.contains("REG_DWORD value shorter than 4 bytes"));
    }

    #[test]
    fn bounds_rejects_bad_cell_offset() {
        let data = empty_hive("ROOT");
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert!(hive.parse_nk(0xFFFF).is_err());
    }

    #[test]
    fn corrupt_hive_returns_error_not_panic() {
        let mut data = empty_hive("ROOT");
        data[0x1020..0x1024].copy_from_slice(&(-999_999i32).to_le_bytes());
        let hive = RegistryHiveReader::new(&data).unwrap();
        assert!(hive.parse_nk(0x20).is_err());
    }

    #[test]
    fn extract_system_fields_from_fixture() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Control",
            &[("ComputerName", 0x600), ("TimeZoneInformation", 0xa00)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", "LAB-PC", 0x1800);
        write_nk(&mut data, 0xa00, "TimeZoneInformation", &[], &[0xd00]);
        write_string_value(
            &mut data,
            0xd00,
            "TimeZoneKeyName",
            "China Standard Time",
            0x1900,
        );

        let info = extract_system_hive_fields(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(info.computer_name.unwrap().value, "LAB-PC");
        assert_eq!(info.timezone.unwrap().value, "China Standard Time");
    }

    #[test]
    fn extract_system_fields_falls_back_when_select_is_corrupt() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_vk(
            &mut data,
            0x1200,
            "Current",
            REG_DWORD,
            0x8000_0004,
            0x9530_7897,
        );
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("ComputerName", 0x600)], &[]);
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", "LAB-PC", 0x1800);

        let info = extract_system_hive_fields(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(info.computer_name.unwrap().value, "LAB-PC");
        assert!(info
            .warnings
            .iter()
            .any(|warning| warning.contains("Select\\Current")));
    }

    #[test]
    fn extract_software_fields_from_fixture() {
        let mut data = empty_hive("SOFTWARE");
        write_nk(&mut data, 0x20, "SOFTWARE", &[("Microsoft", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Microsoft", &[("Windows NT", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "Windows NT",
            &[("CurrentVersion", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "CurrentVersion",
            &[],
            &[0x600, 0x680, 0x700],
        );
        write_string_value(
            &mut data,
            0x600,
            "ProductName",
            "Windows Evidence Edition",
            0x900,
        );
        write_string_value(&mut data, 0x680, "CurrentBuild", "26000", 0x980);
        write_dword_value(&mut data, 0x700, "InstallDate", 1_700_000_000);

        let info = extract_software_hive_fields(&data, "Windows/System32/config/SOFTWARE").unwrap();

        assert_eq!(info.product_name.unwrap().value, "Windows Evidence Edition");
        assert_eq!(info.current_build.unwrap().value, "26000");
        assert!(info.install_date.unwrap().value.starts_with("2023-"));
    }

    #[test]
    fn extract_system_fields_from_committed_tiny_fixture() {
        let bytes = std::fs::read(fixtures::tiny_registry_system_hive())
            .expect("read tiny SYSTEM registry fixture");

        let info = extract_system_hive_fields(&bytes, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(
            info.computer_name
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SYSTEM_COMPUTER_NAME)
        );
        assert_eq!(
            info.timezone.as_ref().map(|field| field.value.as_str()),
            Some(registry_fixture::SYSTEM_TIMEZONE)
        );
        assert!(info.warnings.is_empty());
    }

    #[test]
    fn extract_software_fields_from_committed_tiny_fixture() {
        let bytes = std::fs::read(fixtures::tiny_registry_software_hive())
            .expect("read tiny SOFTWARE registry fixture");

        let info =
            extract_software_hive_fields(&bytes, "Windows/System32/config/SOFTWARE").unwrap();

        assert_eq!(
            info.product_name.as_ref().map(|field| field.value.as_str()),
            Some(registry_fixture::SOFTWARE_PRODUCT_NAME)
        );
        assert_eq!(
            info.current_build
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SOFTWARE_CURRENT_BUILD)
        );
        assert_eq!(
            info.display_version
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SOFTWARE_DISPLAY_VERSION)
        );
        assert!(info
            .install_date
            .as_ref()
            .is_some_and(|field| field.value.starts_with("2023-")));
    }
}
