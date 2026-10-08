//! Bounded XPRESS Huffman chunk decoding, as specified by MS-XCA 2.2.4.
use super::{bits::Bits, codec::copy_match, huffman::Huffman, Result, WofError};

pub(super) fn decompress(data: &[u8], size: usize) -> Result<Vec<u8>> {
    let table = data
        .get(..256)
        .ok_or(WofError::Invalid("truncated XPRESS Huffman table"))?;
    let lengths: Vec<_> = table
        .iter()
        .flat_map(|byte| [byte & 15, byte >> 4])
        .collect();
    let tree = Huffman::new(&lengths)?;
    let mut bits = Bits::new(&data[256..], true)?;
    let mut output = Vec::with_capacity(size);
    while output.len() < size {
        let symbol = tree.symbol(&mut bits)?;
        if symbol < 256 {
            output.push(symbol as u8);
            continue;
        }
        let token = symbol - 256;
        let offset_bits = (token >> 4) as u8;
        let distance = (1usize << offset_bits) + bits.peek(offset_bits)? as usize;
        let length = match_length(&mut bits, token & 15)?;
        // Extended lengths precede the refill caused by consuming offset bits.
        bits.read(offset_bits)?;
        copy_match(&mut output, distance, length, size)?;
    }
    Ok(output)
}

fn match_length(bits: &mut Bits<'_>, nibble: usize) -> Result<usize> {
    let mut length = nibble as u32;
    if length == 15 {
        length += bits.raw_u8()?;
        if length == 270 {
            length = bits.raw_u16()?;
            if length == 0 {
                length = bits.raw_u32()?;
            }
            if length < 15 {
                return Err(WofError::Invalid("invalid extended XPRESS length"));
            }
        }
    }
    usize::try_from(length)
        .ok()
        .and_then(|value| value.checked_add(3))
        .ok_or(WofError::Invalid("XPRESS match length overflow"))
}
