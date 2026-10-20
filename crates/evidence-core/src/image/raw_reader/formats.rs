use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

mod parent;
const MAX_VHDX_BAT_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) use parent::{
    read_qcow_backing_path, read_vhd_parent_name, read_vhdx_parent_path, resolve_vhd_parent_path,
};

pub(crate) fn vhdx_guid(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..4].copy_from_slice(&data1.to_le_bytes());
    out[4..6].copy_from_slice(&data2.to_le_bytes());
    out[6..8].copy_from_slice(&data3.to_le_bytes());
    out[8..].copy_from_slice(&data4);
    out
}

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
        parent: Option<Box<Backend>>,
    },
    Vhdx {
        file: File,
        block_size: u64,
        chunk_ratio: u64,
        virtual_size: u64,
        bat: Vec<u64>,
        parent: Option<Box<Backend>>,
    },
}

pub(crate) fn read_backend(
    backend: &mut Backend,
    offset: u64,
    buf: &mut [u8],
) -> io::Result<usize> {
    match backend {
        Backend::Flat { file, base_offset } => read_flat(file, *base_offset, offset, buf),
        Backend::DynamicVhd {
            file,
            block_size,
            bat,
            parent,
            ..
        } => read_dynamic_vhd(file, *block_size, bat, parent, offset, buf),
        Backend::Qcow2 {
            file,
            cluster_bits,
            l1,
            parent,
            ..
        } => read_qcow(file, *cluster_bits, l1, parent, offset, buf),
        Backend::Vhdx {
            file,
            block_size,
            chunk_ratio,
            bat,
            parent,
            ..
        } => read_vhdx(file, *block_size, *chunk_ratio, bat, parent, offset, buf),
    }
}

fn read_flat(file: &mut File, base_offset: u64, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
    let physical = base_offset
        .checked_add(offset)
        .ok_or_else(|| invalid_data("image read offset overflows u64"))?;
    file.seek(SeekFrom::Start(physical))?;
    file.read(buf)
}

fn read_dynamic_vhd(
    file: &mut File,
    block_size: u64,
    bat: &[u32],
    parent: &mut Option<Box<Backend>>,
    offset: u64,
    buf: &mut [u8],
) -> io::Result<usize> {
    let mut done = 0usize;
    while done < buf.len() {
        let logical = offset
            .checked_add(done as u64)
            .ok_or_else(|| invalid_data("VHD read offset overflows u64"))?;
        let block_index = usize::try_from(logical / block_size)
            .map_err(|_| invalid_data("VHD block index overflow"))?;
        let within = logical % block_size;
        let amount = (buf.len() - done).min((block_size - within) as usize);
        if block_index >= bat.len() || bat[block_index] == 0xffff_ffff {
            if let Some(parent) = parent.as_mut() {
                read_backend(parent, logical, &mut buf[done..done + amount])?;
            } else {
                buf[done..done + amount].fill(0);
            }
        } else {
            let bitmap_bytes = block_size.div_ceil(4096).div_ceil(512) * 512;
            let block_start = u64::from(bat[block_index])
                .checked_mul(512)
                .and_then(|value| value.checked_add(bitmap_bytes))
                .ok_or_else(|| invalid_data("VHD block offset overflow"))?;
            read_dynamic_block(
                file,
                parent,
                DynamicVhdBlockMap {
                    block_start,
                    bitmap_start: u64::from(bat[block_index]) * 512,
                    bitmap_bytes,
                    logical_start: logical,
                },
                within,
                &mut buf[done..done + amount],
            )?;
        }
        done += amount;
    }
    Ok(done)
}

struct DynamicVhdBlockMap {
    block_start: u64,
    bitmap_start: u64,
    bitmap_bytes: u64,
    logical_start: u64,
}

