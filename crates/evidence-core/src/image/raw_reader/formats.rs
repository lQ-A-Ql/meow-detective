use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub(crate) enum Backend {
    Flat {
        file: File,
        base_offset: u64,
    },
    DynamicVhd {
        file: File,
        table_offset: u64,
        block_size: u64,
        bat: Vec<u32>,
        parent: Option<Box<Backend>>,
    },
    Qcow2 {
        file: File,
        cluster_bits: u32,
        virtual_size: u64,
        l1: Vec<u64>,
    },
    Vhdx {
        file: File,
        block_size: u64,
        virtual_size: u64,
        bat: Vec<u64>,
    },
}

pub(crate) fn read_backend(
    backend: &mut Backend,
    offset: u64,
    buf: &mut [u8],
) -> io::Result<usize> {
    match backend {
        Backend::Flat { file, base_offset } => {
            let physical = base_offset
                .checked_add(offset)
                .ok_or_else(|| invalid_data("image read offset overflows u64"))?;
            file.seek(SeekFrom::Start(physical))?;
            file.read(buf)
        }
        Backend::DynamicVhd {
            file,
            table_offset: _,
            block_size,
            bat,
            parent,
        } => {
            let mut done = 0usize;
            while done < buf.len() {
                let logical = offset + done as u64;
                let block_index = usize::try_from(logical / *block_size)
                    .map_err(|_| invalid_data("VHD block index overflow"))?;
                let within = logical % *block_size;
                let amount = (buf.len() - done).min((*block_size - within) as usize);
                if block_index >= bat.len() || bat[block_index] == 0xffff_ffff {
                    if let Some(parent) = parent.as_mut() {
                        let _ = read_backend(parent, logical, &mut buf[done..done + amount])?;
                    } else {
                        buf[done..done + amount].fill(0);
                    }
                } else {
                    let bitmap_bytes = block_size.div_ceil(4096) * 512;
                    let physical = u64::from(bat[block_index])
                        .checked_mul(512)
                        .and_then(|v| v.checked_add(bitmap_bytes))
                        .and_then(|v| v.checked_add(within))
                        .ok_or_else(|| invalid_data("VHD block offset overflow"))?;
                    file.seek(SeekFrom::Start(physical))?;
                    file.read_exact(&mut buf[done..done + amount])?;
                }
                done += amount;
            }
            Ok(done)
        }
        Backend::Qcow2 {
            file,
            cluster_bits,
            l1,
            ..
        } => {
            let cluster_size = 1u64 << *cluster_bits;
            let cluster_index = offset / cluster_size;
            let l2_entries = cluster_size / 8;
            let l1_index = usize::try_from(cluster_index / l2_entries)
                .map_err(|_| invalid_data("QCOW2 L1 index overflow"))?;
            if l1_index >= l1.len() || l1[l1_index] == 0 {
                buf.fill(0);
                return Ok(buf.len());
            }
            let l2_offset = l1[l1_index] & 0x00ff_ffff_ffff_fe00;
            let mut entry = [0u8; 8];
            let l2_index = cluster_index % l2_entries;
            file.seek(SeekFrom::Start(l2_offset + l2_index * 8))?;
            file.read_exact(&mut entry)?;
            let descriptor = u64::from_be_bytes(entry);
            if descriptor == 0 || descriptor & (1 << 62) != 0 {
                buf.fill(0);
                return Ok(buf.len());
            }
            let host = descriptor & 0x00ff_ffff_ffff_fe00;
            file.seek(SeekFrom::Start(host + offset % cluster_size))?;
            file.read(buf)
        }
        Backend::Vhdx {
            file,
            block_size,
            virtual_size: _,
            bat,
        } => {
            let mut done = 0usize;
            while done < buf.len() {
                let logical = offset + done as u64;
                let index = usize::try_from(logical / *block_size)
                    .map_err(|_| invalid_data("VHDX block index overflow"))?;
                let within = logical % *block_size;
                let amount = (buf.len() - done).min((*block_size - within) as usize);
                let descriptor = bat.get(index).copied().unwrap_or(0);
                let state = descriptor & 0x7;
                if state == 6 {
                    let physical = descriptor & 0xffff_ffff_fff0_0000;
                    file.seek(SeekFrom::Start(physical + within))?;
                    file.read_exact(&mut buf[done..done + amount])?;
                } else {
                    buf[done..done + amount].fill(0);
                }
                done += amount;
            }
            Ok(done)
        }
    }
}

