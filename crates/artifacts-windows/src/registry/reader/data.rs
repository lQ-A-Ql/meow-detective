use super::{
    cells::{u16_at, u32_at},
    decode, BrowserError, BrowserOverlaySource, BrowserValue, HiveSnapshot, ValueRange,
};
use std::{borrow::Cow, collections::HashSet};

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
        let (bytes, data_source) = self.data_range(value, start, end)?;
        let bytes = bytes.into_owned();
        let partial = start != 0 || end != total;
        let decoded = decode::decode(value.type_code, &bytes, start, partial);
        let overlay_source = combine_sources(value.overlay_source, data_source);
        Ok(ValueRange {
            bytes,
            total_length: value.byte_length,
            decoded,
            partial,
            overlay_source,
        })
    }

    pub(crate) fn value_bytes(&self, value: &BrowserValue) -> Result<Cow<'_, [u8]>, BrowserError> {
        self.data_range(value, 0, value.byte_length as usize)
            .map(|(bytes, _)| bytes)
    }

    fn data_range(
        &self,
        value: &BrowserValue,
        start: usize,
        end: usize,
    ) -> Result<(Cow<'_, [u8]>, BrowserOverlaySource), BrowserError> {
        let total = value.byte_length as usize;
        if start > end || end > total || total > self.bytes.len() {
            return Err(BrowserError::Corrupt);
        }
        if total == 0 {
            return Ok((Cow::Borrowed(&[]), value.overlay_source));
        }
        if value.inline {
            if total > 4 {
                return Err(BrowserError::Corrupt);
            }
            let at = 4096 + value.offset as usize + 12;
            return Ok((
                Cow::Owned(value.data.to_le_bytes()[start..end].to_vec()),
                self.overlay_for_range(at + start, at + end),
            ));
        }
        let cell = self.cell(value.data)?;
        if self.minor > 3 && total > SEGMENT_BYTES {
            let (bytes, source) = self.segment_range(cell, total, start, end)?;
            Ok((Cow::Owned(bytes), source))
        } else {
            let at = 4096 + value.data as usize + 4;
            Ok((
                Cow::Borrowed(
                    cell.get(start..end)
                        .filter(|_| cell.len() >= total)
                        .ok_or(BrowserError::Corrupt)?,
                ),
                self.overlay_for_range(at + start, at + end),
            ))
        }
    }

    fn segment_range(
        &self,
        cell: &[u8],
        total: usize,
        start: usize,
        end: usize,
    ) -> Result<(Vec<u8>, BrowserOverlaySource), BrowserError> {
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
        let mut source = None;
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
                let at = 4096 + offset as usize + 4;
                let current = self.overlay_for_range(at + from, at + to);
                source =
                    Some(source.map_or(current, |previous| combine_sources(previous, current)));
            }
        }
        Ok((output, source.unwrap_or(BrowserOverlaySource::Base)))
    }
}

fn combine_sources(
    left: BrowserOverlaySource,
    right: BrowserOverlaySource,
) -> BrowserOverlaySource {
    match (left, right) {
        (BrowserOverlaySource::Base, BrowserOverlaySource::Base) => BrowserOverlaySource::Base,
        (BrowserOverlaySource::Recovered, BrowserOverlaySource::Recovered) => {
            BrowserOverlaySource::Recovered
        }
        _ => BrowserOverlaySource::Merged,
    }
}
