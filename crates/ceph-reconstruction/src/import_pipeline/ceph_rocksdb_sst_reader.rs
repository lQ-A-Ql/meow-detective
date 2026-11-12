use std::sync::atomic::{AtomicBool, Ordering};

use ceph_wire::BluefsFnode;
use transport::CommandError;

use super::ceph_bluefs_file_reader::{BluefsExtentReader, PreparedBluefsFile};

pub(super) struct BluefsSstRangeReader<'reader, 'evidence, 'cancel> {
    reader: &'reader mut BluefsExtentReader<'evidence>,
    file: PreparedBluefsFile,
    cancel_token: &'cancel AtomicBool,
}

impl<'reader, 'evidence, 'cancel> BluefsSstRangeReader<'reader, 'evidence, 'cancel> {
    pub(super) fn new(
        reader: &'reader mut BluefsExtentReader<'evidence>,
        fnode: &BluefsFnode,
        cancel_token: &'cancel AtomicBool,
    ) -> Result<Self, CommandError> {
        let file = reader.prepare_file(fnode)?;
        Ok(Self {
            reader,
            file,
            cancel_token,
        })
    }
}

impl rocksdb_wire::RangeReader for BluefsSstRangeReader<'_, '_, '_> {
    type Error = BluefsSstReadError;

    fn is_cancelled(&self) -> bool {
        self.cancel_token.load(Ordering::Relaxed)
    }

    fn read_range(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, Self::Error> {
        let length = u64::try_from(length)
            .map_err(|_| BluefsSstReadError("SST range length exceeds u64".to_string()))?;
        self.reader
            .read_prepared_file_range(&self.file, offset, length)
            .map_err(|error| BluefsSstReadError(error.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub(super) struct BluefsSstReadError(String);
