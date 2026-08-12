use super::super::XfsLogError;
use super::{
    XfsBufferReplay, XfsInodeReplay, XfsReplayAction, XfsReplayPatch, MAX_REPLAY_PATCH_BYTES,
};

/// The kernel's `l_buf_cancel_table`: exact (blkno, len) basic-block ranges
/// with a reference count, one entry per logged cancellation.
#[derive(Default)]
pub(super) struct CancelTable {
    entries: Vec<(u64, u32, u32)>,
}

impl CancelTable {
    pub(super) fn add(&mut self, blkno: u64, len: u32) {
        match self
            .entries
            .iter_mut()
            .find(|entry| entry.0 == blkno && entry.1 == len)
        {
            Some(entry) => entry.2 = entry.2.saturating_add(1),
            None => self.entries.push((blkno, len, 1)),
        }
    }

    pub(super) fn contains(&self, blkno: u64, len: u32) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.0 == blkno && entry.1 == len && entry.2 > 0)
    }

    pub(super) fn remove_one(&mut self, blkno: u64, len: u32) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.0 == blkno && entry.1 == len)
        {
            self.entries[index].2 = self.entries[index].2.saturating_sub(1);
            if self.entries[index].2 == 0 {
                self.entries.remove(index);
            }
        }
    }

    pub(super) fn overlaps(&self, blkno: u64, len: u64) -> bool {
        let end = blkno.saturating_add(len);
        self.entries.iter().any(|entry| {
            let entry_end = entry.0.saturating_add(u64::from(entry.1));
            entry.2 > 0 && blkno < entry_end && entry.0 < end
        })
    }
}

pub(super) struct PatchSink {
    pub(super) actions: Vec<XfsReplayAction>,
    total_bytes: u64,
    pub(super) capacity: u64,
}

impl PatchSink {
    pub(super) fn new(capacity: u64) -> Self {
        Self {
            actions: Vec::new(),
            total_bytes: 0,
            capacity,
        }
    }

    /// Queue one write; returns `false` when the range lies outside the
    /// filesystem, which the caller turns into a skipped item.
    pub(super) fn push(&mut self, offset: u64, bytes: Vec<u8>) -> Result<bool, XfsLogError> {
        let Some(end) = offset.checked_add(bytes.len() as u64) else {
            return Ok(false);
        };
        if end > self.capacity {
            return Ok(false);
        }
        self.reserve(bytes.len())?;
        self.actions
            .push(XfsReplayAction::Patch(XfsReplayPatch { offset, bytes }));
        Ok(true)
    }

    pub(super) fn push_buffer(&mut self, replay: XfsBufferReplay) -> Result<(), XfsLogError> {
        self.reserve(replay.length)?;
        self.actions.push(XfsReplayAction::Buffer(replay));
        Ok(())
    }

    pub(super) fn push_inode(&mut self, replay: XfsInodeReplay) -> Result<(), XfsLogError> {
        self.reserve(replay.length)?;
        self.actions.push(XfsReplayAction::Inode(replay));
        Ok(())
    }

    fn reserve(&mut self, bytes: usize) -> Result<(), XfsLogError> {
        self.total_bytes = self.total_bytes.saturating_add(bytes as u64);
        if self.total_bytes > MAX_REPLAY_PATCH_BYTES {
            return Err(XfsLogError::InvalidData(format!(
                "replay patch bytes exceed the {MAX_REPLAY_PATCH_BYTES} byte cap"
            )));
        }
        Ok(())
    }
}
