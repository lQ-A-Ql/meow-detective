use std::cell::RefCell;

use super::{codec, format::WofAlgorithm, Result, WofError};
use crate::{
    attribute::{data_extents_declared_size, DataAttributeExtent},
    NtfsReader,
};

pub(crate) struct WofStream {
    algorithm: WofAlgorithm,
    pub(crate) logical_size: u64,
    extents: Vec<DataAttributeExtent>,
    compressed_size: u64,
    chunk_count: u64,
    table_bytes: u64,
    entry_width: usize,
    cached_chunk: RefCell<Option<(u64, Vec<u8>)>>,
}

impl WofStream {
    pub(crate) fn new(
        algorithm: WofAlgorithm,
        logical_size: u64,
        extents: Vec<DataAttributeExtent>,
        cluster_size: u64,
    ) -> std::io::Result<Self> {
        let compressed_size = data_extents_declared_size(&extents, cluster_size)?;
        if extents.is_empty() || extents.iter().any(|extent| {
            matches!(extent, DataAttributeExtent::NonResident {attr_flags, runs, ..} if attr_flags & 0xc001 != 0 || runs.iter().any(|run| run.lcn.is_none()))
        }) {
            return Err(WofError::Invalid("missing, sparse, encrypted or NTFS-compressed WOF ADS").into());
        }
        let chunk_size = algorithm.chunk_size();
        let chunk_count = logical_size.div_ceil(chunk_size);
        let entry_width = if logical_size < 1u64 << 32 { 4 } else { 8 };
        let table_bytes = chunk_count
            .saturating_sub(1)
            .checked_mul(entry_width as u64)
            .filter(|&bytes| bytes <= compressed_size)
            .ok_or(WofError::Invalid("WOF chunk table exceeds ADS"))?;
        if (logical_size == 0) != (compressed_size == 0) {
            return Err(WofError::Invalid("inconsistent empty WOF stream").into());
        }
        Ok(Self {
            algorithm,
            logical_size,
            extents,
            compressed_size,
            chunk_count,
            table_bytes,
            entry_width,
            cached_chunk: RefCell::new(None),
        })
    }

    pub(crate) fn read_range(
        &self,
        source: &NtfsReader,
        offset: u64,
        length: usize,
    ) -> std::io::Result<Vec<u8>> {
        self.range(source, offset, length).map_err(Into::into)
    }

    fn range(&self, source: &NtfsReader, offset: u64, length: usize) -> Result<Vec<u8>> {
        if offset >= self.logical_size || length == 0 {
            return Ok(Vec::new());
        }
        let length = (length as u64).min(self.logical_size - offset);
        if length > crate::MAX_BUFFERED_FILE_BYTES as u64 {
            return Err(WofError::Invalid(
                "requested WOF range exceeds buffering limit",
            ));
        }
        let end = offset
            .checked_add(length)
            .ok_or(WofError::Invalid("range overflow"))?;
        let size = self.algorithm.chunk_size();
        let mut output = Vec::new();
        output
            .try_reserve_exact(length as usize)
            .map_err(|_| WofError::Invalid("cannot allocate WOF range"))?;
        for index in offset / size..=(end - 1) / size {
            let chunk = self.chunk(source, index)?;
            let base = index * size;
            let start = offset.saturating_sub(base) as usize;
            let stop = (end - base).min(chunk.len() as u64) as usize;
            output.extend_from_slice(&chunk[start..stop]);
        }
        Ok(output)
    }

    fn chunk(&self, source: &NtfsReader, index: u64) -> Result<Vec<u8>> {
        if let Some((cached_index, bytes)) = self.cached_chunk.borrow().as_ref() {
            if *cached_index == index {
                return Ok(bytes.clone());
            }
        }
        let first = self.boundary(source, index)?;
        let next = self.boundary(source, index + 1)?;
        let expected = (self.logical_size - index * self.algorithm.chunk_size())
            .min(self.algorithm.chunk_size());
        let stored = next
            .checked_sub(first)
            .filter(|&size| size != 0 && size <= expected)
            .ok_or(WofError::Invalid("invalid or descending WOF chunk offsets"))?;
        let bytes = self.read_exact(source, self.table_bytes + first, stored as usize)?;
        let decoded = codec::decompress(&bytes, expected as usize, self.algorithm)?;
        *self.cached_chunk.borrow_mut() = Some((index, decoded.clone()));
        Ok(decoded)
    }

    fn boundary(&self, source: &NtfsReader, index: u64) -> Result<u64> {
        if index == 0 {
            return Ok(0);
        }
        if index == self.chunk_count {
            return Ok(self.compressed_size - self.table_bytes);
        }
        if index > self.chunk_count {
            return Err(WofError::Invalid("chunk index exceeds table"));
        }
        let position = (index - 1)
            .checked_mul(self.entry_width as u64)
            .ok_or(WofError::Invalid("chunk table offset overflow"))?;
        let bytes = self.read_exact(source, position, self.entry_width)?;
        let mut word = [0; 8];
        word[..self.entry_width].copy_from_slice(&bytes);
        let offset = u64::from_le_bytes(word);
        if offset > self.compressed_size - self.table_bytes {
            return Err(WofError::Invalid("chunk offset exceeds compressed payload"));
        }
        Ok(offset)
    }

    fn read_exact(&self, source: &NtfsReader, offset: u64, length: usize) -> Result<Vec<u8>> {
        let bytes = source.read_data_extents_range(&self.extents, offset, length)?;
        if bytes.len() != length {
            return Err(WofError::Invalid("truncated WOF ADS range"));
        }
        Ok(bytes)
    }
}
