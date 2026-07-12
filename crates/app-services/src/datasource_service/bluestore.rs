use std::io::{ErrorKind, Read, Seek, SeekFrom};

use super::Result;

const GIB: u64 = 1024 * 1024 * 1024;
// Keep these aligned with Ceph's ceph-volume util/disk.py detector.
const BLUESTORE_LABEL_SIGNATURE: &[u8] = b"bluestore block device";
const BLUESTORE_LABEL_OFFSETS: [u64; 5] = [0, GIB, 10 * GIB, 100 * GIB, 1000 * GIB];

/// Detect Ceph BlueStore labels without treating an OSD block device as a
/// mountable filesystem.
pub(crate) fn has_bluestore_label<R>(reader: &mut R) -> Result<bool>
where
    R: Read + Seek + ?Sized,
{
    has_bluestore_label_at(reader, 0)
}

pub(super) fn has_bluestore_label_at<R>(reader: &mut R, device_offset: u64) -> Result<bool>
where
    R: Read + Seek + ?Sized,
{
    let mut signature = [0u8; BLUESTORE_LABEL_SIGNATURE.len()];
    for relative_offset in BLUESTORE_LABEL_OFFSETS {
        let Some(offset) = device_offset.checked_add(relative_offset) else {
            continue;
        };
        if reader.seek(SeekFrom::Start(offset)).is_err() {
            continue;
        }
        match reader.read_exact(&mut signature) {
            Ok(()) if signature == BLUESTORE_LABEL_SIGNATURE => return Ok(true),
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(false)
}
