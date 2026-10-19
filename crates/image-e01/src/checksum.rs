//! Incremental EWF Adler-32 checksums without retaining an offset array.
use sha2::{Digest, Sha256};

pub(crate) fn page_digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Clone, Copy)]
pub(crate) struct Adler32(u32, u32);

impl Adler32 {
    pub fn new() -> Self {
        Self(1, 0)
    }
    pub fn update(&mut self, bytes: &[u8]) {
        for block in bytes.chunks(5552) {
            for byte in block {
                self.0 += u32::from(*byte);
                self.1 += self.0;
            }
            self.0 %= 65521;
            self.1 %= 65521;
        }
    }
    pub fn finish(self) -> u32 {
        (self.1 << 16) | self.0
    }
}

pub(crate) fn adler32(bytes: &[u8]) -> u32 {
    let mut checksum = Adler32::new();
    checksum.update(bytes);
    checksum.finish()
}
