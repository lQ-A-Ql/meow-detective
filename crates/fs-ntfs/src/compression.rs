//! NTFS LZNT1 decompression helpers.

use crate::invalid_fs_data;
use std::io;

/// Decompress an LZNT1-compressed buffer.
pub(crate) fn lznt1_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 2 <= data.len() {
        let header = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        if header == 0 {
            break;
        }
        let chunk_len = ((header & 0x0fff) as usize) + 1;
        if pos + chunk_len > data.len() {
            return Err("LZNT1 chunk exceeds input".to_string());
        }
        let chunk = &data[pos..pos + chunk_len];
        pos += chunk_len;
        if header & 0x8000 == 0 {
            out.extend_from_slice(chunk);
        } else {
            decompress_lznt1_chunk(chunk, &mut out)?;
        }
    }
    Ok(out)
}

/// Append one compression unit to the output buffer.
///
/// `logical_bytes` is the decompressed size the unit is expected to cover.
pub(crate) fn append_compressed_unit(
    out: &mut Vec<u8>,
    physical: &[u8],
    has_sparse: bool,
    logical_bytes: u64,
    max_bytes: usize,
) -> io::Result<()> {
    let logical_len = logical_bytes as usize;
    let decoded = if physical.is_empty() {
        vec![0u8; logical_len]
    } else if has_sparse {
        lznt1_decompress(physical)
            .map_err(invalid_fs_data)
            .unwrap_or_else(|_| physical.to_vec())
    } else if physical.len() as u64 == logical_bytes {
        physical.to_vec()
    } else {
        lznt1_decompress(physical).map_err(invalid_fs_data)?
    };

    let append_len = decoded.len().min(logical_len);
    let new_size = out
        .len()
        .checked_add(append_len)
        .ok_or_else(|| invalid_fs_data("compressed output size overflow"))?;
    if new_size > max_bytes {
        return Err(invalid_fs_data(format!(
            "compressed output exceeds {} byte limit (would be {} bytes)",
            max_bytes, new_size
        )));
    }
    out.extend_from_slice(&decoded[..append_len]);
    if append_len < logical_len {
        let final_size = out
            .len()
            .checked_add(logical_len - append_len)
            .ok_or_else(|| invalid_fs_data("compressed sparse padding size overflow"))?;
        if final_size > max_bytes {
            return Err(invalid_fs_data(format!(
                "compressed output exceeds {} byte limit (would be {} bytes)",
                max_bytes, final_size
            )));
        }
        out.resize(final_size, 0);
    }
    Ok(())
}

fn decompress_lznt1_chunk(chunk: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let chunk_start = out.len();
    let mut pos = 0usize;
    while pos < chunk.len() {
        let flags = chunk[pos];
        pos += 1;
        for bit in 0..8 {
            if pos >= chunk.len() {
                break;
            }
            if flags & (1 << bit) == 0 {
                out.push(chunk[pos]);
                pos += 1;
                continue;
            }
            if pos + 2 > chunk.len() {
                return Err("LZNT1 copy token truncated".to_string());
            }
            let token = u16::from_le_bytes([chunk[pos], chunk[pos + 1]]);
            pos += 2;
            let current = out.len().saturating_sub(chunk_start);
            let displacement_bits = lznt1_displacement_bits(current);
            let length_mask = (1u16 << displacement_bits) - 1;
            let length = (token & length_mask) as usize + 3;
            let displacement = (token >> displacement_bits) as usize + 1;
            if displacement > out.len().saturating_sub(chunk_start) {
                return Err("LZNT1 copy token points before chunk".to_string());
            }
            for _ in 0..length {
                let src = out.len() - displacement;
                let byte = out[src];
                out.push(byte);
            }
        }
    }
    Ok(())
}

fn lznt1_displacement_bits(current_chunk_output: usize) -> u16 {
    let mut length_bits = 12u16;
    let mut displacement = current_chunk_output.saturating_sub(1);
    while length_bits > 4 && displacement >= 0x10 {
        length_bits -= 1;
        displacement >>= 1;
    }
    length_bits
}
