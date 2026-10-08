//! WIM LZX, independently framed in 32 KiB WOF chunks.
//! Format reference: Microsoft go-winio/wim/lzx (MIT); see fs-ntfs/NOTICE.
use super::{
    bits::Bits,
    codec::copy_match,
    lzx_trees::{read_trees, Trees},
    Result, WofError,
};

pub(super) fn decompress(data: &[u8], size: usize) -> Result<Vec<u8>> {
    let mut bits = Bits::new(data, false)?;
    let mut output = Vec::with_capacity(size);
    let mut recent = [1usize; 3];
    let mut main_lengths = [0; 496];
    let mut length_lengths = [0; 249];
    let mut raw_padding = false;
    while output.len() < size {
        if raw_padding {
            bits.raw(1)?;
        }
        let kind = bits.read(3)?;
        let length = if bits.read(1)? == 1 {
            32768
        } else {
            bits.read(16)? as usize
        };
        if length == 0 || length > size - output.len() {
            return Err(WofError::Invalid("LZX block exceeds output bounds"));
        }
        raw_padding = kind == 3 && !length.is_multiple_of(2);
        match kind {
            1 | 2 => {
                let trees =
                    read_trees(&mut bits, &mut main_lengths, &mut length_lengths, kind == 2)?;
                let end = output.len() + length;
                decode_block(&mut bits, &trees, &mut recent, &mut output, end)?;
            }
            3 => {
                bits.align_uncompressed()?;
                for offset in &mut recent {
                    *offset = bits.raw_u32()? as usize;
                }
                output.extend_from_slice(bits.raw(length)?);
            }
            _ => return Err(WofError::Invalid("unknown LZX block type")),
        }
    }
    undo_e8(&mut output);
    Ok(output)
}

fn decode_block(
    bits: &mut Bits<'_>,
    trees: &Trees,
    recent: &mut [usize; 3],
    output: &mut Vec<u8>,
    end: usize,
) -> Result<()> {
    while output.len() < end {
        let symbol = trees.main.symbol(bits)?;
        if symbol < 256 {
            output.push(symbol as u8);
            continue;
        }
        let token = symbol - 256;
        let mut length = token & 7;
        if length == 7 {
            length += trees.length.symbol(bits)?;
        }
        let distance = match_offset(bits, trees, recent, token >> 3)?;
        copy_match(output, distance, length + 2, end)?;
    }
    Ok(())
}

fn match_offset(
    bits: &mut Bits<'_>,
    trees: &Trees,
    recent: &mut [usize; 3],
    slot: usize,
) -> Result<usize> {
    if slot < 3 {
        let offset = recent[slot];
        recent.swap(slot, 0);
        return Ok(offset);
    }
    let extra = if slot < 4 { 0 } else { (slot - 2) / 2 } as u8;
    let base = if slot < 4 {
        slot
    } else {
        (2 + (slot & 1)) << extra
    };
    let suffix = match trees.aligned.as_ref() {
        Some(tree) if extra >= 3 => (bits.read(extra - 3)? as usize) * 8 + tree.symbol(bits)?,
        _ => bits.read(extra)? as usize,
    };
    let offset = base + suffix - 2;
    recent[2] = recent[1];
    recent[1] = recent[0];
    recent[0] = offset;
    Ok(offset)
}

fn undo_e8(data: &mut [u8]) {
    let mut position = 0;
    while position < data.len().saturating_sub(10) {
        if data[position] != 0xe8 {
            position += 1;
            continue;
        }
        let raw = &data[position + 1..position + 5];
        let absolute = i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let current = position as i32;
        if absolute >= -current && absolute < 12_000_000 {
            let relative = if absolute >= 0 {
                absolute - current
            } else {
                absolute + 12_000_000
            };
            data[position + 1..position + 5].copy_from_slice(&relative.to_le_bytes());
        }
        position += 5;
    }
}
