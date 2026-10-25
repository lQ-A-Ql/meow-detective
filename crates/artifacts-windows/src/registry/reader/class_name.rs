use super::{BrowserError, BrowserKey, HiveSnapshot};

impl HiveSnapshot {
    pub(crate) fn class_name(&self, key: &BrowserKey) -> Result<Option<String>, BrowserError> {
        let length = usize::from(key.class_length);
        if length == 0 {
            return Ok(None);
        }
        if length > 4096 || !length.is_multiple_of(2) {
            return Err(BrowserError::Corrupt);
        }
        let cell = self.cell(key.class_data)?;
        let data = cell.get(..length).ok_or(BrowserError::Corrupt)?;
        let units = data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| u16::from_le_bytes(*chunk))
            .collect::<Vec<_>>();
        let value = String::from_utf16_lossy(&units);
        let trimmed = value.trim_end_matches('\0');
        Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
    }
}
