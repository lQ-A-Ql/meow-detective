use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlockDeviceError {
    #[error("image I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("block size must be a non-zero power of two")]
    InvalidBlockSize,
    #[error("image size {size} is not aligned to block size {block_size}")]
    UnalignedImageSize { size: u64, block_size: u32 },
    #[error("image contains no addressable blocks")]
    EmptyImage,
    #[error("block request arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("block request [{offset}, {end}) exceeds image size {size}")]
    OutOfBounds { offset: u64, end: u64, size: u64 },
    #[error("block request of {requested} bytes exceeds the {maximum}-byte limit")]
    RequestTooLarge { requested: u64, maximum: usize },
    #[error("image reader lock is poisoned")]
    LockPoisoned,
}
