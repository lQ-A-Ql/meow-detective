//! Read-only primitives for Ceph's little-endian wire encoding.

pub mod bluefs;
pub mod bluefs_transaction;
pub mod bluestore;
pub mod codec;
pub mod crc32c;
pub mod cursor;
pub mod error;

pub use bluefs::{
    decode_bluefs_super_block, BluefsExtent, BluefsFnode, BluefsLayout, BluefsSuper,
    BLUEFS_MAX_EXTENTS, BLUEFS_SUPER_BLOCK_SIZE, BLUEFS_SUPER_OFFSET,
};
pub use bluefs_transaction::{
    decode_bluefs_transaction, inspect_bluefs_transaction, BluefsFnodeDelta, BluefsOperation,
    BluefsTransaction, BluefsTransactionPrefix, BLUEFS_MAX_OPERATIONS, BLUEFS_MAX_OPERATION_BYTES,
};
pub use bluestore::{
    decode_bdev_label_block, select_bdev_label, select_bdev_labels, BdevLabel, BdevLabelCandidate,
    BdevLabelSelection, BDEV_FIRST_LABEL_POSITION, BDEV_LABEL_BLOCK_SIZE, BDEV_LABEL_MAGIC,
    BDEV_LABEL_POSITIONS, BDEV_LABEL_PREFIX_LENGTH,
};
pub use codec::{
    decode_lba_u64, decode_varint_lowz_u64, decode_varint_u64, CephDecode, CephEncode,
    CephStringMap, CephStructEnvelope, CephUtime,
};
pub use cursor::CephCursor;
pub use error::{CephWireError, Result};