pub(crate) struct VhdFooter {
    pub(crate) disk_type: u32,
    pub(crate) virtual_size: u64,
    pub(crate) data_offset: u64,
}

pub(crate) fn clone_backend(backend: &Backend) -> io::Result<Backend> {
    Ok(match backend {
        Backend::Flat { file, base_offset } => Backend::Flat {
            file: file.try_clone()?,
            base_offset: *base_offset,
        },
        Backend::DynamicVhd {
            file,
            table_offset,
            block_size,
            bat,
            parent,
        } => Backend::DynamicVhd {
            file: file.try_clone()?,
            table_offset: *table_offset,
            block_size: *block_size,
            bat: bat.clone(),
            parent: parent
                .as_ref()
                .map(|value| clone_backend(value))
                .transpose()?
                .map(Box::new),
        },
        Backend::Qcow2 {
            file,
            cluster_bits,
            virtual_size,
            l1,
        } => Backend::Qcow2 {
            file: file.try_clone()?,
            cluster_bits: *cluster_bits,
            virtual_size: *virtual_size,
            l1: l1.clone(),
        },
        Backend::Vhdx {
            file,
            block_size,
            virtual_size,
            bat,
        } => Backend::Vhdx {
            file: file.try_clone()?,
            block_size: *block_size,
            virtual_size: *virtual_size,
            bat: bat.clone(),
        },
    })
}

pub(crate) fn open_dynamic_vhd(
    mut file: File,
    footer: &VhdFooter,
    parent: Option<Box<Backend>>,
) -> io::Result<Backend> {
    let mut header = [0u8; 1024];
    file.seek(SeekFrom::Start(footer.data_offset))?;
    file.read_exact(&mut header)?;
    if &header[..8] != b"cxsparse" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid VHD dynamic header",
        ));
    }
    let table_offset = u64::from_be_bytes(header[24..32].try_into().unwrap());
    let entries = u32::from_be_bytes(header[32..36].try_into().unwrap());
    let block_size = u32::from_be_bytes(header[36..40].try_into().unwrap()) as u64;
    if entries == 0 || block_size == 0 || !block_size.is_power_of_two() {
        return Err(invalid_data("invalid VHD dynamic geometry"));
    }
    let mut bat = vec![
        0u8;
        usize::try_from(u64::from(entries) * 4)
            .map_err(|_| invalid_data("VHD BAT too large"))?
    ];
    file.seek(SeekFrom::Start(table_offset))?;
    file.read_exact(&mut bat)?;
    let bat = bat
        .chunks_exact(4)
        .map(|v| u32::from_be_bytes(v.try_into().unwrap()))
        .collect();
    Ok(Backend::DynamicVhd {
        file,
        table_offset,
        block_size,
        bat,
        parent,
    })
}

pub(crate) fn open_qcow2(path: &Path) -> io::Result<Option<(Backend, u64)>> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 72];
    if file.read(&mut header)? < 32 || &header[..4] != b"QFI\xfb" {
        return Ok(None);
    }
    let version = u32::from_be_bytes(header[4..8].try_into().unwrap());
    if version != 2 && version != 3 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported QCOW version",
        ));
    }
    let backing_offset = u64::from_be_bytes(header[8..16].try_into().unwrap());
    let backing_size = u32::from_be_bytes(header[16..20].try_into().unwrap());
    if backing_offset != 0 || backing_size != 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "QCOW2 backing chains are unsupported",
        ));
    }
    let virtual_size = u64::from_be_bytes(header[24..32].try_into().unwrap());
    let cluster_bits = u32::from_be_bytes(header[20..24].try_into().unwrap());
    let l1_offset = u64::from_be_bytes(header[48..56].try_into().unwrap());
    if !(9..=21).contains(&cluster_bits) || virtual_size == 0 {
        return Err(invalid_data("invalid QCOW2 geometry"));
    }
    let cluster_size = 1u64 << cluster_bits;
    let l2_entries = cluster_size / 8;
    let l1_count = (virtual_size
        .saturating_add(cluster_size.saturating_mul(l2_entries).saturating_sub(1))
        / cluster_size.saturating_mul(l2_entries)) as usize;
    if l1_count > 1_000_000 {
        return Err(invalid_data("QCOW2 L1 table is too large"));
    }
    let mut raw = vec![0u8; l1_count * 8];
    file.seek(SeekFrom::Start(l1_offset))?;
    file.read_exact(&mut raw)?;
    let l1 = raw
        .chunks_exact(8)
        .map(|v| u64::from_be_bytes(v.try_into().unwrap()))
        .collect();
    Ok(Some((
        Backend::Qcow2 {
            file,
            cluster_bits,
            virtual_size,
            l1,
        },
        virtual_size,
    )))
}