fn read_dynamic_block(
    file: &mut File,
    parent: &mut Option<Box<Backend>>,
    map: DynamicVhdBlockMap,
    within: u64,
    output: &mut [u8],
) -> io::Result<()> {
    let mut done = 0usize;
    while done < output.len() {
        let logical = within
            .checked_add(done as u64)
            .ok_or_else(|| invalid_data("VHD block offset overflow"))?;
        let sector = logical / 512;
        let sector_offset = logical % 512;
        let amount = (output.len() - done).min((512 - sector_offset) as usize);
        let bitmap_byte = sector / 8;
        let bitmap_bit = sector % 8;
        if bitmap_byte >= map.bitmap_bytes {
            return Err(invalid_data("VHD bitmap does not cover block"));
        }
        file.seek(SeekFrom::Start(map.bitmap_start + bitmap_byte))?;
        let mut bitmap = [0u8; 1];
        file.read_exact(&mut bitmap)?;
        if bitmap[0] & (0x80 >> bitmap_bit) != 0 {
            file.seek(SeekFrom::Start(
                map.block_start + sector * 512 + sector_offset,
            ))?;
            let target = &mut output[done..done + amount];
            let read = file.read(target)?;
            if read < target.len() {
                target[read..].fill(0);
            }
        } else if let Some(parent) = parent.as_mut() {
            let parent_offset = map
                .logical_start
                .checked_add(done as u64)
                .ok_or_else(|| invalid_data("VHD parent offset overflow"))?;
            let _ = read_backend(parent, parent_offset, &mut output[done..done + amount])?;
        } else {
            output[done..done + amount].fill(0);
        }
        done += amount;
    }
    Ok(())
}

fn read_qcow(
    file: &mut File,
    cluster_bits: u32,
    l1: &[u64],
    parent: &mut Option<Box<Backend>>,
    offset: u64,
    buf: &mut [u8],
) -> io::Result<usize> {
    let cluster_size = 1u64 << cluster_bits;
    let mut done = 0usize;
    while done < buf.len() {
        let logical = offset
            .checked_add(done as u64)
            .ok_or_else(|| invalid_data("QCOW read offset overflows u64"))?;
        let within = logical % cluster_size;
        let amount = (buf.len() - done).min((cluster_size - within) as usize);
        let cluster_index = logical / cluster_size;
        let l2_entries = cluster_size / 8;
        let l1_index = usize::try_from(cluster_index / l2_entries)
            .map_err(|_| invalid_data("QCOW2 L1 index overflow"))?;
        let descriptor = if l1_index >= l1.len() || l1[l1_index] == 0 {
            0
        } else {
            let l2_offset = l1[l1_index] & qcow_offset_mask(cluster_bits);
            let l2_index = cluster_index % l2_entries;
            let entry_offset = l2_offset
                .checked_add(l2_index * 8)
                .ok_or_else(|| invalid_data("QCOW2 L2 offset overflows u64"))?;
            let mut entry = [0u8; 8];
            file.seek(SeekFrom::Start(entry_offset))?;
            file.read_exact(&mut entry)?;
            u64::from_be_bytes(entry)
        };
        if descriptor == 0 {
            read_qcow_parent_or_zero(parent, logical, &mut buf[done..done + amount])?;
        } else if descriptor & (1 << 62) != 0 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "compressed QCOW clusters require decompression support",
            ));
        } else {
            let host = descriptor & qcow_offset_mask(cluster_bits);
            let physical = host
                .checked_add(within)
                .ok_or_else(|| invalid_data("QCOW cluster offset overflows u64"))?;
            file.seek(SeekFrom::Start(physical))?;
            file.read_exact(&mut buf[done..done + amount])?;
        }
        done += amount;
    }
    Ok(done)
}

fn qcow_offset_mask(cluster_bits: u32) -> u64 {
    (1u64 << (63 - cluster_bits)) - 1
}

fn read_qcow_parent_or_zero(
    parent: &mut Option<Box<Backend>>,
    offset: u64,
    buf: &mut [u8],
) -> io::Result<usize> {
    if let Some(parent) = parent.as_mut() {
        read_backend(parent, offset, buf)
    } else {
        buf.fill(0);
        Ok(buf.len())
    }
}

