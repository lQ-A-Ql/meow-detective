use super::{bits::Bits, huffman::Huffman, Result, WofError};

pub(super) struct Trees {
    pub(super) main: Huffman,
    pub(super) length: Huffman,
    pub(super) aligned: Option<Huffman>,
}

pub(super) fn read_trees(
    bits: &mut Bits<'_>,
    main: &mut [u8; 496],
    lengths: &mut [u8; 249],
    aligned: bool,
) -> Result<Trees> {
    let aligned = if aligned {
        let mut codes = [0; 8];
        for code in &mut codes {
            *code = bits.read(3)? as u8;
        }
        Some(Huffman::new(&codes)?)
    } else {
        None
    };
    read_lengths(bits, &mut main[..256])?;
    read_lengths(bits, &mut main[256..])?;
    read_lengths(bits, lengths)?;
    Ok(Trees {
        main: Huffman::new(main)?,
        length: Huffman::new(lengths)?,
        aligned,
    })
}

fn read_lengths(bits: &mut Bits<'_>, lengths: &mut [u8]) -> Result<()> {
    let mut codes = [0; 20];
    for code in &mut codes {
        *code = bits.read(4)? as u8;
    }
    let pretree = Huffman::new(&codes)?;
    let mut position = 0;
    while position < lengths.len() {
        let symbol = pretree.symbol(bits)?;
        let (count, value) = match symbol {
            0..=16 => (1, (usize::from(lengths[position]) + 17 - symbol) % 17),
            17 => (bits.read(4)? as usize + 4, 0),
            18 => (bits.read(5)? as usize + 20, 0),
            19 => {
                let count = bits.read(1)? as usize + 4;
                let delta = pretree.symbol(bits)?;
                if delta > 16 {
                    return Err(WofError::Invalid("invalid LZX pretree delta"));
                }
                (count, (usize::from(lengths[position]) + 17 - delta) % 17)
            }
            _ => return Err(WofError::Invalid("invalid LZX pretree symbol")),
        };
        let end = position
            .checked_add(count)
            .filter(|&end| end <= lengths.len())
            .ok_or(WofError::Invalid("LZX code-length run exceeds tree"))?;
        lengths[position..end].fill(value as u8);
        position = end;
    }
    Ok(())
}