pub(crate) fn open_vhdx(path: &Path) -> io::Result<Option<(Backend, u64)>> {
    let mut file = File::open(path)?;
    let mut signature = [0u8; 8];
    if file.read_exact(&mut signature).is_err() || &signature != b"vhdxfile" {
        return Ok(None);
    }
    let mut region = vec![0u8; 64 * 1024];
    file.seek(SeekFrom::Start(192 * 1024))?;
    file.read_exact(&mut region)?;
    if &region[..4] != b"regi" {
        return Err(invalid_data("invalid VHDX region table"));
    }
    let count = u32::from_le_bytes(region[8..12].try_into().unwrap()) as usize;
    let (mut bat_offset, mut bat_length, mut metadata_offset, mut metadata_length) =
        (None, None, None, None);
    for index in 0..count.min(2047) {
        let start = 32 + index * 32;
        if start + 32 > region.len() {
            break;
        }
        let guid = &region[start..start + 16];
        let offset = u64::from_le_bytes(region[start + 16..start + 24].try_into().unwrap());
        let length = u32::from_le_bytes(region[start + 24..start + 28].try_into().unwrap());
        if guid
            == vhdx_guid(
                0x2dc27766,
                0xf623,
                0x4200,
                [0x9d, 0x64, 0x11, 0x5e, 0x9b, 0xfd, 0x4a, 0x08],
            )
        {
            bat_offset = Some(offset);
            bat_length = Some(length);
        }
        if guid
            == vhdx_guid(
                0x8b7ca206,
                0x4790,
                0x4b9a,
                [0xb8, 0xfe, 0x57, 0x5f, 0x05, 0x0f, 0x88, 0x6e],
            )
        {
            metadata_offset = Some(offset);
            metadata_length = Some(length);
        }
    }
    let metadata_offset =
        metadata_offset.ok_or_else(|| invalid_data("VHDX metadata region missing"))?;
    let _metadata_length =
        metadata_length.ok_or_else(|| invalid_data("VHDX metadata length missing"))?;
    let mut table = vec![0u8; 64 * 1024];
    file.seek(SeekFrom::Start(metadata_offset))?;
    file.read_exact(&mut table)?;
    if &table[..8] != b"metadata" {
        return Err(invalid_data("invalid VHDX metadata table"));
    }
    let count = u16::from_le_bytes(table[10..12].try_into().unwrap()) as usize;
    let mut params = None;
    let mut virtual_size = None;
    let mut logical_sector = None;
    for index in 0..count.min(2047) {
        let start = 32 + index * 32;
        if start + 32 > table.len() {
            break;
        }
        let guid = &table[start..start + 16];
        let rel = u32::from_le_bytes(table[start + 16..start + 20].try_into().unwrap()) as u64;
        let len = u32::from_le_bytes(table[start + 20..start + 24].try_into().unwrap()) as usize;
        let mut payload = vec![0u8; len.min(64)];
        file.seek(SeekFrom::Start(metadata_offset + rel))?;
        file.read_exact(&mut payload)?;
        if guid
            == vhdx_guid(
                0xcaa16737,
                0xfa36,
                0x4d43,
                [0xb3, 0xb6, 0x33, 0xf0, 0xaa, 0x44, 0xe7, 0x6b],
            )
        {
            params = Some(payload);
        } else if guid
            == vhdx_guid(
                0x2fa54224,
                0xcd1b,
                0x4876,
                [0xb2, 0x11, 0x5d, 0xbe, 0xd8, 0x3b, 0xf4, 0xb8],
            )
        {
            virtual_size = Some(u64::from_le_bytes(payload[..8].try_into().unwrap()));
        } else if guid
            == vhdx_guid(
                0x8141bf1d,
                0xa96f,
                0x4709,
                [0xba, 0x47, 0xf2, 0x33, 0xa8, 0xfa, 0xab, 0x5f],
            )
        {
            logical_sector = Some(u32::from_le_bytes(payload[..4].try_into().unwrap()));
        }
    }
    let params = params.ok_or_else(|| invalid_data("VHDX file parameters missing"))?;
    let virtual_size = virtual_size.ok_or_else(|| invalid_data("VHDX virtual size missing"))?;
    if logical_sector != Some(512) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "VHDX logical sector size is unsupported",
        ));
    }
    let block_size = u64::from(u32::from_le_bytes(params[..4].try_into().unwrap()));
    let flags = u32::from_le_bytes(params[4..8].try_into().unwrap());
    if flags & 2 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "VHDX differencing parent chains are unsupported",
        ));
    }
    if !(1 << 20..=256 << 20).contains(&block_size) || !block_size.is_power_of_two() {
        return Err(invalid_data("invalid VHDX block size"));
    }
    let data_blocks = virtual_size.div_ceil(block_size);
    let chunk_ratio = (1u64 << 23) * 512 / block_size;
    let bat_entries = data_blocks + data_blocks.saturating_sub(1) / chunk_ratio;
    let bat_offset = bat_offset.ok_or_else(|| invalid_data("VHDX BAT missing"))?;
    let bat_length = bat_length.ok_or_else(|| invalid_data("VHDX BAT length missing"))?;
    if bat_entries * 8 > u64::from(bat_length) {
        return Err(invalid_data("VHDX BAT is too small"));
    }
    let mut raw = vec![0u8; (bat_entries * 8) as usize];
    file.seek(SeekFrom::Start(bat_offset))?;
    file.read_exact(&mut raw)?;
    let bat = raw
        .chunks_exact(8)
        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
        .collect();
    Ok(Some((
        Backend::Vhdx {
            file,
            block_size,
            virtual_size,
            bat,
        },
        virtual_size,
    )))
}

