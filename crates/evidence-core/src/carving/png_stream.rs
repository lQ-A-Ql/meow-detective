//! Incremental zlib validation without allocating the decoded raster.
use flate2::{Decompress, FlushDecompress, Status};

pub(super) struct Raster {
    decoder: Decompress,
    stride: u64,
    expected: u64,
    ended: bool,
    pub palette_allowed: bool,
}
impl Raster {
    pub fn new(ihdr: &[u8; 13]) -> Result<Self, &'static str> {
        let width = u32::from_be_bytes(ihdr[..4].try_into().map_err(|_| "invalidHeader")?);
        let height = u32::from_be_bytes(ihdr[4..8].try_into().map_err(|_| "invalidHeader")?);
        if width == 0 || height == 0 || width > 0x7fff_ffff || height > 0x7fff_ffff {
            return Err("invalidDimensions");
        }
        if ihdr[8] != 8 || ihdr[10] != 0 || ihdr[11] != 0 || ihdr[12] != 0 {
            return Err("unsupportedImageLayout");
        }
        let channels = match ihdr[9] {
            0 => 1,
            2 => 3,
            4 => 2,
            6 => 4,
            _ => return Err("unsupportedImageLayout"),
        };
        let stride = u64::from(width) * channels + 1;
        let expected = stride
            .checked_mul(u64::from(height))
            .ok_or("decodedLimit")?;
        if expected > 128 * 1024 * 1024 {
            return Err("decodedLimit");
        }
        Ok(Self {
            decoder: Decompress::new(true),
            stride,
            expected,
            ended: false,
            palette_allowed: matches!(ihdr[9], 2 | 6),
        })
    }
    pub fn consume(
        &mut self,
        mut input: &[u8],
        output_budget: &mut u64,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<(), &'static str> {
        if self.ended {
            return if input.is_empty() {
                Ok(())
            } else {
                Err("trailingCompressedData")
            };
        }
        let mut output = [0u8; 65536];
        loop {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                return Err("cancelled");
            }
            let old_in = self.decoder.total_in();
            let old_out = self.decoder.total_out();
            let status = self
                .decoder
                .decompress(input, &mut output, FlushDecompress::None)
                .map_err(|_| "invalidCompression")?;
            let consumed = (self.decoder.total_in() - old_in) as usize;
            let produced = (self.decoder.total_out() - old_out) as usize;
            if self.decoder.total_out() > self.expected {
                return Err("decodedSizeMismatch");
            }
            if produced as u64 > *output_budget {
                return Err("validationBudget");
            }
            *output_budget -= produced as u64;
            let first = ((self.stride - old_out % self.stride) % self.stride) as usize;
            if (first..produced)
                .step_by(self.stride as usize)
                .any(|i| output[i] > 4)
            {
                return Err("invalidFilter");
            }
            input = &input[consumed..];
            if status == Status::StreamEnd {
                self.ended = true;
                return if input.is_empty() {
                    Ok(())
                } else {
                    Err("trailingCompressedData")
                };
            }
            if consumed == 0 && produced == 0 {
                return if input.is_empty() {
                    Ok(())
                } else {
                    Err("invalidCompression")
                };
            }
        }
    }
    pub fn complete(&self) -> bool {
        self.ended && self.decoder.total_out() == self.expected
    }
}