fn read_vhdx(
    file: &mut File,
    block_size: u64,
    chunk_ratio: u64,
    bat: &[u64],
    parent: &mut Option<Box<Backend>>,
    offset: u64,
    buf: &mut [u8],
) -> io::Result<usize> {
    let mut done = 0usize;
    while done < buf.len() {
        let logical = offset
            .checked_add(done as u64)
            .ok_or_else(|| invalid_data("VHDX read offset overflows u64"))?;
        let index = usize::try_from(logical / block_size)
            .map_err(|_| invalid_data("VHDX block index overflow"))?;
        let within = logical % block_size;
        let amount = (buf.len() - done).min((block_size - within) as usize);
        let bat_index = index
            .checked_add(index / chunk_ratio as usize)
            .ok_or_else(|| invalid_data("VHDX BAT index overflows usize"))?;
        let descriptor = bat.get(bat_index).copied().unwrap_or(0);
        let state = descriptor & 0x7;
        if state == 6 {
            let physical = (descriptor & 0xffff_ffff_fff0_0000)
                .checked_add(within)
                .ok_or_else(|| invalid_data("VHDX block offset overflows u64"))?;
            file.seek(SeekFrom::Start(physical))?;
            file.read_exact(&mut buf[done..done + amount])?;
        } else if matches!(state, 0 | 1) {
            if let Some(parent) = parent.as_mut() {
                read_backend(parent, logical, &mut buf[done..done + amount])?;
            } else {
                buf[done..done + amount].fill(0);
            }
        } else if state == 7 {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "VHDX partially present blocks require sector bitmap support",
            ));
        } else {
            buf[done..done + amount].fill(0);
        }
        done += amount;
    }
    Ok(done)
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
            parent,
        } => Backend::Qcow2 {
            file: file.try_clone()?,
            cluster_bits: *cluster_bits,
            virtual_size: *virtual_size,
            l1: l1.clone(),
            parent: parent
                .as_ref()
                .map(|value| clone_backend(value))
                .transpose()?
                .map(Box::new),
        },
        Backend::Vhdx {
            file,
            block_size,
            chunk_ratio,
            virtual_size,
            bat,
            parent,
        } => Backend::Vhdx {
            file: file.try_clone()?,
            block_size: *block_size,
            chunk_ratio: *chunk_ratio,
            virtual_size: *virtual_size,
            bat: bat.clone(),
            parent: parent
                .as_ref()
                .map(|value| clone_backend(value))
                .transpose()?
                .map(Box::new),
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
    let table_offset = u64::from_be_bytes(header[16..24].try_into().unwrap());
    let entries = u32::from_be_bytes(header[28..32].try_into().unwrap());
    let block_size = u32::from_be_bytes(header[32..36].try_into().unwrap()) as u64;
    if entries == 0 || block_size == 0 || !block_size.is_power_of_two() {
        return Err(invalid_data("invalid VHD dynamic geometry"));
    }
    if u64::from(entries) * 4 > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "VHD metadata table exceeds reader capacity",
        ));
    }
    let mut bat = vec![
        0u8;
        usize::try_from(u64::from(entries) * 4)
            .map_err(|_| invalid_data("VHD BAT too large"))?
    ];
    file.seek(SeekFrom::Start(table_offset))?;
    file.read_exact(&mut bat)?;
    let bat = bat
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| u32::from_be_bytes(*v))
        .collect();
    Ok(Backend::DynamicVhd {
        file,
        table_offset,
        block_size,
        bat,
        parent,
    })
}

