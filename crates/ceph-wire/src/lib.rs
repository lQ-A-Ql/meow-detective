//! Read-only primitives for Ceph's little-endian wire encoding.

pub mod bluestore;
pub mod codec;
pub mod crc32c;
pub mod cursor;
pub mod error;

pub use bluestore::{
    decode_bdev_label_block, select_bdev_label, select_bdev_labels, BdevLabel, BdevLabelCandidate,
    BdevLabelSelection, BDEV_FIRST_LABEL_POSITION, BDEV_LABEL_BLOCK_SIZE, BDEV_LABEL_MAGIC,
    BDEV_LABEL_POSITIONS, BDEV_LABEL_PREFIX_LENGTH,
};
pub use codec::{CephDecode, CephEncode, CephStringMap, CephStructEnvelope, CephUtime};
pub use cursor::CephCursor;
pub use error::{CephWireError, Result};