fn vhdx_guid(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..4].copy_from_slice(&data1.to_le_bytes());
    out[4..6].copy_from_slice(&data2.to_le_bytes());
    out[6..8].copy_from_slice(&data3.to_le_bytes());
    out[8..].copy_from_slice(&data4);
    out
}

pub(crate) fn read_vhd_footer(path: &Path) -> io::Result<Option<VhdFooter>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len < 512 {
        return Ok(None);
    }
    file.seek(SeekFrom::End(-512))?;
    let mut footer = [0u8; 512];
    file.read_exact(&mut footer)?;
    if &footer[..8] != b"conectix" {
        return Ok(None);
    }
    let disk_type = u32::from_be_bytes(footer[60..64].try_into().expect("fixed slice"));
    let virtual_size = u64::from_be_bytes(footer[48..56].try_into().expect("fixed slice"));
    let data_offset = u64::from_be_bytes(footer[16..24].try_into().expect("fixed slice"));
    Ok(Some(VhdFooter {
        disk_type,
        virtual_size,
        data_offset,
    }))
}

pub(crate) fn read_vhd_parent_name(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len < 512 {
        return Err(invalid_data("VHD parent footer is missing"));
    }
    file.seek(SeekFrom::End(-512))?;
    let mut footer = [0u8; 512];
    file.read_exact(&mut footer)?;
    let value: String = footer[64..]
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .take(224)
        .filter(|unit| *unit != 0)
        .filter_map(|unit| char::from_u32(u32::from(unit)))
        .collect();
    if value.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "differencing VHD has no parent locator",
        ));
    }
    Ok(value)
}

pub(crate) fn resolve_vhd_parent_path(child: &Path, name: &str) -> io::Result<PathBuf> {
    let normalized = name.replace('\\', "/");
    let relative = Path::new(&normalized);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "VHD parent path must be relative",
        ));
    }
    let parent = child
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let resolved = parent.join(relative).canonicalize()?;
    if !resolved.starts_with(&parent) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "VHD parent escapes child directory",
        ));
    }
    Ok(resolved)
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
