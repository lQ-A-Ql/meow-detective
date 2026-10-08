#[path = "wof_xpress.rs"]
mod wof_xpress;
pub use wof_xpress::xpress;

struct Writer {
    data: Vec<u8>,
    word: u16,
    bits: u8,
}
impl Writer {
    fn new() -> Self {
        Self {
            data: Vec::new(),
            word: 0,
            bits: 0,
        }
    }
    fn put(&mut self, value: usize, count: u8) {
        for shift in (0..count).rev() {
            self.word = (self.word << 1) | ((value >> shift) & 1) as u16;
            self.bits += 1;
            if self.bits == 16 {
                self.flush();
            }
        }
    }
    fn flush(&mut self) {
        self.data.extend_from_slice(&self.word.to_le_bytes());
        self.word = 0;
        self.bits = 0;
    }
    fn align(&mut self) {
        if self.bits != 0 {
            self.word <<= 16 - self.bits;
            self.flush();
        }
    }
}

fn lengths(writer: &mut Writer, count: usize, active: &[usize]) {
    // Pretree: delta 0 has code 0, delta 16 code 10, zero run 18 code 11.
    for symbol in 0..20 {
        writer.put(
            match symbol {
                0 => 1,
                16 | 18 => 2,
                _ => 0,
            },
            4,
        );
    }
    let mut position = 0;
    while position < count {
        if active.contains(&position) {
            writer.put(2, 2);
            position += 1;
            continue;
        }
        let stop = active
            .iter()
            .copied()
            .find(|&symbol| symbol > position)
            .unwrap_or(count);
        let remaining = stop - position;
        if remaining >= 20 {
            let run = remaining.min(51);
            writer.put(3, 2);
            writer.put(run - 20, 5);
            position += run;
        } else {
            writer.put(0, 1);
            position += 1;
        }
    }
}

/// A WIM-LZX frame with one optional raw block and a repeating verbatim/aligned block.
pub fn lzx(size: usize, aligned: bool, raw_prefix: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut writer = Writer::new();
    let mut expected = raw_prefix.to_vec();
    if !raw_prefix.is_empty() {
        writer.put(3, 3);
        writer.put(0, 1);
        writer.put(raw_prefix.len(), 16);
        writer.align();
        for _ in 0..3 {
            writer.data.extend_from_slice(&1u32.to_le_bytes());
        }
        writer.data.extend_from_slice(raw_prefix);
        if !raw_prefix.len().is_multiple_of(2) {
            writer.data.push(0);
        }
    }
    let count = size - raw_prefix.len();
    writer.put(if aligned { 2 } else { 1 }, 3);
    writer.put(usize::from(count == 32768), 1);
    if count != 32768 {
        writer.put(count, 16);
    }
    if aligned {
        for _ in 0..8 {
            writer.put(3, 3);
        }
    }
    lengths(&mut writer, 256, &[65]);
    lengths(&mut writer, 240, &[7]);
    lengths(&mut writer, 249, &[247, 248]);
    writer.put(0, 1); // Initial 'A'.
    let mut remaining = count - 1;
    while remaining >= 257 {
        writer.put(1, 1);
        writer.put(1, 1);
        remaining -= 257;
    }
    for _ in 0..remaining {
        writer.put(0, 1);
    }
    writer.align();
    expected.resize(size, b'A');
    // A raw block still carries the WIM E8 transform inside the LZX framing.
    if expected.len() > 10 {
        let mut position = 0;
        while position < expected.len() - 10 {
            if expected[position] != 0xe8 {
                position += 1;
                continue;
            }
            let absolute =
                i32::from_le_bytes(expected[position + 1..position + 5].try_into().unwrap());
            if absolute >= -(position as i32) && absolute < 12_000_000 {
                let relative = if absolute >= 0 {
                    absolute - position as i32
                } else {
                    absolute + 12_000_000
                };
                expected[position + 1..position + 5].copy_from_slice(&relative.to_le_bytes());
            }
            position += 5;
        }
    }
    (writer.data, expected)
}
