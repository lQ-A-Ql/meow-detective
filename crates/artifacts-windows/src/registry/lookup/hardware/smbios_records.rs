use super::{smbios::Structure, RegistryHardwareRecord};

pub(super) fn decode(entry: &Structure<'_>, key_path: &str) -> Option<RegistryHardwareRecord> {
    let kind = match entry.formatted[0] {
        0 => "firmware",
        1 => "system",
        2 => "baseboard",
        4 => "cpu",
        17 => "ram",
        _ => return None,
    };
    let handle = entry.word(2)?;
    let mut record = RegistryHardwareRecord::new(
        kind,
        format!("smbios:{handle:04x}:{:08x}", entry.offset),
        key_path.into(),
    );
    record.value_name = Some("SMBiosData".into());
    record.source_offset = Some(entry.offset);
    match kind {
        "cpu" => processor(entry, &mut record)?,
        "ram" => memory(entry, &mut record)?,
        _ => identity(entry, &mut record),
    }
    Some(record)
}

fn identity(entry: &Structure<'_>, record: &mut RegistryHardwareRecord) {
    record.manufacturer = entry.string(4);
    record.name = entry.string(5).unwrap_or_default();
    if record.kind == "firmware" {
        record.property("releaseDate", entry.string(8));
    } else {
        record.property("version", entry.string(6));
        record.property("serialNumber", entry.string(7));
        if record.kind == "system" {
            record.property("sku", entry.string(0x19));
            record.property("family", entry.string(0x1a));
        }
    }
}

fn processor(entry: &Structure<'_>, record: &mut RegistryHardwareRecord) -> Option<()> {
    if entry.formatted.get(5) != Some(&3) || *entry.formatted.get(0x18)? & 0x40 == 0 {
        return None;
    }
    record.name = entry.string(0x10).unwrap_or_default();
    record.manufacturer = entry.string(7);
    record.property("socket", entry.string(4));
    record.property("maxSpeedMhz", entry.word(0x14).filter(|v| *v > 0));
    record.property("currentSpeedMhz", entry.word(0x16).filter(|v| *v > 0));
    record.property("coreCount", entry.count(0x23, 0x2a));
    record.property("enabledCoreCount", entry.count(0x24, 0x2c));
    record.property("threadCount", entry.count(0x25, 0x2e));
    record.property("serialNumber", entry.string(0x20));
    record.property("partNumber", entry.string(0x22));
    Some(())
}

fn memory(entry: &Structure<'_>, record: &mut RegistryHardwareRecord) -> Option<()> {
    let size = entry.word(0x0c)?;
    // Empty sockets and logical mappings are not installed physical modules.
    if size == 0 || entry.formatted.get(0x12) == Some(&0x1f) {
        return None;
    }
    record.name = entry
        .string(0x1a)
        .or_else(|| entry.string(0x10))
        .unwrap_or_default();
    record.manufacturer = entry.string(0x17);
    let capacity = match size {
        0xffff => None,
        0x7fff => entry
            .dword(0x1c)
            .filter(|v| *v > 0 && *v < 0x8000_0000)
            .map(|v| u64::from(v) * 1024 * 1024),
        value if value & 0x8000 != 0 => Some(u64::from(value & 0x7fff) * 1024),
        value => Some(u64::from(value) * 1024 * 1024),
    };
    record.property("capacityBytes", capacity.filter(|v| *v > 0));
    record.property("deviceLocator", entry.string(0x10));
    record.property("bankLocator", entry.string(0x11));
    record.property("serialNumber", entry.string(0x18));
    record.property("partNumber", entry.string(0x1a));
    record.property("speedMtS", entry.speed(0x15, 0x54));
    record.property("configuredSpeedMtS", entry.speed(0x20, 0x58));
    record.property(
        "memoryType",
        entry
            .formatted
            .get(0x12)
            .and_then(|value| memory_type(*value)),
    );
    record.property(
        "formFactor",
        entry.formatted.get(0x0e).and_then(|value| match value {
            9 => Some("DIMM"),
            13 => Some("SO-DIMM"),
            _ => None,
        }),
    );
    Some(())
}

fn memory_type(value: u8) -> Option<&'static str> {
    match value {
        0x12 => Some("DDR"),
        0x13 => Some("DDR2"),
        0x18 => Some("DDR3"),
        0x1a => Some("DDR4"),
        0x1b => Some("LPDDR"),
        0x1c => Some("LPDDR2"),
        0x1d => Some("LPDDR3"),
        0x1e => Some("LPDDR4"),
        0x22 => Some("DDR5"),
        0x23 => Some("LPDDR5"),
        _ => None,
    }
}
