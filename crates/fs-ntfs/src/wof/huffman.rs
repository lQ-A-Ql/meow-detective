use super::{bits::Bits, Result, WofError};

/// Canonical tables have fixed, bounded storage and reject oversubscribed trees.
pub(super) struct Huffman {
    first_code: [u32; 17],
    first_symbol: [usize; 17],
    counts: [u32; 17],
    symbols: Vec<usize>,
}

impl Huffman {
    pub(super) fn new(lengths: &[u8]) -> Result<Self> {
        let mut table = Self {
            first_code: [0; 17],
            first_symbol: [0; 17],
            counts: [0; 17],
            symbols: Vec::new(),
        };
        for &length in lengths {
            if length > 16 {
                return Err(WofError::Invalid("Huffman length exceeds 16 bits"));
            }
            if length != 0 {
                table.counts[length as usize] += 1;
            }
        }
        let mut code = 0;
        for length in 1..=16 {
            code = (code + table.counts[length - 1]) << 1;
            if code + table.counts[length] > 1 << length {
                return Err(WofError::Invalid("oversubscribed Huffman tree"));
            }
            table.first_code[length] = code;
            table.first_symbol[length] = table.symbols.len();
            table.symbols.extend(
                lengths
                    .iter()
                    .enumerate()
                    .filter_map(|(symbol, &bits)| (usize::from(bits) == length).then_some(symbol)),
            );
        }
        if !table.symbols.is_empty() && table.first_code[16] + table.counts[16] != 1 << 16 {
            return Err(WofError::Invalid("incomplete Huffman tree"));
        }
        Ok(table)
    }

    pub(super) fn symbol(&self, bits: &mut Bits<'_>) -> Result<usize> {
        let mut code = 0;
        for length in 1..=16 {
            code = (code << 1) | bits.read(1)?;
            if let Some(index) = code.checked_sub(self.first_code[length]) {
                if index < self.counts[length] {
                    return Ok(self.symbols[self.first_symbol[length] + index as usize]);
                }
            }
        }
        Err(WofError::Invalid("missing Huffman symbol"))
    }
}
