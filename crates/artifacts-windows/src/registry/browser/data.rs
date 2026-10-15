use super::{
    cells::{u16_at, u32_at},
    decode, BrowserError, BrowserOverlaySource, BrowserValue, HiveSnapshot, ValueRange,
};
use std::collections::HashSet;

const SEGMENT_BYTES: usize = 16_344;
impl HiveSnapshot {
    pub fn value_range(
        &self,
        value: &BrowserValue,
        offset: u64,
        length: usize,
    ) -> Result<ValueRange, BrowserError> {
        if length == 0 || length > 65536 || offset > value.byte_length as u64 {
            return Err(BrowserError::Limit);
        }
        let start = offset as usize;
        let total = value.byte_length as usize;
        let end = start.saturating_add(length).min(total);
        let bytes = if total == 0 {
            Vec::new()
        } else if value.inline {
            if total > 4 {
                return Err(BrowserError::Corrupt);
            }
            value.data.to_le_bytes()[start..end].to_vec()
        } else {
            let cell = self.cell(value.data)?;
            if self.minor > 3 && total > SEGMENT_BYTES {
                self.segment_range(cell, total, start, end)?
            } else {
                if cell.len() < total {
                    return Err(BrowserError::Corrupt);
                }
                cell[start..end].to_vec()
            }
        };
        let partial = start != 0 || end != total;
        let decoded = decode::decode(value.type_code, &bytes, start, partial);
        let overlay_source = self.overlay_for_cell(value.offset, 20 + value.name.len() * 2);
        let data_source = if value.inline {
            self.overlay_for_cell(value.offset, 20 + value.name.len() * 2)
        } else {
            self.overlay_for_range(
                4096usize.saturating_add(value.data as usize),
                4096usize
                    .saturating_add(value.data as usize)
                    .saturating_add(total + 4),
            )
        };
        let overlay_source = match (overlay_source, data_source) {
            (BrowserOverlaySource::Base, BrowserOverlaySource::Recovered)
            | (BrowserOverlaySource::Recovered, BrowserOverlaySource::Base)
            | (BrowserOverlaySource::Merged, _)
            | (_, BrowserOverlaySource::Merged) => BrowserOverlaySource::Merged,
            (BrowserOverlaySource::Recovered, BrowserOverlaySource::Recovered) => {
                BrowserOverlaySource::Recovered
            }
            _ => BrowserOverlaySource::Base,
        };
        Ok(ValueRange {
            bytes,
            total_length: value.byte_length,
            decoded,
            partial,
            overlay_source,
        })
    }

    fn segment_range(
        &self,
        cell: &[u8],
        total: usize,
        start: usize,
        end: usize,
    ) -> Result<Vec<u8>, BrowserError> {
        if cell.get(..2) != Some(b"db") {
            return Err(BrowserError::Corrupt);
        }
        let count = u16_at(cell, 2)? as usize;
        let needed = total.div_ceil(SEGMENT_BYTES);
        if count != needed {
            return Err(BrowserError::Corrupt);
        }
        let list = self.cell(u32_at(cell, 4)?)?;
        if list.len() < count * 4 {
            return Err(BrowserError::Corrupt);
        }
        let mut output = Vec::with_capacity(end - start);
        let mut seen = HashSet::new();
        // Validate all segment ownership and lengths even for a small range.
        for index in 0..count {
            let offset = u32_at(list, index * 4)?;
            if !seen.insert(offset) {
                return Err(BrowserError::Corrupt);
            }
            let data = self.cell(offset)?;
            let base = index * SEGMENT_BYTES;
            let size = SEGMENT_BYTES.min(total - base);
            if data.len() < size {
                return Err(BrowserError::Corrupt);
            }
            let from = start.saturating_sub(base).min(size);
            let to = end.saturating_sub(base).min(size);
            if from < to {
                output.extend_from_slice(&data[from..to]);
            }
        }
        Ok(output)
    }
}
