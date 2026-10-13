use super::{
    cells::{name, u16_at, u32_at},
    decode, BrowserError, BrowserKey, BrowserPage, BrowserValue, HiveSnapshot,
};

impl HiveSnapshot {
    pub fn values(
        &self,
        key: &BrowserKey,
        start: usize,
        limit: usize,
    ) -> Result<BrowserPage<BrowserValue>, BrowserError> {
        if key.value_count == 0 {
            return Ok(BrowserPage {
                declared: 0,
                skipped: 0,
                items: Vec::new(),
                next: None,
                diagnostic: None,
            });
        }
        if key.value_count > 100_000 {
            return Err(BrowserError::Limit);
        }
        let list = self.cell(key.values)?;
        let available = (list.len() / 4).min(key.value_count as usize);
        if start > available {
            return Err(BrowserError::Limit);
        }
        let end = start
            .checked_add(limit)
            .ok_or(BrowserError::Limit)?
            .min(available);
        let mut items = Vec::new();
        for index in start..end {
            let offset = u32_at(list, index * 4)?;
            items.push((offset, self.value(offset)));
        }
        Ok(BrowserPage {
            declared: key.value_count,
            skipped: if end == available {
                key.value_count.saturating_sub(available as u32)
            } else {
                0
            },
            items,
            next: (end < available).then_some(end),
            diagnostic: (available != key.value_count as usize).then_some(BrowserError::Corrupt),
        })
    }

    pub fn value(&self, offset: u32) -> Result<BrowserValue, BrowserError> {
        let cell = self.cell(offset)?;
        if cell.len() < 20 || cell.get(..2) != Some(b"vk") {
            return Err(BrowserError::Corrupt);
        }
        let length = u16_at(cell, 2)? as usize;
        let raw = u32_at(cell, 4)?;
        let mut value = BrowserValue {
            offset,
            name: name(
                cell.get(20..20 + length).ok_or(BrowserError::Corrupt)?,
                u16_at(cell, 16)? & 1 != 0,
            )?,
            type_code: u32_at(cell, 12)?,
            byte_length: raw & 0x7fff_ffff,
            inline: raw & 0x8000_0000 != 0,
            data: u32_at(cell, 8)?,
            preview: String::new(),
            data_error: None,
        };
        match self.value_range(&value, 0, 256) {
            Ok(range) => {
                value.preview = range
                    .decoded
                    .unwrap_or_else(|| hex::encode(&range.bytes[..range.bytes.len().min(32)]));
            }
            Err(error) => value.data_error = Some(error),
        }
        Ok(value)
    }

    pub fn value_type_name(code: u32) -> String {
        decode::type_name(code)
    }
}