pub(crate) fn open_qcow2(
    path: &Path,
    parent: Option<Box<Backend>>,
) -> io::Result<Option<(Backend, u64)>> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 72];
    if file.read(&mut header)? < 32 || &header[..4] != b"QFI\xfb" {
        return Ok(None);
    }
    let version = u32::from_be_bytes(header[4..8].try_into().unwrap());
    if version == 1 {
        let virtual_size = u64::from_be_bytes(header[24..32].try_into().unwrap());
        let cluster_bits = u32::from(header[32]);
        let l2_bits = u32::from(header[33]);
        let l1_offset = u64::from_be_bytes(header[40..48].try_into().unwrap());
        if virtual_size == 0 || !(9..=16).contains(&cluster_bits) || !(6..=13).contains(&l2_bits) {
            return Err(invalid_data("invalid QCOW v1 geometry"));
        }
        return Ok(Some((
            read_qcow_l1(file, virtual_size, cluster_bits, l2_bits, l1_offset, parent)?,
            virtual_size,
        )));
    }
    if version != 2 && version != 3 {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unsupported QCOW version",
        ));
    }
    let virtual_size = u64::from_be_bytes(header[24..32].try_into().unwrap());
    let cluster_bits = u32::from_be_bytes(header[20..24].try_into().unwrap());
    let l1_size = u32::from_be_bytes(header[36..40].try_into().unwrap()) as usize;
    let l1_offset = u64::from_be_bytes(header[40..48].try_into().unwrap());
    if !(9..=21).contains(&cluster_bits) || virtual_size == 0 {
        return Err(invalid_data("invalid QCOW2 geometry"));
    }
    let cluster_size = 1u64 << cluster_bits;
    let l2_entries = cluster_size / 8;
    let expected_l1_count = (virtual_size
        .saturating_add(cluster_size.saturating_mul(l2_entries).saturating_sub(1))
        / cluster_size.saturating_mul(l2_entries)) as usize;
    if l1_size == 0 || l1_size != expected_l1_count || l1_size > 1_000_000 {
        return Err(invalid_data("QCOW2 L1 table is too large"));
    }
    let mut raw = vec![0u8; l1_size * 8];
    file.seek(SeekFrom::Start(l1_offset))?;
    file.read_exact(&mut raw)?;
    let l1 = raw
        .as_chunks::<8>()
        .0
        .iter()
        .map(|v| u64::from_be_bytes(*v))
        .collect();
    Ok(Some((
        Backend::Qcow2 {
            file,
            cluster_bits,
            virtual_size,
            l1,
            parent,
        },
        virtual_size,
    )))
}

fn read_qcow_l1(
    mut file: File,
    virtual_size: u64,
    cluster_bits: u32,
    l2_bits: u32,
    l1_offset: u64,
    parent: Option<Box<Backend>>,
) -> io::Result<Backend> {
    let cluster_size = 1u64 << cluster_bits;
    let l2_entries = 1u64 << l2_bits;
    let l1_count = virtual_size.div_ceil(cluster_size * l2_entries) as usize;
    if l1_count > 1_000_000 {
        return Err(invalid_data("QCOW L1 table is too large"));
    }
    let mut raw = vec![0u8; l1_count * 8];
    file.seek(SeekFrom::Start(l1_offset))?;
    file.read_exact(&mut raw)?;
    let l1 = raw
        .as_chunks::<8>()
        .0
        .iter()
        .map(|v| u64::from_be_bytes(*v))
        .collect();
    Ok(Backend::Qcow2 {
        file,
        cluster_bits,
        virtual_size,
        l1,
        parent,
    })
}

pub(crate) fn open_vhdx(
    path: &Path,
    parent: Option<Box<Backend>>,
) -> io::Result<Option<(Backend, u64)>> {
    let mut file = File::open(path)?;
    let mut signature = [0u8; 8];
    if file.read_exact(&mut signature).is_err() || &signature != b"vhdxfile" {
        return Ok(None);
    }
    let (bat_offset, bat_length, metadata_offset) = read_vhdx_regions(&mut file)?;
    let (params, virtual_size, logical_sector) = read_vhdx_metadata(&mut file, metadata_offset)?;
    if !matches!(logical_sector, Some(512) | Some(4096)) {
        return Err(invalid_data("VHDX logical sector size is invalid"));
    }
    let block_size = u64::from(u32::from_le_bytes(params[..4].try_into().unwrap()));
    if !(1 << 20..=256 << 20).contains(&block_size) || !block_size.is_power_of_two() {
        return Err(invalid_data("invalid VHDX block size"));
    }
    let bat = read_vhdx_bat(&mut file, bat_offset, bat_length, block_size, virtual_size)?;
    let chunk_ratio = (1u64 << 23) * 512 / block_size;
    Ok(Some((
        Backend::Vhdx {
            file,
            block_size,
            chunk_ratio,
            virtual_size,
            bat,
            parent,
        },
        virtual_size,
    )))
}

fn read_vhdx_regions(file: &mut File) -> io::Result<(u64, u32, u64)> {
    let mut region = vec![0u8; 64 * 1024];
    file.seek(SeekFrom::Start(192 * 1024))?;
    file.read_exact(&mut region)?;
    if &region[..4] != b"regi" {
        return Err(invalid_data("invalid VHDX region table"));
    }
    let count = u32::from_le_bytes(region[8..12].try_into().unwrap()) as usize;
    let bat_guid = vhdx_guid(
        0x2dc27766,
        0xf623,
        0x4200,
        [0x9d, 0x64, 0x11, 0x5e, 0x9b, 0xfd, 0x4a, 0x08],
    );
    let metadata_guid = vhdx_guid(
        0x8b7ca206,
        0x4790,
        0x4b9a,
        [0xb8, 0xfe, 0x57, 0x5f, 0x05, 0x0f, 0x88, 0x6e],
    );
    let mut bat = None;
    let mut metadata = None;
    for index in 0..count.min(2047) {
        let start = 16 + index * 32;
        if start + 32 > region.len() {
            break;
        }
        let guid = &region[start..start + 16];
        let offset = u64::from_le_bytes(region[start + 16..start + 24].try_into().unwrap());
        let length = u32::from_le_bytes(region[start + 24..start + 28].try_into().unwrap());
        if guid == bat_guid {
            bat = Some((offset, length));
        } else if guid == metadata_guid {
            metadata = Some(offset);
        }
    }
    let (bat_offset, bat_length) = bat.ok_or_else(|| invalid_data("VHDX BAT missing"))?;
    let metadata_offset = metadata.ok_or_else(|| invalid_data("VHDX metadata region missing"))?;
    Ok((bat_offset, bat_length, metadata_offset))
}

fn read_vhdx_metadata(
    file: &mut File,
    metadata_offset: u64,
) -> io::Result<(Vec<u8>, u64, Option<u32>)> {
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
            && payload.len() >= 8
        {
            virtual_size = Some(u64::from_le_bytes(payload[..8].try_into().unwrap()));
        } else if guid
            == vhdx_guid(
                0x8141bf1d,
                0xa96f,
                0x4709,
                [0xba, 0x47, 0xf2, 0x33, 0xa8, 0xfa, 0xab, 0x5f],
            )
            && payload.len() >= 4
        {
            logical_sector = Some(u32::from_le_bytes(payload[..4].try_into().unwrap()));
        }
    }
    let params = params.ok_or_else(|| invalid_data("VHDX file parameters missing"))?;
    let virtual_size = virtual_size.ok_or_else(|| invalid_data("VHDX virtual size missing"))?;
    Ok((params, virtual_size, logical_sector))
}

fn read_vhdx_bat(
    file: &mut File,
    bat_offset: u64,
    bat_length: u32,
    block_size: u64,
    virtual_size: u64,
) -> io::Result<Vec<u64>> {
    let data_blocks = virtual_size.div_ceil(block_size);
    let chunk_ratio = (1u64 << 23) * 512 / block_size;
    let bat_entries = data_blocks + data_blocks.saturating_sub(1) / chunk_ratio;
    let bat_bytes = bat_entries
        .checked_mul(std::mem::size_of::<u64>() as u64)
        .ok_or_else(|| invalid_data("VHDX BAT size overflows u64"))?;
    if bat_bytes > u64::from(bat_length) {
        return Err(invalid_data("VHDX BAT region is too small"));
    }
    if bat_bytes > MAX_VHDX_BAT_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "VHDX BAT exceeds reader capacity",
        ));
    }
    let mut raw = vec![
        0u8;
        usize::try_from(bat_bytes)
            .map_err(|_| invalid_data("VHDX BAT size exceeds platform capacity"))?
    ];
    file.seek(SeekFrom::Start(bat_offset))?;
    file.read_exact(&mut raw)?;
    Ok(raw
        .as_chunks::<8>()
        .0
        .iter()
        .map(|value| u64::from_le_bytes(*value))
        .collect())
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

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
